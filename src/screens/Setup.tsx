import { useState, type ReactNode } from "react";
import { AlertTriangle, ArrowRight, Check, ChevronRight, Cpu, ExternalLink, FolderOpen, Info as InfoIcon, RefreshCw, RotateCcw, ShieldCheck, X } from "lucide-react";
import { api, openLink, pickFolder } from "../lib/api";
import type { AppError, HostInfo, Settings } from "../lib/types";
import { Button, Dialog, cx, Info } from "../components/ui";
import { ErrorPanel } from "../components/feedback";
import { NestboxLogo } from "../components/Logo";

type CheckState = "ok" | "warn" | "error" | "info";

interface Check {
  id: string;
  title: string;
  state: CheckState;
  text: ReactNode;
  action?: ReactNode;
  steps?: string[];
}

const QEMU_URL = "https://www.qemu.org/download/#windows";

export function Setup({
  host,
  settings,
  onRecheck,
  onSettings,
  onDone,
  checking,
}: {
  host: HostInfo;
  settings: Settings;
  onRecheck: () => Promise<void>;
  onSettings: (s: Settings) => Promise<void>;
  onDone: () => void;
  checking: boolean;
}) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [confirmRestart, setConfirmRestart] = useState(false);
  const hv = host.activeBackend === "hyperv";
  const ready = hv ? host.hypervReady : host.qemuReady;

  const run = async (id: string, fn: () => Promise<unknown>) => {
    setBusy(id);
    setError(null);
    try {
      await fn();
      await onRecheck();
    } catch (e) {
      setError(e as AppError);
    } finally {
      setBusy(null);
    }
  };

  const biosSteps = [
    "PC neu starten und beim Hochfahren wiederholt die BIOS-Taste drücken (meist Entf, F2 oder F10).",
    "Im BIOS/UEFI nach „Intel VT-x“, „Intel Virtualization Technology“, „AMD-V“ oder „SVM Mode“ suchen (oft unter „Advanced“ oder „CPU Configuration“).",
    "Die Option auf „Enabled“ stellen, speichern (meist F10) und neu starten.",
  ];

  const checks: Check[] = [];
  checks.push({
    id: "edition",
    title: host.windowsName,
    state: hv && host.isHome ? "error" : "ok",
    text: hv
      ? host.isHome
        ? "Windows Home enthält kein Hyper-V. Nutze stattdessen QEMU (unten umschalten)."
        : "Enthält Hyper-V – Nestbox nutzt die Virtualisierung von Windows selbst."
      : host.isHome
        ? "Windows Home hat kein Hyper-V – Nestbox nutzt dafür QEMU."
        : "Nestbox nutzt QEMU als Virtualisierung.",
  });
  checks.push({
    id: "bios",
    title: "Virtualisierung im BIOS",
    state: host.virtualizationEnabled ? "ok" : "error",
    text: host.virtualizationEnabled
      ? "Dein Prozessor darf virtuelle Maschinen ausführen."
      : "Die Virtualisierung ist im BIOS/UEFI ausgeschaltet. Ohne sie laufen keine VMs.",
    steps: host.virtualizationEnabled ? undefined : biosSteps,
  });

  if (hv) {
    const featureOk = host.hypervFeature === "enabled" && host.hypervModule;
    checks.push({
      id: "hyperv",
      title: "Hyper-V",
      state: host.hypervFeature === "unavailable" ? "error" : !featureOk ? "error" : !host.vmmsRunning ? "warn" : "ok",
      text:
        host.hypervFeature === "unavailable"
          ? "Hyper-V gibt es in dieser Windows-Version nicht. Wechsle unten zu QEMU."
          : host.hypervFeature === "disabled"
            ? "Hyper-V ist noch ausgeschaltet. Nestbox kann es für dich einschalten – danach ist ein Neustart nötig."
            : !host.hypervModule
              ? "Hyper-V ist an, aber die Verwaltungswerkzeuge fehlen. Nestbox kann sie nachinstallieren."
              : !host.vmmsRunning
                ? "Hyper-V ist eingeschaltet, läuft aber noch nicht. Starte den PC einmal neu."
                : "Ist eingeschaltet und läuft.",
      action:
        host.hypervFeature !== "unavailable" && !featureOk ? (
          <Button size="sm" variant="primary" loading={busy === "hyperv"} icon={<ShieldCheck className="size-4" />} onClick={() => run("hyperv", () => api.enableFeature("Microsoft-Hyper-V"))}>
            Hyper-V aktivieren
          </Button>
        ) : undefined,
    });
    checks.push({
      id: "group",
      title: "Berechtigung",
      state: host.hypervGroupOk ? "ok" : host.hypervGroupNeedsRelogin ? "warn" : "error",
      text: host.hypervGroupOk ? (
        "Du darfst VMs ohne Admin-Abfrage verwalten."
      ) : host.hypervGroupNeedsRelogin ? (
        "Fast geschafft: Melde dich einmal von Windows ab und wieder an, damit die neue Berechtigung gilt."
      ) : (
        <>
          Damit Nestbox VMs ohne ständige Admin-Abfragen steuern kann, musst du Mitglied der Gruppe „Hyper-V-Administratoren“ sein.
          <Info text="Eine Windows-Benutzergruppe, deren Mitglieder Hyper-V verwenden dürfen, ohne vollständige Administratorrechte zu haben. Windows fragt dich einmal um Erlaubnis." />
        </>
      ),
      action:
        !host.hypervGroupOk && !host.hypervGroupNeedsRelogin ? (
          <Button size="sm" variant="primary" loading={busy === "group"} onClick={() => run("group", api.fixHypervGroup)}>
            Mich hinzufügen
          </Button>
        ) : undefined,
    });
  } else {
    checks.push({
      id: "whpx",
      title: "Windows-Hypervisor-Plattform",
      state: host.whpxFeature === "enabled" ? "ok" : "error",
      text:
        host.whpxFeature === "enabled" ? (
          "Ist aktiv – VMs laufen mit voller Geschwindigkeit."
        ) : (
          <>
            Damit VMs schnell laufen, braucht QEMU diese Windows-Funktion. Nestbox schaltet sie ein – danach ist ein Neustart nötig.
            <Info text="Eine Schnittstelle von Windows (WHPX), über die Programme wie QEMU die Virtualisierungsfunktionen des Prozessors nutzen dürfen." />
          </>
        ),
      action:
        host.whpxFeature !== "enabled" ? (
          <Button size="sm" variant="primary" loading={busy === "whpx"} onClick={() => run("whpx", () => api.enableFeature("HypervisorPlatform"))}>
            Aktivieren
          </Button>
        ) : undefined,
    });
    checks.push({
      id: "qemu",
      title: "QEMU",
      state: host.qemuPath ? (host.qemuFirmware ? "ok" : "warn") : "error",
      text: host.qemuPath ? (
        host.qemuFirmware ? (
          <span className="selectable break-all">Gefunden: {host.qemuPath}</span>
        ) : (
          "QEMU ist da, aber die UEFI-Firmware fehlt. Installiere QEMU neu (vollständige Installation)."
        )
      ) : (
        <>
          QEMU ist ein kostenloses Programm, das die VMs ausführt. Lade es herunter, installiere es mit den Standardeinstellungen und klicke dann auf „Erneut prüfen“.
        </>
      ),
      action: !host.qemuPath ? (
        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="primary" icon={<ExternalLink className="size-3.5" />} onClick={() => openLink(QEMU_URL)}>
            QEMU herunterladen
          </Button>
          <Button
            size="sm"
            icon={<FolderOpen className="size-3.5" />}
            onClick={async () => {
              const dir = await pickFolder(settings.qemuDir || undefined);
              if (dir) await run("qemudir", () => onSettings({ ...settings, qemuDir: dir }));
            }}
          >
            Ordner wählen
          </Button>
        </div>
      ) : undefined,
    });
    checks.push({
      id: "win11",
      title: "Hinweis zu Windows 11",
      state: "info",
      text: "Windows 11 braucht einen TPM-Sicherheitschip, den QEMU unter Windows nicht nachbilden kann. Linux und ältere Windows-Versionen (per eigener ISO) funktionieren.",
    });
  }

  const switchTo = hv ? "qemu" : host.hypervFeature !== "unavailable" && !host.isHome ? "hyperv" : null;

  return (
    <div className="flex h-full overflow-y-auto">
      <div className="m-auto w-full max-w-[640px] px-6 py-12">
        <div className="mb-8 flex flex-col items-center text-center anim-rise">
          <NestboxLogo size={64} />
          <h1 className="mt-5 font-display text-[28px] font-semibold tracking-[-0.02em]">Willkommen bei Nestbox</h1>
          <p className="mt-1.5 max-w-md text-[15px] text-text-2">
            {ready ? "Alles bereit. Dein PC kann virtuelle Maschinen ausführen." : "Wir prüfen kurz, ob dein PC bereit für virtuelle Maschinen ist."}
          </p>
        </div>

        {host.rebootPending && (
          <div className="mb-4 flex items-center gap-3 rounded-2xl border border-warn/30 bg-warn-soft p-4 anim-rise">
            <RotateCcw className="size-5 shrink-0 text-warn" />
            <div className="flex-1">
              <div className="font-semibold">Neustart nötig</div>
              <div className="text-[13.5px] text-text-2">Windows schließt die Einrichtung beim nächsten Neustart ab. Speichere vorher deine offenen Dateien.</div>
            </div>
            <Button size="sm" onClick={() => setConfirmRestart(true)}>
              Jetzt neu starten
            </Button>
          </div>
        )}

        <div className="overflow-hidden rounded-2xl border border-line bg-surface shadow-card">
          {checks.map((c, i) => (
            <CheckRow key={c.id} check={c} first={i === 0} delay={i * 60} />
          ))}
        </div>

        {error && (
          <div className="mt-4">
            <ErrorPanel error={error} onClose={() => setError(null)} />
          </div>
        )}

        <div className="mt-6 flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-[13px] text-muted">
            <Cpu className="size-4" />
            <span className="truncate">
              {host.cpuName} · {host.logicalCores} Kerne · {Math.round(host.totalMemoryMb / 1024)} GB RAM
            </span>
          </div>
          <div className="flex shrink-0 gap-2">
            <Button variant="ghost" icon={<RefreshCw className={cx("size-4", checking && "anim-spin")} />} disabled={checking} onClick={() => onRecheck()}>
              Erneut prüfen
            </Button>
            <Button variant="primary" disabled={!ready} onClick={onDone}>
              Los geht’s <ArrowRight className="size-4" />
            </Button>
          </div>
        </div>

        {switchTo && (
          <div className="mt-8 text-center text-[13px] text-muted">
            {switchTo === "qemu" ? "Hyper-V lieber nicht verwenden?" : "Dein Windows kann auch Hyper-V nutzen."}{" "}
            <button
              className="font-medium text-accent-text hover:underline"
              onClick={() => run("switch", () => onSettings({ ...settings, backend: switchTo }))}
            >
              {switchTo === "qemu" ? "Stattdessen QEMU verwenden" : "Hyper-V verwenden"}
            </button>
            <Info
              text={
                switchTo === "qemu"
                  ? "QEMU ist ein eigenständiges Programm zum Ausführen von VMs. Hyper-V ist in Windows eingebaut und meist die bessere Wahl."
                  : "Hyper-V ist in Windows eingebaut, läuft sehr stabil und unterstützt auch Windows 11 als Gast."
              }
            />
          </div>
        )}
      </div>

      <Dialog
        open={confirmRestart}
        onClose={() => setConfirmRestart(false)}
        title="PC jetzt neu starten?"
        description="Speichere vorher alle offenen Dateien. Der Neustart beginnt in wenigen Sekunden."
        icon={<span className="flex size-10 items-center justify-center rounded-full bg-warn-soft"><RotateCcw className="size-5 text-warn" /></span>}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmRestart(false)}>
              Später
            </Button>
            <Button
              variant="primary"
              onClick={async () => {
                setConfirmRestart(false);
                await run("restart", api.restartComputer);
              }}
            >
              Neu starten
            </Button>
          </>
        }
      />
    </div>
  );
}

