export type BackendKind = "hyperv" | "qemu";
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
  hypervFeature: "enabled" | "disabled" | "unavailable";
  hypervModule: boolean;
  vmmsRunning: boolean;
  whpxFeature: "enabled" | "disabled" | "unavailable";
  virtualizationEnabled: boolean;
  hypervisorPresent: boolean;
  windowsHypervisorRunning: boolean;
  isVirtualMachine: boolean;
  machineName: string;
  hypervGroupOk: boolean;
  hypervGroupNeedsRelogin: boolean;
  isAdmin: boolean;
  qemuPath: string | null;
  qemuFirmware: boolean;
  totalMemoryMb: number;
  logicalCores: number;
  cpuName: string;
  freeDiskGb: number;
  rebootPending: boolean;
  hypervReady: boolean;
  qemuReady: boolean;
  activeBackend: BackendKind;
  backendChoice: BackendChoice;
}

export interface Settings {
  vmDir: string;
  backend: BackendChoice;
  qemuDir: string;
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
  hypervId?: string | null;
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
