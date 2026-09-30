import { useState } from "react";
import { Disc3, History, MonitorPlay, Loader2, Monitor, MoreHorizontal, Pause, Pencil, Play, Power, PowerOff, Settings2, Trash2, Zap } from "lucide-react";
import { OsLogo } from "../lib/os";
import { formatMemory } from "../lib/presets";
import { BACKEND_LABEL, type PowerState, type Vm } from "../lib/types";
import { Button, cx, IconButton, Menu, Meter, type MenuItem } from "../components/ui";

export type VmAction = "start" | "pause" | "resume" | "shutdown" | "poweroff" | "console" | "rename" | "settings" | "snapshots" | "eject" | "guesttools" | "delete";

const STATE_LABEL: Record<PowerState, string> = {
  running: "Läuft",
  paused: "Pausiert",
  off: "Aus",
  starting: "Startet …",
  stopping: "Fährt herunter …",
  saving: "Speichert …",
  unknown: "Unbekannt",
  missing: "Nicht gefunden",
};

const STATE_DOT: Record<PowerState, string> = {
  running: "bg-ok",
  paused: "bg-warn",
  off: "bg-line-strong",
  starting: "bg-ok anim-pulse",
  stopping: "bg-warn anim-pulse",
  saving: "bg-warn anim-pulse",
  unknown: "bg-line-strong",
  missing: "bg-err",
};

export function StatusPill({ state, busy }: { state: PowerState; busy?: string | null }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-[12.5px] font-medium text-text-2">
      {busy ? <Loader2 className="size-3 anim-spin text-accent" /> : <span className={cx("size-2 rounded-full", STATE_DOT[state])} />}
      {busy ?? STATE_LABEL[state]}
    </span>
  );
}

function uptime(s: number) {
  if (s < 60) return "gerade gestartet";
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `seit ${h} Std. ${m} Min.` : `seit ${m} Min.`;
}

