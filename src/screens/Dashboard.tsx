import { useState } from "react";
import { Plus, Settings as SettingsIcon, Zap } from "lucide-react";
import { api } from "../lib/api";
import { BACKEND_LABEL, type AppError, type HostInfo, type Settings, type Vm } from "../lib/types";
import { Button, Dialog, IconButton, Tooltip } from "../components/ui";
import { useToast } from "../components/feedback";
import { EmptyNest, NestboxLogo } from "../components/Logo";
import { VmCard, type VmAction } from "./VmCard";
import { UpdateDialog, UpdatePill, type UpdateState } from "../components/Updater";
import { AppSettingsDialog, DeleteDialog, GuestToolsDialog, ImportHypervDialog, RenameDialog, SnapshotsDialog, VmSettingsDialog } from "./dialogs";

export type OpenDialog =
  | { kind: "snapshots" | "rename" | "delete" | "settings" | "poweroff" | "guesttools"; id: string }
  | { kind: "app-settings" }
  | { kind: "hyperv-import" }
  | { kind: "update" }
  | null;

const BUSY_LABEL: Partial<Record<VmAction, string>> = {
  start: "Startet …",
  resume: "Wird fortgesetzt …",
  pause: "Pausiert …",
  shutdown: "Fährt herunter …",
  poweroff: "Wird ausgeschaltet …",
  eject: "Wirft aus …",
  guesttools: "Legt Treiber-CD ein …",
};

