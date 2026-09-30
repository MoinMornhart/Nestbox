import { useCallback, useEffect, useState } from "react";
import { ArrowUpCircle, Check, RefreshCw } from "lucide-react";
import { checkForUpdate, currentVersion, type AvailableUpdate } from "../lib/updater";
import { toAppError } from "../lib/api";
import type { AppError } from "../lib/types";
import { Button, Dialog } from "./ui";
import { ErrorPanel } from "./feedback";

export interface UpdateState {
  version: string;
  update: AvailableUpdate | null;
  checking: boolean;
  /** Ergebnis der letzten manuellen Suche */
  lastResult: "none" | "found" | "error" | null;
  error: AppError | null;
  check: (manual?: boolean) => Promise<AvailableUpdate | null>;
}

/** Sucht beim Start (still) und auf Knopfdruck nach einer neuen Version. */
export function useUpdates(): UpdateState {
  const [version, setVersion] = useState("");
  const [update, setUpdate] = useState<AvailableUpdate | null>(null);
  const [checking, setChecking] = useState(false);
  const [lastResult, setLastResult] = useState<UpdateState["lastResult"]>(null);
  const [error, setError] = useState<AppError | null>(null);

  const check = useCallback(async (manual = false) => {
    setChecking(true);
    setError(null);
    try {
      const u = await checkForUpdate();
      setUpdate(u);
      if (manual) setLastResult(u ? "found" : "none");
      return u;
    } catch (e) {
      // Beim stillen Prüfen (z. B. offline) keine Fehlermeldung zeigen.
      if (manual) {
        setError({
          ...toAppError(e),
          title: "Die Suche nach Updates hat nicht geklappt",
          hint: "Prüfe deine Internetverbindung und versuche es noch einmal. Updates gibt es auch jederzeit auf der Download-Seite.",
        });
        setLastResult("error");
      }
      return null;
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    currentVersion().then(setVersion).catch(() => {});
    const t = setTimeout(() => void check(false), 3000);
    return () => clearTimeout(t);
  }, [check]);

  return { version, update, checking, lastResult, error, check };
}

/** Hinweis oben in der Kopfzeile, wenn ein Update bereitliegt. */
export function UpdatePill({ update, onClick }: { update: AvailableUpdate; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className="inline-flex items-center gap-1.5 rounded-full bg-accent-soft px-3 py-1 text-[12.5px] font-semibold text-accent-text transition-transform hover:scale-[1.03] anim-pop"
    >
      <ArrowUpCircle className="size-3.5" />
      Update auf {update.version}
    </button>
  );
}

export function UpdateDialog({ update, current, onClose }: { update: AvailableUpdate; current: string; onClose: () => void }) {
  const [phase, setPhase] = useState<"ready" | "installing" | "done">("ready");
  const [progress, setProgress] = useState<number | null>(0);
  const [error, setError] = useState<AppError | null>(null);

  const install = async () => {
    setPhase("installing");
    setError(null);
    try {
      await update.install(setProgress);
      setPhase("done");
    } catch (e) {
      setError({
        ...toAppError(e),
        title: "Das Update konnte nicht installiert werden",
        hint: "Versuche es noch einmal. Klappt es nicht, lade die neue Version von der Download-Seite und installiere sie über die alte.",
      });
      setPhase("ready");
    }
  };

  return (
    <Dialog
      open
      onClose={() => phase === "ready" && onClose()}
      dismissable={phase === "ready"}
      width={480}
      title={phase === "done" ? "Update installiert" : `Nestbox ${update.version} ist da`}
      description={
        phase === "done"
          ? "Nestbox startet gleich neu. Deine VMs und Einstellungen bleiben erhalten."
          : `Du nutzt gerade Version ${current || "?"}. Das Update dauert nur einen Moment – laufende VMs laufen dabei weiter.`
      }
      icon={
        <span className="flex size-10 items-center justify-center rounded-full bg-accent-soft">
          {phase === "done" ? <Check className="size-5 text-ok" /> : <ArrowUpCircle className="size-5 text-accent-text" />}
        </span>
      }
      footer={
        phase === "ready" ? (
          <>
            <Button variant="ghost" onClick={onClose}>
              Später
            </Button>
            <Button variant="primary" icon={<RefreshCw className="size-4" />} onClick={install}>
              Jetzt aktualisieren
            </Button>
          </>
        ) : undefined
      }
    >
      {update.notes && phase === "ready" && (
        <div className="max-h-48 overflow-y-auto whitespace-pre-wrap rounded-xl bg-bg-subtle p-3.5 text-[13px] text-text-2 selectable">{update.notes}</div>
      )}
      {phase === "installing" && (
        <div className="py-2">
          <div className="mb-2 flex justify-between text-[13px] text-text-2">
            <span>Wird heruntergeladen und installiert …</span>
            <span className="tabular-nums">{progress === null ? "" : `${progress} %`}</span>
          </div>
          <div className="h-2 overflow-hidden rounded-full bg-line">
            <div
              className={progress === null ? "h-full w-1/3 rounded-full bg-accent anim-pulse" : "h-full rounded-full bg-accent transition-[width] duration-300"}
              style={progress === null ? undefined : { width: `${progress}%` }}
            />
          </div>
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
