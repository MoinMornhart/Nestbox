import { useCallback, useEffect, useRef, useState } from "react";
import { api, inTauri } from "./lib/api";
import type { AppError, HostInfo, Settings, Vm } from "./lib/types";
import { Button } from "./components/ui";
import { ErrorPanel, ToastProvider } from "./components/feedback";
import { NestboxLogo } from "./components/Logo";
import { Setup } from "./screens/Setup";
import { Dashboard, type OpenDialog } from "./screens/Dashboard";
import { Wizard, type WizardInit } from "./screens/wizard/Wizard";

type View = "loading" | "setup" | "dashboard" | "wizard";

// Nur im Browser-Mock: Startansicht per URL (für Vorschaubilder), z. B. ?view=wizard&step=2
const params = new URLSearchParams(location.search);
const forcedView = !inTauri ? (params.get("view") as View | null) : null;
const wizardInit: WizardInit | undefined =
  forcedView === "wizard"
    ? { step: Number(params.get("step") ?? 0), osId: params.get("os") ?? undefined, iso: params.get("iso") ?? undefined, advanced: params.get("advanced") === "1" }
    : undefined;
const initialDialog: OpenDialog = (() => {
  const d = !inTauri ? params.get("dialog") : null;
  if (!d) return null;
  if (d === "app-settings") return { kind: "app-settings" };
  const [kind, id] = d.split(":");
  return { kind: kind as "snapshots", id };
})();

const POLL_MS = 2000;

export default function App() {
  const [view, setView] = useState<View>("loading");
  const [host, setHost] = useState<HostInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [fatal, setFatal] = useState<AppError | null>(null);
  const [checking, setChecking] = useState(false);
  const [vms, setVms] = useState<Vm[]>([]);
  const [loaded, setLoaded] = useState(false);

  const recheck = useCallback(async () => {
    setChecking(true);
    try {
      const [h, s] = await Promise.all([api.hostInfo(), api.getSettings()]);
      setHost(h);
      setSettings(s);
      return { h, s };
    } finally {
      setChecking(false);
    }
  }, []);

  // Start: Einrichtung prüfen und passende Ansicht wählen
  useEffect(() => {
    (async () => {
      try {
        const { h, s } = await recheck();
        const ready = h.activeBackend === "hyperv" ? h.hypervReady : h.qemuReady;
        setView(forcedView ?? (s.setupDone && ready ? "dashboard" : "setup"));
      } catch (e) {
        setFatal(e as AppError);
      }
    })();
  }, [recheck]);

  const refresh = useCallback(async () => {
    try {
      setVms(await api.listVms());
    } catch {
      /* Beim nächsten Durchlauf erneut versuchen */
    } finally {
      setLoaded(true);
    }
  }, []);

  // Status alle 2 Sekunden aktualisieren (nur wenn das Fenster sichtbar ist)
  const polling = useRef(false);
  useEffect(() => {
    if (view !== "dashboard" && view !== "wizard") return;
    let stop = false;
    let timer: number;
    void refresh();
    const tick = async () => {
      if (stop) return;
      if (!document.hidden && !polling.current) {
        polling.current = true;
        try {
          const list = await api.vmStatus();
          setVms((prev) => prev.map((v) => ({ ...v, status: list.find((s) => s.id === v.id) ?? v.status })));
        } catch {
          /* ignorieren – nächster Versuch in 2 s */
        } finally {
          polling.current = false;
        }
      }
      timer = window.setTimeout(tick, POLL_MS);
    };
    timer = window.setTimeout(tick, POLL_MS);
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  }, [view, refresh]);

  const saveSettings = async (s: Settings) => {
    const saved = await api.saveSettings(s);
    setSettings(saved);
    setHost(await api.hostInfo());
  };

  if (fatal) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div className="w-full max-w-lg">
          <ErrorPanel error={fatal} />
          <Button className="mt-4" onClick={() => location.reload()}>
            Erneut versuchen
          </Button>
        </div>
      </div>
    );
  }

  if (view === "loading" || !host || !settings) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 anim-fade">
        <div className="anim-pulse">
          <NestboxLogo size={56} />
        </div>
        <div className="text-[13.5px] text-muted">Dein PC wird geprüft …</div>
      </div>
    );
  }

  return (
    <ToastProvider>
      {view === "setup" && (
        <Setup
          host={host}
          settings={settings}
          checking={checking}
          onRecheck={async () => {
            await recheck();
          }}
          onSettings={saveSettings}
          onDone={async () => {
            await saveSettings({ ...settings, setupDone: true });
            setView("dashboard");
          }}
        />
      )}
      {view === "dashboard" && (
        <Dashboard
          host={host}
          settings={settings}
          vms={vms}
          loaded={loaded}
          refresh={refresh}
          onNew={() => setView("wizard")}
          onSaveSettings={saveSettings}
          onOpenSetup={() => {
            void recheck();
            setView("setup");
          }}
          initialDialog={initialDialog}
        />
      )}
      {view === "wizard" && (
        <Wizard
          host={host}
          settings={settings}
          vms={vms}
          init={wizardInit}
          onClose={() => {
            setView("dashboard");
            void refresh();
          }}
          onCreated={() => void refresh()}
        />
      )}
    </ToastProvider>
  );
}
