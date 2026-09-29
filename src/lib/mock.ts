// Mock-Modus: simuliert das Tauri-Backend im Browser (npm run dev).
// Szenario per URL wählbar: ?mock=bereit | leer | einrichtung | qemu | vm | neustart
// Fehler erzwingen: ?fail=start (bzw. create, snapshot, …)
import type { AppError, BackendKind, CreateProgress, CreateSpec, HostInfo, Settings, Snapshot, Vm, VmStatus } from "./types";

const params = new URLSearchParams(typeof location !== "undefined" ? location.search : "");
const scenario = params.get("mock") ?? "bereit";
const failOn = params.get("fail") ?? "";

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = Date.now();
const iso = (minsAgo: number) => new Date(now - minsAgo * 60_000).toISOString();

const VBOX = "C:\\Program Files\\Oracle\\VirtualBox\\VBoxManage.exe";
const QEMU = "C:\\Program Files\\qemu\\qemu-system-x86_64.exe";

const baseHost: HostInfo = {
  windowsName: "Windows 11 Home",
  editionId: "Core",
  build: "26200",
  isHome: true,
  whpxFeature: "disabled",
  virtualizationEnabled: true,
  windowsHypervisorRunning: false,
  isVirtualMachine: false,
  machineName: "LENOVO ThinkPad T14s",
  vboxPath: VBOX,
  vboxVersion: "7.1.4r165100",
  qemuPath: null,
  qemuFirmware: false,
  qemuBroken: false,
  qemuAccelerated: false,
  wingetAvailable: true,
  totalMemoryMb: 32768,
  logicalCores: 16,
  cpuName: "AMD Ryzen 7 7840U",
  freeDiskGb: 412.5,
  rebootPending: false,
  vboxReady: true,
  qemuReady: false,
  activeBackend: "virtualbox",
  backendChoice: "auto",
};

let host: HostInfo = { ...baseHost };
if (scenario === "einrichtung") {
  // Frischer PC: weder VirtualBox noch QEMU installiert
  host = { ...host, vboxPath: null, vboxVersion: null, vboxReady: false };
} else if (scenario === "qemu") {
  // Nur QEMU installiert, Windows-Hypervisor-Plattform noch aus
  host = { ...host, vboxPath: null, vboxVersion: null, vboxReady: false, qemuPath: QEMU, qemuFirmware: true, qemuReady: true, activeBackend: "qemu" };
} else if (scenario === "vm") {
  // Windows läuft selbst in einer VM ohne durchgereichte Virtualisierung
  host = {
    ...host,
    windowsName: "Windows 11 Pro",
    editionId: "Professional",
    isHome: false,
    virtualizationEnabled: false,
    isVirtualMachine: true,
    machineName: "QEMU Standard PC (Q35 + ICH9, 2009)",
    cpuName: "QEMU Virtual CPU version 2.5+",
    vboxPath: null,
    vboxVersion: null,
    vboxReady: false,
    logicalCores: 6,
    totalMemoryMb: 14932,
  };
} else if (scenario === "neustart") {
  host = { ...host, vboxPath: null, vboxVersion: null, vboxReady: false, qemuPath: QEMU, qemuFirmware: true, qemuReady: false, rebootPending: true, activeBackend: "qemu" };
}

let settings: Settings = {
  vmDir: "C:\\Users\\Demo\\Nestbox\\VMs",
  backend: "auto",
  qemuDir: "",
  rebootPendingSince: null,
  setupDone: scenario === "bereit" || scenario === "leer",
};

const status = (id: string, state: VmStatus["state"], cpu = 0, mem = 0, up = 0): VmStatus => ({
  id,
  state,
  cpuPercent: cpu,
  memoryUsedMb: mem,
  uptimeSeconds: up,
});

function vm(p: Partial<Vm> & Pick<Vm, "id" | "name" | "osId" | "osFamily">, st: VmStatus["state"], cpu = 0, mem = 0): Vm {
  return {
    backend: host.activeBackend,
    isoPath: null,
    cpus: 4,
    memoryMb: 8192,
    diskGb: 64,
    dir: `C:\\Users\\Demo\\Nestbox\\VMs\\${p.name}`,
    diskPath: `C:\\Users\\Demo\\Nestbox\\VMs\\${p.name}\\${p.name}.vdi`,
    created: iso(60 * 24 * 3),
    snapshots: [],
    ...p,
    status: status(p.id, st, cpu, mem, st === "running" ? 5400 : 0),
  };
}

