import type { OsFamily } from "./types";

export interface OsEntry {
  id: string;
  name: string;
  family: OsFamily;
  tagline: string;
  downloadUrl: string;
  /** Hintergrundfarbe der Logo-Kachel */
  tint: string;
}

export const OS_CATALOG: OsEntry[] = [
  { id: "ubuntu", name: "Ubuntu", family: "linux", tagline: "Beliebt & einsteigerfreundlich", downloadUrl: "https://ubuntu.com/download/desktop", tint: "#E95420" },
  { id: "mint", name: "Linux Mint", family: "linux", tagline: "Vertraut wie Windows", downloadUrl: "https://linuxmint.com/download.php", tint: "#6BB244" },
  { id: "fedora", name: "Fedora", family: "linux", tagline: "Modern & aktuell", downloadUrl: "https://fedoraproject.org/de/workstation/download", tint: "#3C6EB4" },
  { id: "windows11", name: "Windows 11", family: "windows", tagline: "Offizielles Microsoft-Abbild", downloadUrl: "https://www.microsoft.com/de-de/software-download/windows11", tint: "#0A78D4" },
];

export function osById(id: string): OsEntry | undefined {
  return OS_CATALOG.find((o) => o.id === id);
}

/** Schlichte Logos als SVG – erkennbar, aber ohne externe Dateien. */
export function OsLogo({ osId, family, size = 40 }: { osId: string; family: OsFamily; size?: number }) {
  const r = size * 0.28;
  const common = { width: size, height: size, viewBox: "0 0 48 48", "aria-hidden": true } as const;
  switch (osId) {
    case "ubuntu":
      return (
        <svg {...common}>
          <rect width="48" height="48" rx={r * (48 / size)} fill="#E95420" />
          <circle cx="24" cy="24" r="9.5" fill="none" stroke="#fff" strokeWidth="3.4" />
          <circle cx="13.2" cy="24" r="3.6" fill="#fff" stroke="#E95420" strokeWidth="1.6" />
          <circle cx="29.4" cy="14.6" r="3.6" fill="#fff" stroke="#E95420" strokeWidth="1.6" />
          <circle cx="29.4" cy="33.4" r="3.6" fill="#fff" stroke="#E95420" strokeWidth="1.6" />
        </svg>
      );
    case "mint":
      return (
        <svg {...common}>
          <rect width="48" height="48" rx={r * (48 / size)} fill="#6BB244" />
          <path d="M12 14h5v14a5 5 0 0 0 5 5h9a5 5 0 0 0 5-5V20a4 4 0 0 0-8 0v10h-4V20a4 4 0 0 0-4-4" fill="none" stroke="#fff" strokeWidth="3.4" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      );
    case "fedora":
      return (
        <svg {...common}>
          <rect width="48" height="48" rx={r * (48 / size)} fill="#3C6EB4" />
          <circle cx="24" cy="24" r="12.5" fill="#fff" />
          <path d="M22 34V21a4.5 4.5 0 0 1 8.3-2.4M18 26h9" fill="none" stroke="#3C6EB4" strokeWidth="3.4" strokeLinecap="round" />
        </svg>
      );
    case "windows11":
      return (
        <svg {...common}>
          <rect width="48" height="48" rx={r * (48 / size)} fill="#0A78D4" />
          <rect x="12" y="12" width="11" height="11" rx="1" fill="#fff" />
          <rect x="25" y="12" width="11" height="11" rx="1" fill="#fff" />
          <rect x="12" y="25" width="11" height="11" rx="1" fill="#fff" />
          <rect x="25" y="25" width="11" height="11" rx="1" fill="#fff" />
        </svg>
      );
    default:
      return family === "windows" ? (
        <svg {...common}>
          <rect width="48" height="48" rx={r * (48 / size)} fill="#5B6B7C" />
          <rect x="12" y="12" width="11" height="11" rx="1" fill="#fff" opacity=".92" />
          <rect x="25" y="12" width="11" height="11" rx="1" fill="#fff" opacity=".92" />
          <rect x="12" y="25" width="11" height="11" rx="1" fill="#fff" opacity=".92" />
          <rect x="25" y="25" width="11" height="11" rx="1" fill="#fff" opacity=".92" />
        </svg>
      ) : (
        <svg {...common}>
          <rect width="48" height="48" rx={r * (48 / size)} fill="#7A6A58" />
          <circle cx="24" cy="24" r="11" fill="none" stroke="#fff" strokeWidth="3" />
          <circle cx="24" cy="24" r="3" fill="#fff" />
        </svg>
      );
  }
}

