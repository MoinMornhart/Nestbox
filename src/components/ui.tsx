import { useEffect, useLayoutEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Loader2 } from "lucide-react";

export function cx(...c: (string | false | null | undefined)[]) {
  return c.filter(Boolean).join(" ");
}

// ── Buttons ──

type Variant = "primary" | "secondary" | "ghost" | "danger" | "subtle";

export function Button({
  variant = "secondary",
  size = "md",
  loading,
  icon,
  children,
  className,
  disabled,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: "sm" | "md" | "lg"; loading?: boolean; icon?: ReactNode }) {
  const v: Record<Variant, string> = {
    primary: "bg-accent text-white hover:bg-accent-hover shadow-[0_1px_2px_rgb(0_0_0/0.12),inset_0_1px_0_rgb(255_255_255/0.15)]",
    secondary: "bg-surface text-text border border-line-strong hover:bg-surface-2 hover:border-line-strong shadow-[0_1px_2px_rgb(0_0_0/0.04)]",
    ghost: "text-text-2 hover:bg-bg-subtle hover:text-text",
    danger: "bg-err text-white hover:brightness-110",
    subtle: "bg-accent-soft text-accent-text hover:brightness-[0.97]",
  };
  const s = { sm: "h-8 px-3 text-[13px] gap-1.5 rounded-lg", md: "h-9 px-4 text-sm gap-2 rounded-[10px]", lg: "h-11 px-5 text-[15px] gap-2 rounded-xl" }[size];
  return (
    <button
      {...rest}
      disabled={disabled || loading}
      className={cx(
        "inline-flex items-center justify-center font-medium whitespace-nowrap transition-all duration-150 active:scale-[0.98] disabled:opacity-50 disabled:pointer-events-none select-none",
        v[variant],
        s,
        className,
      )}
    >
      {loading ? <Loader2 className="size-4 anim-spin" /> : icon}
      {children}
    </button>
  );
}

export function IconButton({
  label,
  children,
  className,
  tooltipSide = "top",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; tooltipSide?: "top" | "bottom" }) {
  return (
    <Tooltip text={label} side={tooltipSide}>
      <button
        aria-label={label}
        {...rest}
        className={cx(
          "inline-flex size-8 items-center justify-center rounded-lg text-text-2 transition-colors hover:bg-bg-subtle hover:text-text disabled:opacity-40 disabled:pointer-events-none",
          className,
        )}
      >
        {children}
      </button>
    </Tooltip>
  );
}

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cx("anim-spin text-muted", className ?? "size-4")} />;
}

// ── Tooltip ──

export function Tooltip({ text, children, side = "top", wide }: { text: ReactNode; children: ReactNode; side?: "top" | "bottom"; wide?: boolean }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const timer = useRef<number>(0);
  const show = () => {
    timer.current = window.setTimeout(() => {
      const r = ref.current?.getBoundingClientRect();
      if (r) setPos({ x: r.left + r.width / 2, y: side === "top" ? r.top : r.bottom });
    }, 350);
  };
  const hide = () => {
    clearTimeout(timer.current);
    setPos(null);
  };
  return (
    <span ref={ref} className="inline-flex" onMouseEnter={show} onMouseLeave={hide} onMouseDown={hide} onClick={hide}>
      {children}
      {pos &&
        createPortal(
          <div
            role="tooltip"
            className={cx(
              "pointer-events-none fixed z-[100] rounded-lg bg-[#2a241f] px-2.5 py-1.5 text-[12px] leading-snug text-[#f6f1ea] shadow-pop anim-fade dark:bg-[#3a332c]",
              wide ? "max-w-[300px]" : "max-w-[240px]",
            )}
            style={{
              left: Math.min(Math.max(pos.x, 130), window.innerWidth - 130),
              top: pos.y + (side === "top" ? -8 : 8),
              transform: `translate(-50%, ${side === "top" ? "-100%" : "0"})`,
            }}
          >
            {text}
          </div>,
          document.body,
        )}
    </span>
  );
}

/** Kleines Fragezeichen mit Erklärung für Fachbegriffe */
export function Info({ text }: { text: ReactNode }) {
  return (
    <Tooltip text={text} wide>
      <span className="ml-1 inline-flex size-4 items-center justify-center rounded-full border border-line-strong text-[10px] font-semibold text-muted hover:text-text">
        ?
      </span>
    </Tooltip>
  );
}

// ── Dialog ──

export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 460,
  icon,
  dismissable = true,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  width?: number;
  icon?: ReactNode;
  dismissable?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && dismissable) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose, dismissable]);

  if (!open) return null;
  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
      <div className="absolute inset-0 bg-[var(--overlay)] backdrop-blur-[2px] anim-fade" onClick={() => dismissable && onClose()} />
      <div
        role="dialog"
        aria-modal="true"
        className="relative flex max-h-full w-full flex-col overflow-hidden rounded-2xl border border-line bg-surface shadow-pop anim-pop"
        style={{ maxWidth: width }}
      >
        <div className="flex gap-3.5 px-6 pt-6">
          {icon && <div className="mt-0.5 shrink-0">{icon}</div>}
          <div className="min-w-0">
            <h2 className="font-display text-[17px] font-semibold tracking-[-0.01em]">{title}</h2>
            {description && <p className="mt-1 text-text-2">{description}</p>}
          </div>
        </div>
        {children && <div className="min-h-0 overflow-y-auto px-6 pt-4">{children}</div>}
        {footer && <div className="mt-2 flex items-center justify-end gap-2 px-6 pb-5 pt-4">{footer}</div>}
        {!footer && <div className="pb-6" />}
      </div>
    </div>,
    document.body,
  );
}

