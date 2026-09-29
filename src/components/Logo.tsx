/** Das Nestbox-Vogelhaus */
export function NestboxLogo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden>
      <defs>
        <linearGradient id="nb-bg" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#d98a4c" />
          <stop offset="1" stopColor="#b8612b" />
        </linearGradient>
      </defs>
      <rect x="32" y="32" width="960" height="960" rx="220" fill="url(#nb-bg)" />
      <path d="M512 214 790 452v318a36 36 0 0 1-36 36H270a36 36 0 0 1-36-36V452z" fill="#fff8f0" />
      <path d="M190 470 512 196l322 274" fill="none" stroke="#fff8f0" strokeWidth="64" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="512" cy="540" r="92" fill="#b8612b" />
      <rect x="490" y="682" width="44" height="92" rx="22" fill="#b8612b" />
    </svg>
  );
}

/** Große, ruhige Illustration für leere Zustände */
export function EmptyNest() {
  return (
    <svg width="168" height="140" viewBox="0 0 168 140" aria-hidden className="text-accent">
      <ellipse cx="84" cy="128" rx="60" ry="7" fill="currentColor" opacity=".08" />
      <path d="M84 16 128 54v62a6 6 0 0 1-6 6H46a6 6 0 0 1-6-6V54z" fill="var(--surface)" stroke="var(--line-strong)" strokeWidth="2" />
      <path d="M32 58 84 14l52 44" fill="none" stroke="currentColor" strokeWidth="7" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx="84" cy="70" r="14" fill="var(--bg-subtle)" stroke="var(--line-strong)" strokeWidth="2" />
      <rect x="81" y="94" width="6" height="16" rx="3" fill="currentColor" opacity=".7" />
      <path d="M128 36c6-6 16-6 20 0-6 1-10 4-12 9" fill="none" stroke="var(--muted)" strokeWidth="2" strokeLinecap="round" opacity=".6" />
    </svg>
  );
}