function CheckRow({ check, first, delay }: { check: Check; first: boolean; delay: number }) {
  const [open, setOpen] = useState(false);
  const icon = {
    ok: (
      <span className="flex size-7 items-center justify-center rounded-full bg-ok-soft">
        <Check className="size-4 text-ok" strokeWidth={2.6} />
      </span>
    ),
    warn: (
      <span className="flex size-7 items-center justify-center rounded-full bg-warn-soft">
        <AlertTriangle className="size-4 text-warn" />
      </span>
    ),
    error: (
      <span className="flex size-7 items-center justify-center rounded-full bg-err-soft">
        <X className="size-4 text-err" strokeWidth={2.6} />
      </span>
    ),
    info: (
      <span className="flex size-7 items-center justify-center rounded-full bg-info-soft">
        <InfoIcon className="size-4 text-text-2" />
      </span>
    ),
  }[check.state];
  return (
    <div className={cx("flex gap-4 px-5 py-4 anim-rise", !first && "border-t border-line")} style={{ animationDelay: `${delay}ms` }}>
      <div className="pt-0.5">{icon}</div>
      <div className="min-w-0 flex-1">
        <div className="font-semibold">{check.title}</div>
        <div className="mt-0.5 text-[13.5px] text-text-2">{check.text}</div>
        {check.steps && (
          <div className="mt-2">
            <button onClick={() => setOpen(!open)} className="inline-flex items-center gap-1 text-[13px] font-medium text-accent-text">
              <ChevronRight className={cx("size-3.5 transition-transform", open && "rotate-90")} />
              So schaltest du sie ein
            </button>
            {open && (
              <ol className="mt-2 list-decimal space-y-1 pl-5 text-[13px] text-text-2 anim-fade">
                {check.steps.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ol>
            )}
          </div>
        )}
        {check.action && <div className="mt-3">{check.action}</div>}
      </div>
    </div>
  );
}