export function Dashboard({
  host,
  settings,
  vms,
  loaded,
  onNew,
  refresh,
  onSaveSettings,
  onOpenSetup,
  initialDialog = null,
  updates,
}: {
  updates: UpdateState;
  host: HostInfo;
  settings: Settings;
  vms: Vm[];
  loaded: boolean;
  onNew: () => void;
  refresh: () => Promise<void>;
  onSaveSettings: (s: Settings) => Promise<void>;
  onOpenSetup: () => void;
  initialDialog?: OpenDialog;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState<Record<string, string>>({});
  const [dialog, setDialog] = useState<OpenDialog>(initialDialog);
  const running = vms.filter((v) => v.status.state === "running").length;

  const dialogVm = dialog && "id" in dialog ? vms.find((v) => v.id === dialog.id) : undefined;

  async function perform(vm: Vm, action: VmAction) {
    const simple: Partial<Record<VmAction, () => Promise<void>>> = {
      start: () => api.start(vm.id),
      resume: () => api.resume(vm.id),
      pause: () => api.pause(vm.id),
      shutdown: () => api.shutdown(vm.id),
      console: () => api.openConsole(vm.id),
      eject: () => api.ejectIso(vm.id),
      guesttools: () => api.installGuestTools(vm.id),
    };
    if (action === "poweroff" || action === "rename" || action === "delete" || action === "settings" || action === "snapshots") {
      setDialog({ kind: action, id: vm.id });
      return;
    }
    const fn = simple[action];
    if (!fn) return;
    const label = BUSY_LABEL[action];
    if (label) setBusy((b) => ({ ...b, [vm.id]: label }));
    try {
      await fn();
      if (action === "eject") toast.ok("Installationsmedium ausgeworfen – die VM startet jetzt von ihrer Festplatte");
      if (action === "shutdown") toast.ok(`„${vm.name}“ ist heruntergefahren`);
      if (action === "guesttools") setDialog({ kind: "guesttools", id: vm.id });
    } catch (e) {
      toast.error(e as AppError);
    } finally {
      setBusy(({ [vm.id]: _, ...rest }) => rest);
      await refresh();
    }
  }

  async function powerOff(vm: Vm) {
    setDialog(null);
    setBusy((b) => ({ ...b, [vm.id]: BUSY_LABEL.poweroff! }));
    try {
      await api.powerOff(vm.id);
    } catch (e) {
      toast.error(e as AppError);
    } finally {
      setBusy(({ [vm.id]: _, ...rest }) => rest);
      await refresh();
    }
  }

  const closeAndRefresh = (msg?: string) => {
    setDialog(null);
    if (msg) toast.ok(msg);
    void refresh();
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex h-16 shrink-0 items-center gap-3 border-b border-line bg-bg/80 px-6 backdrop-blur">
        <NestboxLogo size={30} />
        <div className="font-display text-[17px] font-semibold tracking-[-0.01em]">Nestbox</div>
        <Tooltip text={host.activeBackend === "virtualbox" ? "Neue VMs laufen mit VirtualBox (kostenlos)." : host.activeBackend === "hyperv" ? "Neue VMs laufen mit Hyper-V, der Virtualisierung von Windows Pro." : host.qemuAccelerated ? "Neue VMs laufen mit QEMU und der Windows-Hypervisor-Plattform." : "Neue VMs laufen mit QEMU ohne Beschleunigung – das ist langsam."}>
          <span className="ml-1 rounded-full bg-bg-subtle px-2.5 py-0.5 text-[12px] font-medium text-text-2">{BACKEND_LABEL[host.activeBackend]}</span>
        </Tooltip>
        <div className="flex-1" />
        {updates.update && <UpdatePill update={updates.update} onClick={() => setDialog({ kind: "update" })} />}
        <IconButton label="Einstellungen" tooltipSide="bottom" onClick={() => setDialog({ kind: "app-settings" })}>
          <SettingsIcon className="size-[18px]" />
        </IconButton>
        <Button variant="primary" icon={<Plus className="size-4" strokeWidth={2.5} />} onClick={onNew}>
          Neue VM erstellen
        </Button>
      </header>

      <main className="min-h-0 flex-1 overflow-y-auto">
        {loaded && vms.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center px-6 pb-16 text-center anim-rise">
            <EmptyNest />
            <h1 className="mt-6 font-display text-[24px] font-semibold tracking-[-0.02em]">Dein Nest ist noch leer</h1>
            <p className="mt-2 max-w-sm text-[15px] text-text-2">Erstelle deine erste virtuelle Maschine – in weniger als einer Minute ist sie startklar.</p>
            <div className="mt-6 flex gap-2">
              <Button variant="primary" size="lg" icon={<Plus className="size-4" strokeWidth={2.5} />} onClick={onNew}>
                Neue VM erstellen
              </Button>
              {host.hypervReady && (
                <Button size="lg" onClick={() => setDialog({ kind: "hyperv-import" })}>
                  Hyper-V-VMs übernehmen
                </Button>
              )}
            </div>
          </div>
        ) : (
          <div className="mx-auto max-w-[1180px] px-6 py-7">
            <div className="mb-5 flex items-baseline justify-between">
              <h1 className="font-display text-[22px] font-semibold tracking-[-0.02em]">Deine VMs</h1>
              <span className="text-[13px] text-muted">
                {vms.length} {vms.length === 1 ? "VM" : "VMs"}
                {running > 0 && ` · ${running} ${running === 1 ? "läuft" : "laufen"}`}
              </span>
            </div>
            <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
              {vms.map((vm, i) => (
                <VmCard key={vm.id} vm={vm} index={i} busy={busy[vm.id] ?? null} onAction={(a) => perform(vm, a)} />
              ))}
            </div>
          </div>
        )}
      </main>

      {dialog?.kind === "snapshots" && dialogVm && <SnapshotsDialog vm={dialogVm} onClose={() => closeAndRefresh()} />}
      {dialog?.kind === "guesttools" && dialogVm && <GuestToolsDialog vm={dialogVm} onClose={() => setDialog(null)} />}
      {dialog?.kind === "rename" && dialogVm && <RenameDialog vm={dialogVm} onClose={() => setDialog(null)} onDone={() => closeAndRefresh("Umbenannt")} />}
      {dialog?.kind === "delete" && dialogVm && <DeleteDialog vm={dialogVm} onClose={() => setDialog(null)} onDone={() => closeAndRefresh(`„${dialogVm.name}“ wurde gelöscht`)} />}
      {dialog?.kind === "settings" && dialogVm && (
        <VmSettingsDialog vm={dialogVm} host={host} onClose={() => setDialog(null)} onDone={() => closeAndRefresh("Einstellungen gespeichert")} />
      )}
      {dialog?.kind === "app-settings" && (
        <AppSettingsDialog settings={settings} host={host} onClose={() => setDialog(null)} onSave={onSaveSettings} onOpenSetup={onOpenSetup} onImportHyperv={() => setDialog({ kind: "hyperv-import" })} updates={updates} onOpenUpdate={() => setDialog({ kind: "update" })} />
      )}
      {dialog?.kind === "update" && updates.update && <UpdateDialog update={updates.update} current={updates.version} onClose={() => setDialog(null)} />}
      {dialog?.kind === "hyperv-import" && (
        <ImportHypervDialog onClose={() => setDialog(null)} onDone={(n) => closeAndRefresh(n === 1 ? "1 Hyper-V-VM übernommen" : `${n} Hyper-V-VMs übernommen`)} />
      )}
      <Dialog
        open={dialog?.kind === "poweroff" && !!dialogVm}
        onClose={() => setDialog(null)}
        title={`„${dialogVm?.name}“ sofort ausschalten?`}
        description="Das ist wie Stecker ziehen: Die VM wird ohne Herunterfahren beendet, ungespeicherte Daten darin gehen verloren. Nutze das nur, wenn die VM nicht mehr reagiert."
        icon={
          <span className="flex size-10 items-center justify-center rounded-full bg-warn-soft">
            <Zap className="size-5 text-warn" />
          </span>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setDialog(null)}>
              Abbrechen
            </Button>
            <Button variant="danger" onClick={() => dialogVm && powerOff(dialogVm)}>
              Sofort ausschalten
            </Button>
          </>
        }
      />
    </div>
  );
}