let vms: Vm[] =
  ["leer", "einrichtung", "qemu", "neustart", "vm"].includes(scenario)
    ? []
    : [
        vm({ id: "a1", name: "Ubuntu 24.04", osId: "ubuntu", osFamily: "linux", cpus: 4, memoryMb: 8192, isoPath: "C:\\Users\\Demo\\Downloads\\ubuntu-24.04.3-desktop-amd64.iso" }, "running", 23, 6120),
        vm({ id: "b2", name: "Windows 11 Test", osId: "windows11", osFamily: "windows", cpus: 4, memoryMb: 8192, diskGb: 80 }, "paused", 0, 8192),
        vm({ id: "c3", name: "Fedora Workstation", osId: "fedora", osFamily: "linux", cpus: 2, memoryMb: 4096, diskGb: 48 }, "off"),
        vm({ id: "d4", name: "Linux Mint", osId: "mint", osFamily: "linux", cpus: 2, memoryMb: 4096, diskGb: 40 }, "off"),
      ];

const snapshots: Record<string, Snapshot[]> = {
  a1: [
    { id: "s1", name: "Frisch installiert", created: iso(60 * 26), withState: true },
    { id: "s2", name: "Vor dem großen Update", created: iso(60 * 3), withState: true },
  ],
  b2: [{ id: "s3", name: "Nach der Einrichtung", created: iso(60 * 50), withState: true }],
};

function err(title: string, hint: string, details?: string): AppError {
  return { title, hint, details };
}

function maybeFail(what: string) {
  if (failOn === what) {
    throw err(
      "Die VM konnte nicht gestartet werden",
      "Es ist gerade nicht genug freier Arbeitsspeicher da. Schließe andere Programme oder VMs, oder gib der VM in den Einstellungen weniger Arbeitsspeicher.",
      "VBoxManage.exe: error: The VM session was aborted.\nVBoxManage.exe: error: Not enough memory to start the VM (VERR_NO_MEMORY)\nVBoxManage.exe: error: Details: code E_FAIL (0x80004005), component SessionMachine, interface ISession",
    );
  }
}

function find(id: string) {
  const v = vms.find((x) => x.id === id);
  if (!v) throw err("Diese VM gibt es nicht mehr", "Die Liste wird neu geladen.");
  return v;
}

function setState(id: string, state: VmStatus["state"]) {
  const v = find(id);
  v.status = status(id, state, state === "running" ? 12 : 0, state === "off" ? 0 : Math.round(v.memoryMb * 0.7), state === "running" ? 10 : 0);
}

function jitter() {
  for (const v of vms) {
    if (v.status.state === "running") {
      v.status.cpuPercent = Math.max(1, Math.min(96, Math.round(v.status.cpuPercent + (Math.random() - 0.5) * 14)));
      v.status.memoryUsedMb = Math.round(v.memoryMb * (0.6 + Math.random() * 0.15));
      v.status.uptimeSeconds += 2;
    }
  }
}

