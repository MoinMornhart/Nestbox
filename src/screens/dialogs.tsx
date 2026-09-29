import { useEffect, useState } from "react";
import { AlertTriangle, FileText, FolderOpen, History, Plus, RotateCcw, Trash2 } from "lucide-react";
import { api, pickFolder, revealPath } from "../lib/api";
import { formatMemory, limitsFor } from "../lib/presets";
import type { AppError, BackendChoice, HostInfo, Settings, Snapshot, Vm } from "../lib/types";
import { Button, Checkbox, Dialog, Info, Segmented, Slider, Spinner, TextInput, cx } from "../components/ui";
import { ErrorPanel, useToast } from "../components/feedback";
import { NestboxLogo } from "../components/Logo";

const fmtDate = (iso: string) =>
  new Date(iso).toLocaleString("de-DE", { weekday: "short", day: "2-digit", month: "long", year: "numeric", hour: "2-digit", minute: "2-digit" });

function relative(iso: string) {
  const d = (Date.now() - new Date(iso).getTime()) / 1000;
  if (d < 60) return "gerade eben";
  if (d < 3600) return `vor ${Math.floor(d / 60)} Min.`;
  if (d < 86400) return `vor ${Math.floor(d / 3600)} Std.`;
  const days = Math.floor(d / 86400);
  return days === 1 ? "gestern" : `vor ${days} Tagen`;
}

// ── Sicherungspunkte ──

