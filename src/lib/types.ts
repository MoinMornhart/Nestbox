export type BackendKind = "virtualbox" | "qemu" | "hyperv";

export const BACKEND_LABEL: Record<BackendKind, string> = { virtualbox: "VirtualBox", qemu: "QEMU", hyperv: "Hyper-V" };
export type BackendChoice = "auto" | BackendKind;
export type OsFamily = "windows" | "linux";
export type PowerState = "running" | "paused" | "off" | "starting" | "stopping" | "saving" | "unknown" | "missing";

export interface AppError {
  title: string;
  hint: string;
  details?: string | null;
}

export interface HostInfo {
  windowsName: string;
  editionId: string;
  build: string;
  isHome: boolean;
  whpxFeature: "enabled" | "disabled" | "unavailable";
  virtualizationEnabled: boolean;
  windowsHypervisorRunning: boolean;
  isVirtualMachine: boolean;
  machineName: string;
  vboxPath: string | null;
  vboxVersion: string | null;
  qemuPath: string | null;
  qemuFirmware: boolean;
  qemuBroken: boolean;
  qemuAccelerated: boolean;
  wingetAvailable: boolean;
  totalMemoryMb: number;
  logicalCores: number;
  cpuName: string;
  freeDiskGb: number;
  rebootPending: boolean;
  vboxReady: boolean;
  qemuReady: boolean;
  activeBackend: BackendKind;
  backendChoice: BackendChoice;
  hypervFeature: "enabled" | "disabled" | "unavailable";
  hypervModule: boolean;
  vmmsRunning: boolean;
  hypervGroupOk: boolean;
  hypervGroupNeedsRelogin: boolean;
  hypervReady: boolean;
}

/** ready = neueste Version liegt bereit · missing = noch nicht geladen · outdated = ältere vorhanden · manual = nur von Hand (Windows) */
export interface IsoStatus {
  state: "ready" | "missing" | "outdated" | "manual";
  path: string | null;
  fileName: string | null;
  version: string;
  offline: boolean;
}

export interface IsoProgress {
  phase: "resolve" | "download" | "verify" | "done";
  received: number;
  total: number;
  fileName: string;
}

export interface HyperVCandidate {
  id: string;
  name: string;
  state: string;
  cpus: number;
  memoryMb: number;
  diskGb: number;
  diskPath: string;
  path: string;
  windows: boolean;
  created: string;
}

export interface Settings {
  vmDir: string;
  backend: BackendChoice;
  qemuDir: string;
  /** leer = „ISOs“ neben dem VM-Ordner */
  isoDir: string;
  rebootPendingSince: string | null;
  setupDone: boolean;
}

export interface VmStatus {
  id: string;
  state: PowerState;
  cpuPercent: number;
  memoryUsedMb: number;
  uptimeSeconds: number;
}

export interface SnapshotMeta {
  id: string;
  name: string;
  created: string;
  withState: boolean;
}

export interface Vm {
  id: string;
  name: string;
  backend: BackendKind;
  osFamily: OsFamily;
  osId: string;
  isoPath: string | null;
  cpus: number;
  memoryMb: number;
  diskGb: number;
  dir: string;
  diskPath: string;
  created: string;
  vboxId?: string | null;
  hypervId?: string | null;
  imported?: boolean;
  qmpPort?: number | null;
  snapshots: SnapshotMeta[];
  status: VmStatus;
}

export interface Snapshot {
  id: string;
  name: string;
  created: string;
  withState: boolean;
}

export interface CreateSpec {
  name: string;
  osFamily: OsFamily;
  osId: string;
  isoPath: string | null;
  cpus: number;
  memoryMb: number;
  diskGb: number;
}

export interface CreateProgress {
  step: string;
  label: string;
  state: "pending" | "active" | "done" | "error";
}

export interface VmChanges {
  cpus: number;
  memoryMb: number;
  diskGb: number;
}

export interface IsoInfo {
  path: string;
  fileName: string;
  sizeMb: number;
}
