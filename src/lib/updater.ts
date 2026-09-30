// Updates über das Tauri-Updater-Plugin: prüft latest.json auf GitHub, lädt das
// signierte Installationspaket und startet Nestbox neu. Im Browser (Mock) simuliert.
import { inTauri } from "./api";

export interface AvailableUpdate {
  version: string;
  notes: string;
  date: string | null;
  /** Lädt und installiert das Update; `onProgress` bekommt 0–100 (oder null, wenn die Größe unbekannt ist). */
  install: (onProgress: (percent: number | null) => void) => Promise<void>;
}

export async function currentVersion(): Promise<string> {
  if (!inTauri) return "0.9.4";
  const { getVersion } = await import("@tauri-apps/api/app");
  return getVersion();
}

export async function checkForUpdate(): Promise<AvailableUpdate | null> {
  if (!inTauri) {
    if (new URLSearchParams(location.search).get("update") !== "1") return null;
    return {
      version: "0.9.5",
      notes: "• VirtualBox-Installation zuverlässiger\n• Hyper-V-VMs übernehmen\n• Kleinere Verbesserungen",
      date: new Date().toISOString(),
      install: async (onProgress) => {
        for (let p = 0; p <= 100; p += 10) {
          onProgress(p);
          await new Promise((r) => setTimeout(r, 180));
        }
      },
    };
  }
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (!update) return null;
  return {
    version: update.version,
    notes: update.body ?? "",
    date: update.date ?? null,
    install: async (onProgress) => {
      let total = 0;
      let done = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          onProgress(total ? 0 : null);
        } else if (event.event === "Progress") {
          done += event.data.chunkLength;
          onProgress(total ? Math.min(100, Math.round((done / total) * 100)) : null);
        } else if (event.event === "Finished") {
          onProgress(100);
        }
      });
      // Unter Windows beendet das Installationsprogramm Nestbox selbst; falls nicht, hier neu starten.
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}
