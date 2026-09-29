import { useState, type ReactNode } from "react";
import { AlertTriangle, ArrowRight, Check, ChevronRight, Cpu, Download, ExternalLink, FolderOpen, Info as InfoIcon, RefreshCw, RotateCcw, X, Zap } from "lucide-react";
import { api, openLink, pickFolder } from "../lib/api";
import type { AppError, BackendKind, HostInfo, Settings } from "../lib/types";
import { Button, Dialog, cx, Info } from "../components/ui";
import { ErrorPanel } from "../components/feedback";
import { NestboxLogo } from "../components/Logo";

type CheckState = "ok" | "warn" | "error" | "info";

interface Check {
  id: string;
  title: ReactNode;
  state: CheckState;
  text: ReactNode;
  action?: ReactNode;
  steps?: string[];
  stepsLabel?: string;
}

const VBOX_URL = "https://www.virtualbox.org/wiki/Downloads";
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
  const ready = host.vboxReady || host.qemuReady;

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

  const install = (kind: BackendKind, url: string) => (
    <div className="flex flex-wrap gap-2">
      {host.wingetAvailable && (
        <Button size="sm" variant="primary" loading={busy === kind} disabled={!!busy && busy !== kind} icon={<Download className="size-3.5" />} onClick={() => run(kind, () => api.installSoftware(kind))}>
          {busy === kind ? "Wird installiert …" : "Jetzt installieren"}
        </Button>
      )}
      <Button size="sm" variant={host.wingetAvailable ? "ghost" : "primary"} icon={<ExternalLink className="size-3.5" />} onClick={() => openLink(url)}>
        Download-Seite
      </Button>
    </div>
  );

  const biosSteps = [
    "PC neu starten und beim Hochfahren wiederholt die BIOS-Taste drücken (meist Entf, F2 oder F10).",
    "Im BIOS/UEFI nach „Intel VT-x“, „Intel Virtualization Technology“, „AMD-V“ oder „SVM Mode“ suchen (oft unter „Advanced“ oder „CPU Configuration“).",
    "Die Option auf „Enabled“ stellen, speichern (meist F10) und neu starten.",
  ];
  const nestedSteps = [
    "Proxmox: VM auswählen → Hardware → Prozessoren → Typ auf „host“ stellen.",
    "Hyper-V als Host: in einer Admin-PowerShell „Set-VMProcessor -VMName <Name> -ExposeVirtualizationExtensions $true“ ausführen.",
    "VMware: in den VM-Einstellungen beim Prozessor „Intel VT-x/EPT oder AMD-V/RVI virtualisieren“ anhaken.",
    "Danach diese VM komplett herunterfahren (nicht nur neu starten) und wieder einschalten.",
  ];

  const checks: Check[] = [];

  // 1. Virtualisierung
  checks.push({
    id: "virt",
    title: host.isVirtualMachine ? "Verschachtelte Virtualisierung" : "Virtualisierung im BIOS",
    state: host.virtualizationEnabled ? "ok" : host.qemuReady ? "warn" : "error",
    text: host.virtualizationEnabled ? (
      host.isVirtualMachine ? "Dein Windows läuft selbst in einer VM – der Host reicht die Virtualisierung durch." : "Dein Prozessor darf virtuelle Maschinen ausführen."
    ) : (
      <>
        {host.isVirtualMachine ? (
          <>
            Dein Windows läuft selbst in einer virtuellen Maschine ({host.machineName}). Der Host reicht die Virtualisierung nicht durch.
            <Info text="Das nennt man „verschachtelte Virtualisierung“ (nested virtualization): eine VM, in der wieder VMs laufen. Der Prozessor muss dafür seine Virtualisierungsbefehle an die innere VM weitergeben." />
          </>
        ) : (
          "Die Virtualisierung ist im BIOS/UEFI ausgeschaltet."
        )}{" "}
        Bis das behoben ist, funktioniert nur QEMU – und zwar sehr langsam.
      </>
    ),
    steps: host.virtualizationEnabled ? undefined : host.isVirtualMachine ? nestedSteps : biosSteps,
    stepsLabel: "So schaltest du sie ein",
  });

  // 2. VirtualBox
  checks.push({
    id: "vbox",
    title: (
      <span className="inline-flex items-center gap-2">
        VirtualBox
        <span className="rounded-full bg-accent-soft px-2 py-px text-[11px] font-semibold text-accent-text">Empfohlen</span>
      </span>
    ),
    state: host.vboxPath ? (host.virtualizationEnabled ? "ok" : "warn") : "error",
    text: host.vboxPath ? (
      host.virtualizationEnabled ? (
        <>Installiert (Version {host.vboxVersion?.split("r")[0] ?? "?"}). Kann alle Systeme ausführen – auch Windows 11.</>
      ) : (
        "Installiert, braucht aber die Virtualisierung (siehe oben)."
      )
    ) : (
      <>
        Kostenloses Programm von Oracle, das die VMs ausführt. Kann auch Windows 11 (mit TPM und Secure Boot).
        <Info text="VirtualBox ist kostenlos und läuft auf jeder Windows-Version – auch Home. Die Installation fragt einmal nach Administratorrechten." />
      </>
    ),
    action: host.vboxPath ? undefined : install("virtualbox", VBOX_URL),
  });

  // 3. QEMU
  checks.push({
    id: "qemu",
    title: (
      <span className="inline-flex items-center gap-2">
        QEMU <span className="text-[12px] font-normal text-muted">Alternative</span>
      </span>
    ),
    state: host.qemuBroken ? "error" : host.qemuPath ? (host.qemuFirmware ? "ok" : "warn") : host.vboxReady ? "info" : "error",
    text: host.qemuBroken ? (
      "QEMU ist nur unvollständig installiert und startet nicht (z. B. weil die Installation abgebrochen wurde oder der Speicherplatz knapp war). Installiere es erneut."
    ) : host.qemuPath ? (
      host.qemuFirmware ? (
        <span className="selectable break-all">Installiert: {host.qemuPath}</span>
      ) : (
        "QEMU ist da, aber die UEFI-Firmware fehlt. Installiere QEMU neu (vollständige Installation)."
      )
    ) : host.vboxReady ? (
      "Optional. Kostenlos und quelloffen – nur nötig, wenn du VirtualBox nicht nutzen möchtest."
    ) : (
      <>
        Kostenlos und quelloffen. Läuft notfalls auch ohne Hardware-Virtualisierung (dann langsam). Windows 11 geht damit nicht.
        <Info text="Windows 11 verlangt einen TPM-Sicherheitschip, den QEMU unter Windows nicht nachbilden kann. Linux und ältere Windows-Versionen funktionieren." />
      </>
    ),
    action: !host.qemuPath || host.qemuBroken ? (
      <div className="flex flex-wrap items-center gap-2">
        {install("qemu", QEMU_URL)}
        <Button
          size="sm"
          variant="ghost"
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

  // 4. Beschleunigung für QEMU (nur relevant, wenn QEMU genutzt wird)
  if (host.qemuPath && host.virtualizationEnabled && host.activeBackend === "qemu") {
    checks.push({
      id: "whpx",
      title: "Schneller Modus für QEMU",
      state: host.qemuAccelerated ? "ok" : "warn",
      text: host.qemuAccelerated ? (
        "Die Windows-Hypervisor-Plattform ist aktiv – QEMU läuft mit voller Geschwindigkeit."
      ) : (
        <>
          Schalte die Windows-Hypervisor-Plattform ein, damit QEMU-VMs flüssig laufen. Danach ist ein Neustart nötig.
          <Info text="Eine kostenlose Windows-Funktion (WHPX), über die QEMU die Virtualisierung des Prozessors nutzt. Ohne sie rechnet QEMU alles in Software – das ist sehr langsam." />
        </>
      ),
      action:
        !host.qemuAccelerated && host.whpxFeature !== "enabled" ? (
          <Button size="sm" variant="primary" icon={<Zap className="size-3.5" />} loading={busy === "whpx"} onClick={() => run("whpx", api.enableWhpx)}>
            Einschalten
          </Button>
        ) : undefined,
    });
  }

  const usable = host.vboxReady ? "VirtualBox" : host.qemuReady ? "QEMU" : null;

  return (
    <div className="flex h-full overflow-y-auto">
      <div className="m-auto w-full max-w-[640px] px-6 py-12">
        <div className="mb-8 flex flex-col items-center text-center anim-rise">
          <NestboxLogo size={64} />
          <h1 className="mt-5 font-display text-[28px] font-semibold tracking-[-0.02em]">Willkommen bei Nestbox</h1>
          <p className="mt-1.5 max-w-md text-[15px] text-text-2">
            {ready
              ? `Alles bereit. Deine VMs laufen mit ${usable} – kostenlos, auf jeder Windows-Version.`
              : "Wir prüfen kurz, ob dein PC bereit für virtuelle Maschinen ist. Alles, was fehlt, ist kostenlos."}
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

        {busy === "virtualbox" || busy === "qemu" ? (
          <p className="mt-3 text-center text-[13px] text-muted anim-fade">
            Das dauert ein paar Minuten. Bestätige die Windows-Abfrage („Möchten Sie zulassen …?“) mit „Ja“, falls sie erscheint.
          </p>
        ) : null}

        {error && (
          <div className="mt-4">
            <ErrorPanel error={error} onClose={() => setError(null)} />
          </div>
        )}

        <div className="mt-6 flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2 text-[13px] text-muted">
            <Cpu className="size-4 shrink-0" />
            <span className="truncate">
              {host.windowsName} · {host.logicalCores} Kerne · {Math.round(host.totalMemoryMb / 1024)} GB RAM
            </span>
          </div>
          <div className="flex shrink-0 gap-2">
            <Button variant="ghost" icon={<RefreshCw className={cx("size-4", checking && "anim-spin")} />} disabled={checking || !!busy} onClick={() => onRecheck()}>
              Erneut prüfen
            </Button>
            <Button variant="primary" disabled={!ready || !!busy} onClick={onDone}>
              Los geht’s <ArrowRight className="size-4" />
            </Button>
          </div>
        </div>
      </div>

      <Dialog
        open={confirmRestart}
        onClose={() => setConfirmRestart(false)}
        title="PC jetzt neu starten?"
        description="Speichere vorher alle offenen Dateien. Der Neustart beginnt in wenigen Sekunden."
        icon={
          <span className="flex size-10 items-center justify-center rounded-full bg-warn-soft">
            <RotateCcw className="size-5 text-warn" />
          </span>
        }
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
              {check.stepsLabel ?? "So geht’s"}
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
