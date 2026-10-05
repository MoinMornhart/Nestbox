// Einzige Verbindung zum Rust-Backend. Außerhalb von Tauri (npm run dev im Browser)
// antwortet stattdessen ein Mock mit Beispieldaten – für Entwicklung und Vorschaubilder.
import type {
  AppError,
  HyperVCandidate,
  IsoProgress,
  IsoStatus,
  BackendKind,
  CreateProgress,
  CreateSpec,
  HostInfo,
  IsoInfo,
  Settings,
  Snapshot,
  Vm,
  VmChanges,
  VmStatus,
} from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "title" in e && "hint" in e) return e as AppError;
  return {
    title: "Etwas ist schiefgelaufen",
    hint: "Versuche es noch einmal. Bleibt der Fehler, hilft die Log-Datei (Einstellungen → Log-Ordner öffnen).",
    details: e instanceof Error ? e.message : String(e),
  };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    if (inTauri) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<T>(cmd, args);
    }
    const { mockCall } = await import("./mock");
    return await mockCall<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}

export const api = {
  hostInfo: () => call<HostInfo>("get_host_info"),
  installSoftware: (kind: BackendKind) => call<void>("install_software", { kind }),
  enableWhpx: () => call<void>("fix_enable_whpx"),
  enableHyperv: () => call<void>("fix_enable_hyperv"),
  fixHypervGroup: () => call<void>("fix_hyperv_group"),
  listHypervImport: () => call<HyperVCandidate[]>("list_hyperv_import"),
  importHyperv: (candidates: HyperVCandidate[]) => call<number>("import_hyperv", { candidates }),
  restartComputer: () => call<void>("restart_computer"),

  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (newSettings: Settings) => call<Settings>("save_settings", { newSettings }),
  logPath: () => call<string>("get_log_path"),

  listVms: () => call<Vm[]>("list_vms"),
  vmStatus: () => call<VmStatus[]>("vm_status"),
  checkName: (name: string, backend: BackendKind, exceptId?: string) =>
    call<string | null>("check_vm_name", { name, backend, exceptId: exceptId ?? null }),
  inspectIso: (path: string) => call<IsoInfo>("inspect_iso", { path }),
  isoStatus: (osId: string) => call<IsoStatus>("iso_status", { osId }),
  isoFolder: () => call<string>("iso_folder"),
  cancelIsoDownload: (osId: string) => call<void>("cancel_iso_download", { osId }),

  async ensureIso(osId: string, onProgress: (p: IsoProgress) => void): Promise<string> {
    if (inTauri) {
      const { Channel } = await import("@tauri-apps/api/core");
      const channel = new Channel<IsoProgress>();
      channel.onmessage = onProgress;
      return call<string>("ensure_iso", { osId, onProgress: channel });
    }
    const { mockEnsureIso } = await import("./mock");
    return mockEnsureIso(osId, onProgress);
  },

  async createVm(spec: CreateSpec, backend: BackendKind, onProgress: (p: CreateProgress) => void): Promise<Vm> {
    if (inTauri) {
      const { Channel } = await import("@tauri-apps/api/core");
      const channel = new Channel<CreateProgress>();
      channel.onmessage = onProgress;
      return call<Vm>("create_vm", { spec, backend, onProgress: channel });
    }
    const { mockCreate } = await import("./mock");
    try {
      return await mockCreate(spec, backend, onProgress);
    } catch (e) {
      throw toAppError(e);
    }
  },

  start: (id: string) => call<void>("start_vm", { id }),
  pause: (id: string) => call<void>("pause_vm", { id }),
  resume: (id: string) => call<void>("resume_vm", { id }),
  shutdown: (id: string) => call<void>("shutdown_vm", { id }),
  powerOff: (id: string) => call<void>("poweroff_vm", { id }),
  openConsole: (id: string) => call<void>("open_console", { id }),
  rename: (id: string, name: string) => call<void>("rename_vm", { id, name }),
  update: (id: string, changes: VmChanges) => call<void>("update_vm", { id, changes }),
  ejectIso: (id: string) => call<void>("eject_iso", { id }),
  installGuestTools: (id: string) => call<void>("install_guest_tools", { id }),
  remove: (id: string, deleteDisk: boolean) => call<void>("delete_vm", { id, deleteDisk }),

  listSnapshots: (id: string) => call<Snapshot[]>("list_snapshots", { id }),
  createSnapshot: (id: string, name: string) => call<Snapshot>("create_snapshot", { id, name }),
  restoreSnapshot: (id: string, snapshotId: string) => call<void>("restore_snapshot", { id, snapshotId }),
  deleteSnapshot: (id: string, snapshotId: string) => call<void>("delete_snapshot", { id, snapshotId }),
};

// ── Systemfunktionen (Dateiauswahl, Links, Explorer) ──

export async function pickIso(): Promise<string | null> {
  if (!inTauri) return "C:\\Users\\Demo\\Downloads\\ubuntu-24.04.3-desktop-amd64.iso";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const res = await open({ multiple: false, directory: false, filters: [{ name: "ISO-Abbild", extensions: ["iso"] }] });
  return typeof res === "string" ? res : null;
}

export async function pickFolder(current?: string): Promise<string | null> {
  if (!inTauri) return current ?? null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const res = await open({ multiple: false, directory: true, defaultPath: current });
  return typeof res === "string" ? res : null;
}

export async function openLink(url: string) {
  if (!inTauri) {
    window.open(url, "_blank", "noopener");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export async function revealPath(path: string) {
  if (!inTauri) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

/** Datei-Drop aus dem Explorer. Tauri liefert echte Pfade, der Browser nur Dateinamen. */
export async function onFileDrop(handlers: {
  over: (active: boolean) => void;
  drop: (paths: string[]) => void;
}): Promise<() => void> {
  if (inTauri) {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    return getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") handlers.over(true);
      else if (p.type === "leave") handlers.over(false);
      else if (p.type === "drop") {
        handlers.over(false);
        handlers.drop(p.paths);
      }
    });
  }
  const over = (e: DragEvent) => {
    e.preventDefault();
    handlers.over(true);
  };
  const leave = (e: DragEvent) => {
    if (!e.relatedTarget) handlers.over(false);
  };
  const drop = (e: DragEvent) => {
    e.preventDefault();
    handlers.over(false);
    const files = Array.from(e.dataTransfer?.files ?? []);
    handlers.drop(files.map((f) => `C:\\Users\\Demo\\Downloads\\${f.name}`));
  };
  window.addEventListener("dragover", over);
  window.addEventListener("dragleave", leave);
  window.addEventListener("drop", drop);
  return () => {
    window.removeEventListener("dragover", over);
    window.removeEventListener("dragleave", leave);
    window.removeEventListener("drop", drop);
  };
}