export function SnapshotsDialog({ vm, onClose }: { vm: Vm; onClose: () => void }) {
  const toast = useToast();
  const [list, setList] = useState<Snapshot[] | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);
  const [confirm, setConfirm] = useState<{ kind: "restore" | "delete"; snap: Snapshot } | null>(null);
  const [working, setWorking] = useState(false);

  const load = async () => {
    try {
      setList(await api.listSnapshots(vm.id));
    } catch (e) {
      setError(e as AppError);
      setList([]);
    }
  };
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vm.id]);

  const create = async () => {
    setCreating(true);
    setError(null);
    try {
      const s = await api.createSnapshot(vm.id, name);
      setName("");
      toast.ok(`Sicherungspunkt „${s.name}“ erstellt`);
      await load();
    } catch (e) {
      setError(e as AppError);
    } finally {
      setCreating(false);
    }
  };

  const doConfirm = async () => {
    if (!confirm) return;
    setWorking(true);
    setError(null);
    try {
      if (confirm.kind === "restore") {
        await api.restoreSnapshot(vm.id, confirm.snap.id);
        toast.ok(`„${vm.name}“ wurde auf „${confirm.snap.name}“ zurückgesetzt`);
      } else {
        await api.deleteSnapshot(vm.id, confirm.snap.id);
        toast.ok("Sicherungspunkt gelöscht");
      }
      setConfirm(null);
      await load();
    } catch (e) {
      setConfirm(null);
      setError(e as AppError);
    } finally {
      setWorking(false);
    }
  };

  const running = vm.status.state === "running" || vm.status.state === "paused";

  return (
    <>
      <Dialog
        open={!confirm}
        onClose={onClose}
        width={540}
        title="Sicherungspunkte"
        description={
          <>
            Speichere den aktuellen Zustand von „{vm.name}“ und kehre später jederzeit dorthin zurück – z. B. vor einem Update.
            <Info text="Technisch heißen Sicherungspunkte „Snapshots“ oder „Checkpoints“. Sie speichern Festplatte und – bei laufender VM – auch den Arbeitsspeicher." />
          </>
        }
        icon={
          <span className="flex size-10 items-center justify-center rounded-full bg-accent-soft">
            <History className="size-5 text-accent-text" />
          </span>
        }
        footer={
          <Button variant="ghost" onClick={onClose}>
            Schließen
          </Button>
        }
      >
        <div className="flex gap-2">
          <TextInput className="flex-1" value={name} onChange={setName} placeholder={`Sicherungspunkt vom ${new Date().toLocaleDateString("de-DE")}`} onEnter={create} />
          <Button variant="primary" size="lg" loading={creating} icon={<Plus className="size-4" />} onClick={create}>
            Erstellen
          </Button>
        </div>
        {error && (
          <div className="mt-3">
            <ErrorPanel error={error} onClose={() => setError(null)} />
          </div>
        )}
        <div className="mt-4">
          {list === null ? (
            <div className="flex justify-center py-8">
              <Spinner className="size-5" />
            </div>
          ) : list.length === 0 ? (
            <div className="rounded-xl border border-dashed border-line-strong px-4 py-8 text-center text-[13.5px] text-muted">Noch keine Sicherungspunkte.</div>
          ) : (
            <ul className="overflow-hidden rounded-xl border border-line">
              {list.map((s, i) => (
                <li key={s.id} className={cx("group flex items-center gap-3 px-4 py-3", i > 0 && "border-t border-line")}>
                  <span className="flex size-8 items-center justify-center rounded-lg bg-bg-subtle">
                    <History className="size-4 text-text-2" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{s.name}</div>
                    <div className="text-[12.5px] text-muted">
                      {fmtDate(s.created)} · {relative(s.created)}
                      {!s.withState && " · nur Festplatte"}
                    </div>
                  </div>
                  <Button size="sm" icon={<RotateCcw className="size-3.5" />} onClick={() => setConfirm({ kind: "restore", snap: s })}>
                    Wiederherstellen
                  </Button>
                  <button
                    onClick={() => setConfirm({ kind: "delete", snap: s })}
                    className="rounded-lg p-2 text-muted opacity-60 transition-all hover:bg-err-soft hover:text-err hover:opacity-100 group-hover:opacity-100"
                    aria-label="Löschen"
                    title="Sicherungspunkt löschen"
                  >
                    <Trash2 className="size-4" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </Dialog>

      <Dialog
        open={!!confirm}
        onClose={() => !working && setConfirm(null)}
        title={confirm?.kind === "restore" ? "Sicherungspunkt wiederherstellen?" : "Sicherungspunkt löschen?"}
        description={
          confirm?.kind === "restore"
            ? `„${vm.name}“ wird auf den Stand von „${confirm.snap.name}“ zurückgesetzt. ${running ? "Die VM wird dafür kurz angehalten. " : ""}Alles, was seitdem passiert ist, geht verloren – außer du erstellst vorher einen neuen Sicherungspunkt.`
            : `„${confirm?.snap.name}“ wird entfernt. Die VM selbst bleibt unverändert.`
        }
        icon={
          <span className={cx("flex size-10 items-center justify-center rounded-full", confirm?.kind === "restore" ? "bg-warn-soft" : "bg-err-soft")}>
            {confirm?.kind === "restore" ? <RotateCcw className="size-5 text-warn" /> : <Trash2 className="size-5 text-err" />}
          </span>
        }
        footer={
          <>
            <Button variant="ghost" disabled={working} onClick={() => setConfirm(null)}>
              Abbrechen
            </Button>
            <Button variant={confirm?.kind === "restore" ? "primary" : "danger"} loading={working} onClick={doConfirm}>
              {confirm?.kind === "restore" ? "Wiederherstellen" : "Löschen"}
            </Button>
          </>
        }
      />
    </>
  );
}

// ── Umbenennen ──

export function RenameDialog({ vm, onClose, onDone }: { vm: Vm; onClose: () => void; onDone: () => void }) {
  const [name, setName] = useState(vm.name);
  const [error, setError] = useState<string | null>(null);
  const [appError, setAppError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    setAppError(null);
    try {
      const problem = await api.checkName(name, vm.backend, vm.id);
      if (problem) {
        setError(problem);
        return;
      }
      await api.rename(vm.id, name.trim());
      onDone();
    } catch (e) {
      setAppError(e as AppError);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title="VM umbenennen"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Abbrechen
          </Button>
          <Button variant="primary" loading={busy} disabled={!name.trim() || name.trim() === vm.name} onClick={save}>
            Speichern
          </Button>
        </>
      }
    >
      <TextInput
        value={name}
        autoFocus
        error={error}
        onChange={(v) => {
          setName(v);
          setError(null);
        }}
        onEnter={save}
      />
      {appError && (
        <div className="mt-3">
          <ErrorPanel error={appError} />
        </div>
      )}
    </Dialog>
  );
}

// ── Löschen ──

export function DeleteDialog({ vm, onClose, onDone }: { vm: Vm; onClose: () => void; onDone: () => void }) {
  const [deleteDisk, setDeleteDisk] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const running = vm.status.state !== "off" && vm.status.state !== "missing" && vm.status.state !== "unknown";
  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.remove(vm.id, deleteDisk);
      onDone();
    } catch (e) {
      setError(e as AppError);
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={() => !busy && onClose()}
      title={`„${vm.name}“ löschen?`}
      description={running ? "Die VM läuft gerade und wird dafür sofort ausgeschaltet. Ungespeicherte Daten in der VM gehen verloren." : "Die VM verschwindet aus Nestbox."}
      icon={
        <span className="flex size-10 items-center justify-center rounded-full bg-err-soft">
          <Trash2 className="size-5 text-err" />
        </span>
      }
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={onClose}>
            Abbrechen
          </Button>
          <Button variant="danger" loading={busy} onClick={go}>
            {deleteDisk ? "VM und Festplatte löschen" : "VM löschen"}
          </Button>
        </>
      }
    >
      <Checkbox checked={deleteDisk} onChange={setDeleteDisk}>
        <span className="font-medium">Festplatte ebenfalls löschen</span>
        <span className="mt-0.5 block text-muted">
          Entfernt alle Dateien und Sicherungspunkte der VM endgültig und gibt den Speicherplatz frei. Ohne Haken bleibt die Festplatte erhalten.
        </span>
      </Checkbox>
      {deleteDisk && (
        <div className="mt-3 flex items-center gap-2 text-[13px] text-err anim-fade">
          <AlertTriangle className="size-4" />
          Das kann nicht rückgängig gemacht werden.
        </div>
      )}
      {error && (
        <div className="mt-3">
          <ErrorPanel error={error} />
        </div>
      )}
    </Dialog>
  );
}

// ── Einstellungen einer VM ──

export function VmSettingsDialog({ vm, host, onClose, onDone }: { vm: Vm; host: HostInfo; onClose: () => void; onDone: () => void }) {
  const limits = limitsFor(vm.osFamily, host.logicalCores, host.totalMemoryMb);
  const [cpus, setCpus] = useState(vm.cpus);
  const [mem, setMem] = useState(vm.memoryMb);
  const [disk, setDisk] = useState(vm.diskGb);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const off = vm.status.state === "off" || vm.status.state === "unknown";
  const changed = cpus !== vm.cpus || mem !== vm.memoryMb || disk !== vm.diskGb;

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.update(vm.id, { cpus, memoryMb: mem, diskGb: disk });
      onDone();
    } catch (e) {
      setError(e as AppError);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      width={520}
      title={`Einstellungen · ${vm.name}`}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Abbrechen
          </Button>
          <Button variant="primary" loading={busy} disabled={!changed || !off} onClick={save}>
            Speichern
          </Button>
        </>
      }
    >
      {!off && (
        <div className="mb-4 rounded-xl border border-warn/30 bg-warn-soft p-3 text-[13.5px]">
          Fahre die VM zuerst herunter, um Leistung und Festplatte zu ändern.
        </div>
      )}
      <div className="grid gap-6">
        <Slider label="Prozessorkerne" info="Wie viele Rechenkerne die VM gleichzeitig nutzen darf." value={cpus} min={limits.minCpus} max={Math.max(limits.maxCpus, vm.cpus)} format={(v) => `${v}`} onChange={setCpus} disabled={!off} />
        <Slider
          label="Arbeitsspeicher (RAM)"
          info="Solange die VM läuft, ist dieser Arbeitsspeicher für sie reserviert."
          value={mem}
          min={limits.minMemoryMb}
          max={Math.max(limits.maxMemoryMb, vm.memoryMb)}
          step={512}
          format={formatMemory}
          onChange={setMem}
          disabled={!off}
        />
        <Slider
          label="Festplatte"
          info="Die Festplatte kann nur vergrößert werden. Im Betriebssystem der VM musst du die Partition danach noch erweitern."
          value={disk}
          min={vm.diskGb}
          max={Math.max(vm.diskGb + 8, Math.min(1024, vm.diskGb + Math.floor(host.freeDiskGb)))}
          step={8}
          format={(v) => `${v} GB`}
          onChange={setDisk}
          disabled={!off}
        />
      </div>
      <div className="mt-5 space-y-1 rounded-xl bg-bg-subtle p-3.5 text-[12.5px] text-text-2">
        <Row k="Technik" v={vm.backend === "hyperv" ? "Hyper-V" : "QEMU"} />
        <Row k="Installationsmedium" v={vm.isoPath ? vm.isoPath.split("\\").pop()! : "keines"} />
        <Row
          k="Dateien"
          v={
            <button className="inline-flex items-center gap-1 font-medium text-accent-text hover:underline" onClick={() => revealPath(vm.diskPath)}>
              <FolderOpen className="size-3.5" /> Im Explorer zeigen
            </button>
          }
        />
      </div>
      {error && (
        <div className="mt-3">
          <ErrorPanel error={error} />
        </div>
      )}
    </Dialog>
  );
}

