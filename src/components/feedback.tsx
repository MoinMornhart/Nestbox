import { createContext, useCallback, useContext, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, Check, ChevronRight, Copy, X } from "lucide-react";
import type { AppError } from "../lib/types";
import { cx } from "./ui";

/** Fehlermeldung mit Lösungsvorschlag; technische Details ausklappbar. */
export function ErrorPanel({ error, onClose, compact }: { error: AppError; onClose?: () => void; compact?: boolean }) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(`${error.title}\n\n${error.hint}\n\n${error.details ?? ""}`);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* Zwischenablage nicht verfügbar */
    }
  };
  return (
    <div className={cx("rounded-xl border border-err/25 bg-err-soft anim-rise", compact ? "p-3.5" : "p-4")}>
      <div className="flex gap-3">
        <AlertTriangle className="mt-0.5 size-[18px] shrink-0 text-err" />
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-text">{error.title}</div>
          <div className="mt-1 text-[13.5px] text-text-2">
            <span className="font-medium text-text">So klappt es: </span>
            {error.hint}
          </div>
          {error.details && (
            <div className="mt-2.5">
              <button onClick={() => setOpen(!open)} className="inline-flex items-center gap-1 text-[12.5px] font-medium text-text-2 hover:text-text">
                <ChevronRight className={cx("size-3.5 transition-transform", open && "rotate-90")} />
                Technische Details
              </button>
              {open && (
                <div className="relative mt-2 anim-fade">
                  <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-surface/70 p-3 pr-10 font-mono text-[11.5px] leading-relaxed text-text-2">
                    {error.details}
                  </pre>
                  <button
                    onClick={copy}
                    title="Kopieren"
                    className="absolute right-2 top-2 rounded-md p-1.5 text-muted hover:bg-bg-subtle hover:text-text"
                  >
                    {copied ? <Check className="size-3.5 text-ok" /> : <Copy className="size-3.5" />}
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
        {onClose && (
          <button onClick={onClose} className="h-fit rounded-md p-1 text-muted hover:bg-surface/60 hover:text-text" aria-label="Schließen">
            <X className="size-4" />
          </button>
        )}
      </div>
    </div>
  );
}

// ── Toasts ──

interface Toast {
  id: number;
  kind: "ok" | "error";
  text?: string;
  error?: AppError;
}

const ToastCtx = createContext<{ ok: (t: string) => void; error: (e: AppError) => void }>({ ok: () => {}, error: () => {} });

export function useToast() {
  return useContext(ToastCtx);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const remove = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), []);
  const push = useCallback(
    (t: Omit<Toast, "id">, ms: number | null) => {
      const id = Date.now() + Math.random();
      setToasts((list) => [...list.slice(-3), { ...t, id }]);
      if (ms) setTimeout(() => remove(id), ms);
    },
    [remove],
  );
  const api = {
    ok: useCallback((text: string) => push({ kind: "ok", text }, 3200), [push]),
    error: useCallback((error: AppError) => push({ kind: "error", error }, null), [push]),
  };
  return (
    <ToastCtx.Provider value={api}>
      {children}
      {createPortal(
        <div className="pointer-events-none fixed bottom-5 right-5 z-[70] flex w-[400px] max-w-[calc(100vw-40px)] flex-col gap-2.5">
          {toasts.map((t) =>
            t.kind === "ok" ? (
              <div key={t.id} className="pointer-events-auto ml-auto flex items-center gap-2.5 rounded-xl border border-line bg-surface px-4 py-3 shadow-pop anim-toast">
                <span className="flex size-5 items-center justify-center rounded-full bg-ok-soft">
                  <Check className="size-3.5 text-ok" />
                </span>
                <span className="text-[13.5px] font-medium">{t.text}</span>
              </div>
            ) : (
              <div key={t.id} className="pointer-events-auto rounded-xl bg-surface shadow-pop anim-toast">
                <ErrorPanel error={t.error!} compact onClose={() => remove(t.id)} />
              </div>
            ),
          )}
        </div>,
        document.body,
      )}
    </ToastCtx.Provider>
  );
}