export async function mockCall<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const id = args.id as string;
  await wait(cmd === "vm_status" ? 60 : 180);
  switch (cmd) {
    case "get_host_info":
      await wait(700);
      return { ...host, backendChoice: settings.backend } as T;
    case "install_software":
      await wait(2500);
      if (args.kind === "virtualbox") host = { ...host, vboxPath: VBOX, vboxVersion: "7.1.4r165100", vboxReady: host.virtualizationEnabled, activeBackend: "virtualbox" };
      else host = { ...host, qemuPath: QEMU, qemuFirmware: true, qemuReady: true, activeBackend: host.vboxReady ? host.activeBackend : "qemu" };
      return undefined as T;
    case "fix_enable_whpx":
      await wait(1400);
      host = { ...host, rebootPending: true };
      return undefined as T;
    case "restart_computer":
      return undefined as T;
    case "get_settings":
      return { ...settings } as T;
    case "save_settings":
      settings = { ...(args.newSettings as Settings) };
      return { ...settings } as T;
    case "get_log_path":
      return "C:\\Users\\Demo\\AppData\\Local\\Nestbox\\logs\\nestbox.log" as T;
    case "list_vms":
      return structuredClone(vms) as T;
    case "vm_status":
      jitter();
      return vms.map((v) => ({ ...v.status })) as T;
    case "check_vm_name": {
      const n = String(args.name ?? "").trim();
      if (!n) return "Bitte gib einen Namen ein." as T;
      if (/[<>:"/\\|?*']/.test(n)) return "Diese Zeichen sind nicht erlaubt: < > : \" / \\ | ? * '" as T;
      if (vms.some((v) => v.name.toLowerCase() === n.toLowerCase() && v.id !== args.exceptId))
        return "Du hast schon eine VM mit diesem Namen." as T;
      return null as T;
    }
    case "inspect_iso": {
      const p = String(args.path);
      if (!p.toLowerCase().endsWith(".iso"))
        throw err("Das ist keine ISO-Datei", "Wähle eine Datei mit der Endung „.iso“ – das Abbild einer Installations-DVD.");
      return { path: p, fileName: p.split("\\").pop(), sizeMb: 5892 } as T;
    }
    case "start_vm":
      maybeFail("start");
      await wait(600);
      setState(id, "running");
      return undefined as T;
    case "pause_vm":
      setState(id, "paused");
      return undefined as T;
    case "resume_vm":
      setState(id, "running");
      return undefined as T;
    case "shutdown_vm":
      await wait(1200);
      setState(id, "off");
      return undefined as T;
    case "poweroff_vm":
      setState(id, "off");
      return undefined as T;
    case "open_console":
      return undefined as T;
    case "rename_vm":
      find(id).name = String(args.name);
      return undefined as T;
    case "update_vm": {
      const v = find(id);
      if (v.status.state !== "off") throw err("Die VM muss dafür ausgeschaltet sein", "Fahre die VM herunter und ändere die Einstellungen dann erneut.");
      Object.assign(v, args.changes);
      return undefined as T;
    }
    case "install_guest_tools":
      await wait(500);
      return undefined as T;
    case "eject_iso":
      find(id).isoPath = null;
      return undefined as T;
    case "delete_vm":
      await wait(700);
      vms = vms.filter((v) => v.id !== id);
      return undefined as T;
    case "list_snapshots":
      return [...(snapshots[id] ?? [])].sort((a, b) => b.created.localeCompare(a.created)) as T;
    case "create_snapshot": {
      maybeFail("snapshot");
      await wait(900);
      const s: Snapshot = {
        id: `s${Math.random().toString(36).slice(2, 7)}`,
        name: String(args.name || `Sicherungspunkt vom ${new Date().toLocaleString("de-DE", { dateStyle: "short", timeStyle: "short" })}`),
        created: new Date().toISOString(),
        withState: true,
      };
      (snapshots[id] ??= []).push(s);
      return s as T;
    }
    case "restore_snapshot":
      await wait(900);
      return undefined as T;
    case "delete_snapshot":
      snapshots[id] = (snapshots[id] ?? []).filter((s) => s.id !== args.snapshotId);
      return undefined as T;
    default:
      throw err("Unbekannter Befehl", "Nur im Mock-Modus.", cmd);
  }
}

export async function mockCreate(spec: CreateSpec, backend: BackendKind, onProgress: (p: CreateProgress) => void): Promise<Vm> {
  const steps: [string, string][] = [
    ["folder", "Ordner vorbereiten"],
    ["disk", "Virtuelle Festplatte anlegen"],
    ["vm", "Virtuelle Maschine anlegen"],
    ...(backend === "virtualbox" && spec.osFamily === "windows" ? ([["security", "TPM & Secure Boot einrichten"]] as [string, string][]) : []),
    ["iso", "Installationsmedium einlegen"],
    ["start", "VM starten"],
  ];
  const slow = params.get("langsam") === "1";
  for (const [step, label] of steps) {
    onProgress({ step, label, state: "active" });
    await wait(slow ? 60_000 : 650);
    if (failOn === "create" && step === "vm") {
      throw err(
        "Virtuelle Maschine einrichten",
        "Es ist gerade nicht genug freier Arbeitsspeicher da. Schließe andere Programme oder VMs, oder gib der VM weniger Arbeitsspeicher.",
        "VBoxManage.exe: error: Not enough memory to start the VM (VERR_NO_MEMORY)\nVBoxManage.exe: error: Details: code E_FAIL (0x80004005), component ConsoleWrap, interface IConsole",
      );
    }
    onProgress({ step, label, state: "done" });
  }
  const v = vm(
    { id: `n${Date.now()}`, name: spec.name, osId: spec.osId, osFamily: spec.osFamily, cpus: spec.cpus, memoryMb: spec.memoryMb, diskGb: spec.diskGb, isoPath: spec.isoPath, backend, created: new Date().toISOString() },
    "running",
    35,
    Math.round(spec.memoryMb * 0.5),
  );
  vms = [...vms, v];
  return structuredClone(v);
}