function Row({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="flex gap-3">
      <span className="w-36 shrink-0 text-muted">{k}</span>
      <span className="min-w-0 flex-1 truncate selectable">{v}</span>
    </div>
  );
}

// ── App-Einstellungen ──

export function AppSettingsDialog({
  settings,
  host,
  onClose,
  onSave,
  onOpenSetup,
}: {
  settings: Settings;
  host: HostInfo;
  onClose: () => void;
  onSave: (s: Settings) => Promise<void>;
  onOpenSetup: () => void;
}) {
  const [draft, setDraft] = useState(settings);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [logPath, setLogPath] = useState("");
  useEffect(() => {
    api.logPath().then(setLogPath).catch(() => {});
  }, []);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await onSave(draft);
      onClose();
    } catch (e) {
      setError(e as AppError);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      width={560}
      title="Einstellungen"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Abbrechen
          </Button>
          <Button variant="primary" loading={busy} onClick={save}>
            Speichern
          </Button>
        </>
      }
    >
      <Section title="Speicherort für neue VMs" sub="Hier legt Nestbox Festplatten und Konfiguration neuer VMs ab. Bestehende VMs bleiben, wo sie sind.">
        <div className="flex gap-2">
          <div className="flex h-9 min-w-0 flex-1 items-center rounded-[10px] border border-line-strong bg-surface-2 px-3 text-[13px]">
            <span className="truncate selectable">{draft.vmDir}</span>
          </div>
          <Button
            icon={<FolderOpen className="size-4" />}
            onClick={async () => {
              const d = await pickFolder(draft.vmDir);
              if (d) setDraft({ ...draft, vmDir: d });
            }}
          >
            Ändern
          </Button>
        </div>
      </Section>

      <Section
        title="Virtualisierung"
        sub={
          <>
            „Automatisch“ nutzt Hyper-V, wenn dein Windows es enthält, sonst QEMU.
            <Info text="Hyper-V ist in Windows Pro/Enterprise/Education eingebaut. QEMU ist ein eigenständiges, kostenloses Programm und funktioniert auch unter Windows Home." />
          </>
        }
      >
        <Segmented<BackendChoice>
          value={draft.backend}
          onChange={(b) => setDraft({ ...draft, backend: b })}
          options={[
            { value: "auto", label: "Automatisch" },
            ...(host.isHome ? [] : [{ value: "hyperv" as const, label: "Hyper-V" }]),
            { value: "qemu", label: "QEMU" },
          ]}
        />
      </Section>

      {(draft.backend === "qemu" || host.activeBackend === "qemu") && (
        <Section title="QEMU-Ordner" sub={host.qemuPath ? `Gefunden: ${host.qemuPath}` : "Nicht gefunden. Wähle den Ordner, in dem qemu-system-x86_64.exe liegt."}>
          <div className="flex gap-2">
            <div className="flex h-9 min-w-0 flex-1 items-center rounded-[10px] border border-line-strong bg-surface-2 px-3 text-[13px]">
              <span className="truncate text-text-2">{draft.qemuDir || "Automatisch suchen"}</span>
            </div>
            <Button
              icon={<FolderOpen className="size-4" />}
              onClick={async () => {
                const d = await pickFolder(draft.qemuDir || undefined);
                if (d) setDraft({ ...draft, qemuDir: d });
              }}
            >
              Wählen
            </Button>
          </div>
        </Section>
      )}

      <Section title="Fehlersuche" sub="Die Log-Datei enthält alle Befehle, die Nestbox ausgeführt hat.">
        <div className="flex flex-wrap gap-2">
          <Button icon={<FileText className="size-4" />} onClick={() => revealPath(logPath)} disabled={!logPath}>
            Log-Ordner öffnen
          </Button>
          <Button
            variant="ghost"
            onClick={() => {
              onClose();
              onOpenSetup();
            }}
          >
            Einrichtung erneut prüfen
          </Button>
        </div>
      </Section>

      <div className="mb-2 mt-6 flex items-center gap-3 border-t border-line pt-5 text-[12.5px] text-muted">
        <NestboxLogo size={22} />
        Nestbox 1.0 · {host.windowsName} · {host.activeBackend === "hyperv" ? "Hyper-V" : "QEMU"}
      </div>
      {error && <ErrorPanel error={error} />}
    </Dialog>
  );
}

function Section({ title, sub, children }: { title: string; sub?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="mb-6">
      <div className="font-semibold">{title}</div>
      {sub && <div className="mb-2.5 mt-0.5 text-[13px] text-text-2">{sub}</div>}
      {children}
    </div>
  );
}