export function VmCard({ vm, busy, onAction, index }: { vm: Vm; busy: string | null; onAction: (a: VmAction) => void; index: number }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const st = vm.status.state;
  const on = st === "running" || st === "starting";
  const paused = st === "paused";
  const off = st === "off" || st === "unknown";
  const missing = st === "missing";

  const items: MenuItem[] = [
    { label: "Anzeigen", icon: <Monitor className="size-4" />, onClick: () => onAction("console"), disabled: !on && !paused },
    { label: "Sicherungspunkte", icon: <History className="size-4" />, onClick: () => onAction("snapshots"), disabled: missing },
    { label: "Umbenennen", icon: <Pencil className="size-4" />, onClick: () => onAction("rename"), separatorBefore: true, disabled: missing },
    { label: "Einstellungen", icon: <Settings2 className="size-4" />, onClick: () => onAction("settings"), disabled: missing },
    ...(vm.backend === "virtualbox"
      ? [{ label: "Gasterweiterungen installieren", icon: <MonitorPlay className="size-4" />, onClick: () => onAction("guesttools"), disabled: !on, hint: "flüssiges Bild" }]
      : []),
    ...(vm.isoPath ? [{ label: "Installationsmedium auswerfen", icon: <Disc3 className="size-4" />, onClick: () => onAction("eject"), disabled: missing }] : []),
    { label: "Sofort ausschalten", icon: <Zap className="size-4" />, onClick: () => onAction("poweroff"), disabled: off || missing, hint: "wie Stecker ziehen", separatorBefore: true },
    { label: "Löschen …", icon: <Trash2 className="size-4" />, onClick: () => onAction("delete"), danger: true },
  ];

  return (
    <div
      onContextMenu={(e) => {
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY });
      }}
      onDoubleClick={() => (on || paused) && onAction("console")}
      className="group flex flex-col rounded-2xl border border-line bg-surface p-5 shadow-card transition-all duration-200 hover:-translate-y-0.5 hover:border-line-strong hover:shadow-[0_2px_4px_rgb(0_0_0/0.04),0_12px_32px_rgb(60_40_20/0.1)] anim-rise"
      style={{ animationDelay: `${index * 50}ms` }}
    >
      <div className="flex items-start gap-3.5">
        <div className="relative">
          <OsLogo osId={vm.osId} family={vm.osFamily} size={46} />
          {on && <span className="absolute -bottom-0.5 -right-0.5 size-3.5 rounded-full border-2 border-surface bg-ok" />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate font-display text-[16px] font-semibold tracking-[-0.01em]" title={vm.name}>
            {vm.name}
          </div>
          <div className="mt-0.5 flex items-center gap-2">
            <StatusPill state={st} busy={busy} />
            {on && !busy && <span className="truncate text-[12px] text-muted">· {uptime(vm.status.uptimeSeconds)}</span>}
            {vm.backend === "hyperv" && !on && <span className="rounded-full bg-bg-subtle px-1.5 py-px text-[11px] font-medium text-muted">Hyper-V</span>}
          </div>
        </div>
        <IconButton
          label="Weitere Aktionen"
          onClick={(e) => {
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
            setMenu({ x: r.right - 220, y: r.bottom + 6 });
          }}
          className="-mr-1.5 -mt-1"
        >
          <MoreHorizontal className="size-[18px]" />
        </IconButton>
      </div>

      <div className="mt-5 flex gap-4">
        {on || paused ? (
          <>
            <Meter label="CPU" value={paused ? 0 : vm.status.cpuPercent} detail={paused ? "–" : `${Math.round(vm.status.cpuPercent)} %`} />
            <Meter
              label="RAM"
              value={(vm.status.memoryUsedMb / vm.memoryMb) * 100}
              detail={`${formatMemory(vm.status.memoryUsedMb)} / ${formatMemory(vm.memoryMb)}`}
            />
          </>
        ) : (
          <div className="flex h-[26px] items-center gap-3 text-[12.5px] text-muted">
            <span>{vm.cpus} {vm.cpus === 1 ? "Kern" : "Kerne"}</span>
            <span className="size-1 rounded-full bg-line-strong" />
            <span>{formatMemory(vm.memoryMb)} RAM</span>
            <span className="size-1 rounded-full bg-line-strong" />
            <span>{vm.diskGb} GB</span>
          </div>
        )}
      </div>

      <div className="mt-5 flex items-center gap-2 border-t border-line pt-4">
        {missing ? (
          <span className="text-[12.5px] text-muted">In {BACKEND_LABEL[vm.backend]} nicht mehr vorhanden</span>
        ) : off ? (
          <Button variant="primary" size="sm" icon={<Play className="size-3.5 fill-current" />} disabled={!!busy} onClick={() => onAction("start")}>
            Starten
          </Button>
        ) : paused ? (
          <Button variant="primary" size="sm" icon={<Play className="size-3.5 fill-current" />} disabled={!!busy} onClick={() => onAction("resume")}>
            Fortsetzen
          </Button>
        ) : (
          <Button variant="secondary" size="sm" icon={<Monitor className="size-3.5" />} disabled={!!busy} onClick={() => onAction("console")}>
            Anzeigen
          </Button>
        )}
        <div className="flex-1" />
        {!missing && (
          <>
            <IconButton label="Sicherungspunkte" onClick={() => onAction("snapshots")}>
              <History className="size-4" />
            </IconButton>
            {on && (
              <IconButton label="Pausieren" disabled={!!busy} onClick={() => onAction("pause")}>
                <Pause className="size-4" />
              </IconButton>
            )}
            {(on || paused) && (
              <IconButton label="Herunterfahren" disabled={!!busy} onClick={() => onAction("shutdown")}>
                <Power className="size-4" />
              </IconButton>
            )}
          </>
        )}
        {missing && (
          <Button size="sm" variant="ghost" icon={<PowerOff className="size-3.5" />} onClick={() => onAction("delete")}>
            Aus Liste entfernen
          </Button>
        )}
      </div>

      <Menu anchor={menu} items={items} onClose={() => setMenu(null)} />
    </div>
  );
}