// ── Kontextmenü ──

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
  hint?: string;
  separatorBefore?: boolean;
}

export function Menu({ anchor, items, onClose }: { anchor: { x: number; y: number } | null; items: MenuItem[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);

  useLayoutEffect(() => {
    if (!anchor || !ref.current) return;
    const r = ref.current.getBoundingClientRect();
    setPos({
      x: Math.min(anchor.x, window.innerWidth - r.width - 8),
      y: anchor.y + r.height > window.innerHeight - 8 ? Math.max(8, anchor.y - r.height) : anchor.y,
    });
  }, [anchor]);

  useEffect(() => {
    if (!anchor) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", key);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", onClose);
    };
  }, [anchor, onClose]);

  if (!anchor) return null;
  return createPortal(
    <div
      ref={ref}
      role="menu"
      className="fixed z-[60] min-w-[220px] rounded-xl border border-line bg-surface p-1.5 shadow-pop anim-pop"
      style={{ left: (pos ?? anchor).x, top: (pos ?? anchor).y, visibility: pos ? "visible" : "hidden" }}
    >
      {items.map((it, i) => (
        <div key={i}>
          {it.separatorBefore && <div className="mx-2 my-1.5 h-px bg-line" />}
          <button
            role="menuitem"
            disabled={it.disabled}
            onClick={() => {
              onClose();
              it.onClick();
            }}
            className={cx(
              "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-[7px] text-left text-[13.5px] transition-colors disabled:opacity-40",
              it.danger ? "text-err hover:bg-err-soft" : "text-text hover:bg-bg-subtle",
            )}
          >
            <span className={cx("flex size-4 items-center justify-center", it.danger ? "text-err" : "text-text-2")}>{it.icon}</span>
            <span className="flex-1">{it.label}</span>
            {it.hint && <span className="text-[12px] text-muted">{it.hint}</span>}
          </button>
        </div>
      ))}
    </div>,
    document.body,
  );
}

// ── Formularelemente ──

export function TextInput({
  value,
  onChange,
  error,
  autoFocus,
  placeholder,
  onEnter,
  className,
}: {
  value: string;
  onChange: (v: string) => void;
  error?: string | null;
  autoFocus?: boolean;
  placeholder?: string;
  onEnter?: () => void;
  className?: string;
}) {
  return (
    <div className={className}>
      <input
        value={value}
        autoFocus={autoFocus}
        placeholder={placeholder}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && onEnter?.()}
        onFocus={(e) => e.target.select()}
        className={cx(
          "h-11 w-full rounded-xl border bg-surface px-3.5 text-[15px] text-text outline-none transition-shadow placeholder:text-muted",
          error ? "border-err shadow-[0_0_0_3px_var(--err-soft)]" : "border-line-strong focus:border-accent focus:shadow-[0_0_0_3px_var(--accent-soft)]",
        )}
      />
      <div className={cx("overflow-hidden text-[13px] text-err transition-all", error ? "mt-1.5 max-h-10" : "max-h-0")}>{error}</div>
    </div>
  );
}

export function Slider({
  label,
  info,
  value,
  min,
  max,
  step = 1,
  format,
  onChange,
  disabled,
}: {
  label: string;
  info?: ReactNode;
  value: number;
  min: number;
  max: number;
  step?: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
  disabled?: boolean;
}) {
  const fill = max > min ? ((value - min) / (max - min)) * 100 : 100;
  return (
    <label className={cx("block", disabled && "opacity-50")}>
      <div className="mb-2 flex items-center justify-between">
        <span className="flex items-center text-[13px] font-medium text-text-2">
          {label}
          {info && <Info text={info} />}
        </span>
        <span className="font-display text-[14px] font-semibold tabular-nums">{format(value)}</span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled || max <= min}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ ["--fill" as string]: `${fill}%` }}
      />
      <div className="mt-1 flex justify-between text-[11px] text-muted tabular-nums">
        <span>{format(min)}</span>
        <span>{format(max)}</span>
      </div>
    </label>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="inline-flex rounded-[10px] bg-bg-subtle p-1">
      {options.map((o) => (
        <button
          key={o.value}
          onClick={() => onChange(o.value)}
          className={cx(
            "rounded-lg px-3.5 py-1.5 text-[13px] font-medium transition-all",
            value === o.value ? "bg-surface text-text shadow-card" : "text-text-2 hover:text-text",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Checkbox({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex cursor-default items-start gap-3 rounded-xl border border-line p-3.5 transition-colors hover:bg-surface-2">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-4 shrink-0 accent-[var(--accent)]"
      />
      <span className="text-[13.5px]">{children}</span>
    </label>
  );
}

// ── Kleine Anzeigen ──

export function Meter({ value, label, detail }: { value: number; label: string; detail: string }) {
  const v = Math.max(0, Math.min(100, value));
  return (
    <div className="min-w-0 flex-1">
      <div className="mb-1 flex items-baseline justify-between gap-2 text-[11.5px]">
        <span className="text-muted">{label}</span>
        <span className="truncate tabular-nums text-text-2">{detail}</span>
      </div>
      <div className="h-1 overflow-hidden rounded-full bg-line">
        <div className="h-full rounded-full bg-accent/70 transition-[width] duration-700 ease-out" style={{ width: `${v}%` }} />
      </div>
    </div>
  );
}