export interface OsGuess {
  osId: string;
  family: OsFamily;
  /** true, wenn der Dateiname eindeutig war */
  confident: boolean;
}

/** Errät das Betriebssystem am Dateinamen der ISO. */
export function guessOs(fileName: string): OsGuess {
  const n = fileName.toLowerCase();
  if (/k?x?l?ubuntu/.test(n)) return { osId: "ubuntu", family: "linux", confident: true };
  if (/linuxmint|(^|[^a-z])mint([^a-z]|$)/.test(n)) return { osId: "mint", family: "linux", confident: true };
  if (/fedora/.test(n)) return { osId: "fedora", family: "linux", confident: true };
  if (/win(dows)?[ _.-]?11|win11|w11/.test(n)) return { osId: "windows11", family: "windows", confident: true };
  if (/(^|[^a-z])win|windows|w10|server[ _-]?20\d\d|en[-_]us_windows|de[-_]de_windows/.test(n))
    return { osId: "custom", family: "windows", confident: true };
  if (/debian|arch|manjaro|opensuse|suse|pop[-_]?os|elementary|zorin|kali|centos|rocky|alma|mx[-_]|endeavour|nixos|tails|rhel|linux|mageia|garuda|void|alpine|gentoo|kubuntu|lubuntu|deepin|tumbleweed/.test(n))
    return { osId: "custom", family: "linux", confident: true };
  return { osId: "custom", family: "linux", confident: false };
}

/** Sinnvoller Namensvorschlag, z. B. „Ubuntu 24.04“ aus ubuntu-24.04.3-desktop-amd64.iso */
export function suggestName(osId: string, fileName: string | null, taken: string[]): string {
  const f = (fileName ?? "").toLowerCase();
  let base: string;
  switch (osId) {
    case "ubuntu": {
      const m = f.match(/(\d{2}\.\d{2})/);
      base = m ? `Ubuntu ${m[1]}` : "Ubuntu";
      break;
    }
    case "mint": {
      const m = f.match(/(\d{2}(\.\d)?)/);
      base = m ? `Linux Mint ${m[1]}` : "Linux Mint";
      break;
    }
    case "fedora": {
      const m = f.match(/[-_](\d{2})[-_.]/);
      base = m ? `Fedora ${m[1]}` : "Fedora";
      break;
    }
    case "windows11":
      base = "Windows 11";
      break;
    default: {
      const raw = (fileName ?? "Meine VM").replace(/\.iso$/i, "");
      base = raw
        .replace(/[_]+/g, " ")
        .replace(/[-]+/g, " ")
        .replace(/\b(amd64|x86_64|x64|64bit|desktop|live|dvd|netinst|install(er)?|iso)\b/gi, "")
        .replace(/[<>:"/\\|?*']/g, "")
        .replace(/\s+/g, " ")
        .trim()
        .slice(0, 40);
      base = base ? base.charAt(0).toUpperCase() + base.slice(1) : "Meine VM";
    }
  }
  const lower = taken.map((t) => t.toLowerCase());
  if (!lower.includes(base.toLowerCase())) return base;
  for (let i = 2; i < 100; i++) {
    const c = `${base} (${i})`;
    if (!lower.includes(c.toLowerCase())) return c;
  }
  return base;
}
