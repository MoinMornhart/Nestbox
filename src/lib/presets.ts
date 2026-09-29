import type { OsFamily } from "./types";

export type PresetId = "light" | "balanced" | "strong";

export interface Resources {
  cpus: number;
  memoryMb: number;
  diskGb: number;
}

export interface Limits {
  maxCpus: number;
  maxMemoryMb: number;
  minCpus: number;
  minMemoryMb: number;
  minDiskGb: number;
  maxDiskGb: number;
  /** Erfüllt der PC die Mindestanforderungen des Betriebssystems überhaupt? */
  hostTooSmall: boolean;
}

const roundDown = (v: number, step: number) => Math.floor(v / step) * step;

/** Grenzen: nie mehr als 50 % der Kerne und des Arbeitsspeichers des PCs. */
export function limitsFor(family: OsFamily, hostCores: number, hostMemoryMb: number): Limits {
  const maxCpus = Math.max(1, Math.floor(hostCores / 2));
  const maxMemoryMb = Math.max(512, roundDown(hostMemoryMb / 2, 512));
  const wantCpus = family === "windows" ? 2 : 1;
  const wantMem = family === "windows" ? 4096 : 1024;
  return {
    maxCpus,
    maxMemoryMb,
    minCpus: Math.min(wantCpus, maxCpus),
    minMemoryMb: Math.min(wantMem, maxMemoryMb),
    minDiskGb: family === "windows" ? 64 : 16,
    maxDiskGb: 1024,
    hostTooSmall: maxCpus < wantCpus || maxMemoryMb < wantMem,
  };
}

/** Die drei Leistungsstufen – automatisch aus der Hardware des PCs berechnet. */
export function presetsFor(family: OsFamily, hostCores: number, hostMemoryMb: number): Record<PresetId, Resources> {
  const l = limitsFor(family, hostCores, hostMemoryMb);
  const clampC = (c: number) => Math.max(l.minCpus, Math.min(l.maxCpus, c));
  const clampM = (m: number) => Math.max(l.minMemoryMb, Math.min(l.maxMemoryMb, m));
  const win = family === "windows";

  const light: Resources = {
    cpus: clampC(2),
    memoryMb: clampM(win ? 4096 : 2048),
    diskGb: win ? 64 : 32,
  };
  const balanced: Resources = {
    cpus: Math.max(light.cpus, clampC(Math.round(hostCores * 0.25))),
    memoryMb: Math.max(light.memoryMb, clampM(roundDown(hostMemoryMb * 0.25, 1024))),
    diskGb: win ? 80 : 64,
  };
  const strong: Resources = {
    cpus: Math.max(balanced.cpus, l.maxCpus),
    memoryMb: Math.max(balanced.memoryMb, roundDown(l.maxMemoryMb, 1024) || l.maxMemoryMb),
    diskGb: win ? 128 : 128,
  };
  return { light, balanced, strong };
}

export function formatMemory(mb: number): string {
  if (mb >= 1024) {
    const gb = mb / 1024;
    return `${Number.isInteger(gb) ? gb : gb.toFixed(1).replace(".", ",")} GB`;
  }
  return `${mb} MB`;
}
