import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowRight, Check, ChevronDown, Disc3, ExternalLink, Feather, Gauge, HardDrive, Loader2, MemoryStick, Rocket, Scale, Sparkles, Upload, X } from "lucide-react";
import { api, onFileDrop, openLink, pickIso } from "../../lib/api";
import { guessOs, OS_CATALOG, osById, OsLogo, suggestName } from "../../lib/os";
import { formatMemory, limitsFor, presetsFor, type PresetId } from "../../lib/presets";
import type { AppError, BackendKind, CreateProgress, HostInfo, IsoInfo, OsFamily, Settings, Vm } from "../../lib/types";
import { Button, cx, IconButton, Info, Segmented, Slider, TextInput, Tooltip } from "../../components/ui";
import { ErrorPanel } from "../../components/feedback";

const STEPS = ["System", "Name", "Leistung", "Fertig"];

export interface WizardInit {
  step?: number;
  osId?: string;
  iso?: string;
  advanced?: boolean;
}

export function Wizard({
  host,
  settings,
  vms,
  onClose,
  onCreated,
  init,
}: {
  host: HostInfo;
  settings: Settings;
  vms: Vm[];
  onClose: () => void;
  onCreated: (vm: Vm) => void;
  init?: WizardInit;
}) {
  const backend: BackendKind = host.activeBackend;
  const [step, setStep] = useState(init?.step ?? 0);
  const [dir, setDir] = useState<1 | -1>(1);

  // Schritt 1: Betriebssystem + ISO
  const [osId, setOsId] = useState<string | null>(init?.osId ?? null);
  const [family, setFamily] = useState<OsFamily>(init?.osId ? (osById(init.osId)?.family ?? "linux") : "linux");
  const [iso, setIso] = useState<IsoInfo | null>(null);
  const [guessUnsure, setGuessUnsure] = useState(false);
  const [isoError, setIsoError] = useState<AppError | null>(null);
  const [dragging, setDragging] = useState(false);

  // Schritt 2: Name
  const [name, setName] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [nameError, setNameError] = useState<string | null>(null);
  const [checkingName, setCheckingName] = useState(false);

  // Schritt 3: Leistung
  const [preset, setPreset] = useState<PresetId | "custom">("balanced");
  const [advanced, setAdvanced] = useState(init?.advanced ?? false);
  const presets = useMemo(() => presetsFor(family, host.logicalCores, host.totalMemoryMb), [family, host]);
  const limits = useMemo(() => limitsFor(family, host.logicalCores, host.totalMemoryMb), [family, host]);
  const [res, setRes] = useState(presets.balanced);

  // Schritt 4: Erstellen
  const [phase, setPhase] = useState<"form" | "creating" | "done" | "error">("form");
  const [progress, setProgress] = useState<CreateProgress[]>([]);
  const [createError, setCreateError] = useState<AppError | null>(null);
  const [created, setCreated] = useState<Vm | null>(null);

  const osEntry = osId ? osById(osId) : undefined;
  const win11Blocked = backend === "qemu";

  // Beispiel-ISO für Vorschaubilder (nur Mock-Modus)
  useEffect(() => {
    if (init?.iso) void acceptIso(init.iso);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Leistungsstufe folgt dem Betriebssystem
  useEffect(() => {
    if (preset !== "custom") setRes(presets[preset]);
  }, [presets, preset]);

  // Datei-Drop aus dem Explorer (nur in Schritt 1)
  useEffect(() => {
    if (step !== 0 || phase !== "form") return;
    let off: (() => void) | undefined;
    let alive = true;
    onFileDrop({
      over: setDragging,
      drop: (paths) => paths[0] && void acceptIso(paths[0]),
    }).then((fn) => (alive ? (off = fn) : fn()));
    return () => {
      alive = false;
      off?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, phase, osId]);

  async function acceptIso(path: string) {
    setIsoError(null);
    try {
      const info = await api.inspectIso(path);
      const g = guessOs(info.fileName);
      let nextOs = osId;
      if (g.osId !== "custom" && g.osId !== osId) nextOs = g.osId;
      else if (g.osId === "custom" && (!osId || g.confident)) nextOs = "custom";
      if (nextOs === "windows11" && win11Blocked) nextOs = "custom";
      const fam = nextOs && nextOs !== "custom" ? osById(nextOs)!.family : g.family;
      setOsId(nextOs ?? "custom");
      setFamily(fam);
      setGuessUnsure(nextOs === "custom" && !g.confident);
      setIso(info);
      if (!nameTouched) setName(suggestName(nextOs ?? "custom", info.fileName, vms.map((v) => v.name)));
    } catch (e) {
      setIsoError(e as AppError);
    }
  }

  function chooseTile(id: string) {
    setOsId(id);
    const f = osById(id)!.family;
    setFamily(f);
    setGuessUnsure(false);
    if (!nameTouched) setName(suggestName(id, iso?.fileName ?? null, vms.map((v) => v.name)));
    // Passt die gewählte ISO nicht zur Kachel, wieder entfernen.
    if (iso) {
      const g = guessOs(iso.fileName);
      if (g.confident && g.osId !== "custom" && g.osId !== id) setIso(null);
    }
  }

  // Namen live prüfen (kurz verzögert)
  useEffect(() => {
    if (step !== 1) return;
    setCheckingName(true);
    const t = setTimeout(async () => {
      try {
        setNameError(await api.checkName(name, backend));
      } catch {
        setNameError(null);
      } finally {
        setCheckingName(false);
      }
    }, 280);
    return () => clearTimeout(t);
  }, [name, step, backend]);

  const canNext = [!!iso && !!osId, !nameError && !checkingName && name.trim().length > 0, true, true][step];

  function go(to: number) {
    setDir(to > step ? 1 : -1);
    setStep(to);
  }

  async function create() {
    setPhase("creating");
    setCreateError(null);
    const expected: [string, string][] = [
      ["folder", "Ordner vorbereiten"],
      ["disk", "Virtuelle Festplatte anlegen"],
      ["vm", "Virtuelle Maschine anlegen"],
      ...(backend === "hyperv" ? ([["security", family === "windows" ? "Secure Boot & TPM einrichten" : "Secure Boot einrichten"]] as [string, string][]) : []),
      ["iso", "Installationsmedium einlegen"],
      ["start", "VM starten"],
    ];
    setProgress(expected.map(([s, l]) => ({ step: s, label: l, state: "pending" })));
    try {
      const vm = await api.createVm(
        { name: name.trim(), osFamily: family, osId: osId ?? "custom", isoPath: iso?.path ?? null, cpus: res.cpus, memoryMb: res.memoryMb, diskGb: res.diskGb },
        backend,
        (p) => setProgress((list) => list.map((x) => (x.step === p.step ? { ...x, state: p.state } : x))),
      );
      setCreated(vm);
      setPhase("done");
      onCreated(vm);
    } catch (e) {
      setProgress((list) => list.map((x) => (x.state === "active" ? { ...x, state: "error" } : x)));
      setCreateError(e as AppError);
      setPhase("error");
    }
  }

  // Enter = weiter
  const nextRef = useRef<() => void>(() => {});
  nextRef.current = () => {
    if (phase !== "form" || !canNext) return;
    if (step < 3) go(step + 1);
    else void create();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) nextRef.current();
      if (e.key === "Escape" && phase === "form") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase, onClose]);

  const specLine = `${res.cpus} ${res.cpus === 1 ? "Kern" : "Kerne"} · ${formatMemory(res.memoryMb)} Arbeitsspeicher · ${res.diskGb} GB Festplatte`;

  return (
    <div className="flex h-full flex-col bg-bg anim-fade">
      {/* Kopfzeile mit Schrittanzeige */}
      <header className="flex h-16 shrink-0 items-center justify-between border-b border-line px-5">
        <div className="w-40">
          {phase === "form" && (
            <IconButton label="Abbrechen" tooltipSide="bottom" onClick={onClose}>
              <X className="size-[18px]" />
            </IconButton>
          )}
        </div>
        <ol className="flex items-center gap-2">
          {STEPS.map((s, i) => {
            const done = i < step || phase === "done";
            const active = i === step && phase !== "done";
            return (
              <li key={s} className="flex items-center gap-2">
                {i > 0 && <span className={cx("h-px w-8 transition-colors", i <= step ? "bg-accent/50" : "bg-line-strong")} />}
                <span
                  className={cx(
                    "flex items-center gap-2 rounded-full py-1 pl-1 pr-3 text-[13px] font-medium transition-colors",
                    active ? "bg-accent-soft text-accent-text" : done ? "text-text-2" : "text-muted",
                  )}
                >
                  <span
                    className={cx(
                      "flex size-6 items-center justify-center rounded-full text-[12px] transition-colors",
                      active ? "bg-accent text-white" : done ? "bg-ok-soft text-ok" : "bg-bg-subtle text-muted",
                    )}
                  >
                    {done ? <Check className="size-3.5" strokeWidth={3} /> : i + 1}
                  </span>
                  {s}
                </span>
              </li>
            );
          })}
        </ol>
        <div className="w-40" />
      </header>

      {/* Inhalt */}
      <main className="min-h-0 flex-1 overflow-y-auto">
        <div key={`${step}-${phase === "form" ? "f" : "x"}`} className={cx("mx-auto w-full max-w-[760px] px-8 py-10", dir === 1 ? "anim-slide" : "anim-slide-back")}>
          {step === 0 && (
            <>
              <StepTitle title="Welches Betriebssystem möchtest du?" sub="Wähle eines aus oder zieh einfach eine ISO-Datei ins Fenster." />
              <div className="grid grid-cols-4 gap-3">
                {OS_CATALOG.map((o) => {
                  const blocked = o.id === "windows11" && win11Blocked;
                  const tile = (
                    <button
                      key={o.id}
                      disabled={blocked}
                      onClick={() => chooseTile(o.id)}
                      className={cx(
                        "group relative flex w-full flex-col items-start rounded-2xl border bg-surface p-4 text-left transition-all duration-150",
                        osId === o.id ? "border-accent shadow-[0_0_0_3px_var(--accent-soft)]" : "border-line hover:-translate-y-0.5 hover:border-line-strong hover:shadow-card",
                        blocked && "opacity-45",
                      )}
                    >
                      <OsLogo osId={o.id} family={o.family} size={44} />
                      <div className="mt-3 font-semibold">{o.name}</div>
                      <div className="mt-0.5 text-[12.5px] leading-snug text-muted">{blocked ? "Mit QEMU nicht verfügbar" : o.tagline}</div>
                      {!blocked && (
                        <span
                          role="link"
                          tabIndex={0}
                          onClick={(e) => {
                            e.stopPropagation();
                            void openLink(o.downloadUrl);
                          }}
                          className="mt-3 inline-flex items-center gap-1 text-[12px] font-medium text-accent-text opacity-80 hover:underline hover:opacity-100"
                        >
                          Herunterladen <ExternalLink className="size-3" />
                        </span>
                      )}
                      {osId === o.id && (
                        <span className="absolute right-3 top-3 flex size-5 items-center justify-center rounded-full bg-accent text-white anim-pop">
                          <Check className="size-3" strokeWidth={3} />
                        </span>
                      )}
                    </button>
                  );
                  return blocked ? (
                    <Tooltip key={o.id} text="Windows 11 verlangt einen TPM-Sicherheitschip. QEMU kann ihn unter Windows nicht bereitstellen – nutze dafür Hyper-V (Windows Pro) oder installiere Linux." wide>
                      <div className="w-full">{tile}</div>
                    </Tooltip>
                  ) : (
                    tile
                  );
                })}
              </div>

              {/* ISO-Bereich */}
              <div className="mt-6">
                {iso ? (
                  <div className="flex items-center gap-4 rounded-2xl border border-line bg-surface p-4 anim-rise">
                    <span className="flex size-11 items-center justify-center rounded-xl bg-accent-soft">
                      <Disc3 className="size-5 text-accent-text" />
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-medium selectable">{iso.fileName}</div>
                      <div className="text-[12.5px] text-muted">
                        {(iso.sizeMb / 1024).toFixed(1).replace(".", ",")} GB · Installationsmedium
                        <Info text="Eine ISO-Datei ist das Abbild einer Installations-DVD. Die VM startet davon, damit du das Betriebssystem installieren kannst." />
                      </div>
                    </div>
                    {osId === "custom" && (
                      <div className="flex flex-col items-end gap-1">
                        <span className="text-[11.5px] text-muted">{guessUnsure ? "Bitte prüfen:" : "Erkannt:"}</span>
                        <Segmented<OsFamily>
                          value={family}
                          onChange={setFamily}
                          options={[
                            { value: "linux", label: "Linux" },
                            { value: "windows", label: "Windows" },
                          ]}
                        />
                      </div>
                    )}
                    <Button size="sm" variant="ghost" onClick={async () => { const p = await pickIso(); if (p) await acceptIso(p); }}>
                      Ändern
                    </Button>
                  </div>
                ) : (
                  <div
                    className={cx(
                      "flex flex-col items-center rounded-2xl border-2 border-dashed px-6 py-8 text-center transition-all",
                      dragging ? "scale-[1.01] border-accent bg-accent-soft" : "border-line-strong bg-surface-2",
                    )}
                  >
                    <span className={cx("flex size-12 items-center justify-center rounded-2xl transition-colors", dragging ? "bg-accent text-white" : "bg-bg-subtle text-text-2")}>
                      <Upload className="size-5" />
                    </span>
                    <div className="mt-3 font-semibold">
                      {dragging ? "Loslassen zum Übernehmen" : osEntry && osEntry.id !== "custom" ? `${osEntry.name}-ISO hierher ziehen` : "ISO-Datei hierher ziehen"}
                    </div>
                    <div className="mt-1 text-[13px] text-muted">
                      oder{" "}
                      <button className="font-medium text-accent-text hover:underline" onClick={async () => { const p = await pickIso(); if (p) await acceptIso(p); }}>
                        Datei auswählen
                      </button>
                    </div>
                    {osEntry && osEntry.id !== "custom" && (
                      <button
                        onClick={() => openLink(osEntry.downloadUrl)}
                        className="mt-4 inline-flex items-center gap-1.5 rounded-full bg-surface px-3.5 py-1.5 text-[13px] font-medium text-text-2 shadow-card hover:text-text"
                      >
                        Noch keine ISO? Offiziell bei {osEntry.name === "Windows 11" ? "Microsoft" : osEntry.name} herunterladen
                        <ExternalLink className="size-3.5" />
                      </button>
                    )}
                  </div>
                )}
                {isoError && (
                  <div className="mt-3">
                    <ErrorPanel error={isoError} onClose={() => setIsoError(null)} />
                  </div>
                )}
              </div>
            </>
          )}

          {step === 1 && (
            <>
              <StepTitle title="Wie soll die VM heißen?" sub="Unter diesem Namen findest du sie später in der Übersicht." />
              <div className="flex items-start gap-4 rounded-2xl border border-line bg-surface p-5">
                <OsLogo osId={osId ?? "custom"} family={family} size={48} />
                <div className="flex-1">
                  <TextInput
                    value={name}
                    autoFocus
                    error={nameError}
                    onChange={(v) => {
                      setName(v);
                      setNameTouched(true);
                    }}
                    onEnter={() => nextRef.current()}
                  />
                  <div className="mt-2 flex items-center gap-1.5 text-[12.5px] text-muted">
                    {checkingName ? <Loader2 className="size-3 anim-spin" /> : !nameError && name.trim() ? <Check className="size-3.5 text-ok" /> : null}
                    <span className="truncate selectable">Speicherort: {settings.vmDir}\{name.trim() || "…"}</span>
                  </div>
                </div>
              </div>
            </>
          )}

          {step === 2 && (
            <>
              <StepTitle
                title="Wie viel Leistung soll die VM bekommen?"
                sub={`Passend zu deinem PC berechnet. Nestbox nutzt höchstens die Hälfte (${limits.maxCpus} Kerne, ${formatMemory(limits.maxMemoryMb)}), damit Windows flüssig bleibt.`}
              />
              {limits.hostTooSmall && (
                <div className="mb-4 rounded-xl border border-warn/30 bg-warn-soft p-3.5 text-[13.5px]">
                  <span className="font-semibold">Knapp bemessen: </span>
                  {family === "windows" ? "Windows braucht mindestens 2 Kerne und 4 GB Arbeitsspeicher." : "Das Betriebssystem braucht mehr Leistung."} Die Hälfte deines PCs reicht dafür eigentlich nicht – die VM kann langsam sein.
                </div>
              )}
              <div className="grid grid-cols-3 gap-3">
                {(
                  [
                    ["light", "Leicht", "Für einfache Aufgaben", Feather],
                    ["balanced", "Ausgewogen", "Für den Alltag", Scale],
                    ["strong", "Leistungsstark", "Für anspruchsvolle Programme", Rocket],
                  ] as const
                ).map(([id, title, sub, Icon]) => {
                  const p = presets[id];
                  const active = preset === id;
                  return (
                    <button
                      key={id}
                      onClick={() => {
                        setPreset(id);
                        setRes(p);
                      }}
                      className={cx(
                        "relative flex flex-col items-start rounded-2xl border bg-surface p-5 text-left transition-all",
                        active ? "border-accent shadow-[0_0_0_3px_var(--accent-soft)]" : "border-line hover:-translate-y-0.5 hover:border-line-strong hover:shadow-card",
                      )}
                    >
                      {id === "balanced" && (
                        <span className="absolute right-3 top-3 rounded-full bg-accent-soft px-2 py-0.5 text-[11px] font-semibold text-accent-text">Empfohlen</span>
                      )}
                      <span className={cx("flex size-10 items-center justify-center rounded-xl", active ? "bg-accent text-white" : "bg-bg-subtle text-text-2")}>
                        <Icon className="size-5" />
                      </span>
                      <div className="mt-3 font-semibold">{title}</div>
                      <div className="text-[12.5px] text-muted">{sub}</div>
                      <div className="mt-4 w-full space-y-1.5 border-t border-line pt-3 text-[13px] text-text-2">
                        <Spec icon={<Gauge className="size-3.5" />} text={`${p.cpus} ${p.cpus === 1 ? "Kern" : "Kerne"}`} />
                        <Spec icon={<MemoryStick className="size-3.5" />} text={`${formatMemory(p.memoryMb)} RAM`} />
                        <Spec icon={<HardDrive className="size-3.5" />} text={`${p.diskGb} GB Festplatte`} />
                      </div>
                    </button>
                  );
                })}
              </div>

              <button onClick={() => setAdvanced(!advanced)} className="mt-5 inline-flex items-center gap-1.5 text-[13.5px] font-medium text-text-2 hover:text-text">
                <ChevronDown className={cx("size-4 transition-transform", advanced && "rotate-180")} />
                Erweitert
              </button>
              {advanced && (
                <div className="mt-3 grid gap-6 rounded-2xl border border-line bg-surface p-5 anim-rise">
                  <Slider
                    label="Prozessorkerne"
                    info="Wie viele Rechenkerne deines Prozessors die VM gleichzeitig nutzen darf. Mehr Kerne = schneller bei aufwendigen Aufgaben."
                    value={res.cpus}
                    min={limits.minCpus}
                    max={limits.maxCpus}
                    format={(v) => `${v}`}
                    onChange={(v) => {
                      setPreset("custom");
                      setRes({ ...res, cpus: v });
                    }}
                  />
                  <Slider
                    label="Arbeitsspeicher (RAM)"
                    info="Solange die VM läuft, ist dieser Arbeitsspeicher für sie reserviert und fehlt deinem PC."
                    value={res.memoryMb}
                    min={limits.minMemoryMb}
                    max={limits.maxMemoryMb}
                    step={512}
                    format={formatMemory}
                    onChange={(v) => {
                      setPreset("custom");
                      setRes({ ...res, memoryMb: v });
                    }}
                  />
                  <Slider
                    label="Festplatte"
                    info="Die Maximalgröße der virtuellen Festplatte. Sie belegt auf deinem PC nur so viel Platz, wie tatsächlich genutzt wird."
                    value={res.diskGb}
                    min={limits.minDiskGb}
                    max={Math.max(limits.minDiskGb, Math.min(limits.maxDiskGb, Math.floor(host.freeDiskGb) || limits.maxDiskGb, 512))}
                    step={8}
                    format={(v) => `${v} GB`}
                    onChange={(v) => {
                      setPreset("custom");
                      setRes({ ...res, diskGb: v });
                    }}
                  />
                </div>
              )}
            </>
          )}

          {step === 3 && phase === "form" && (
            <>
              <StepTitle title="Alles bereit" sub="Prüfe kurz die Übersicht – dann legt Nestbox die VM an und startet sie." />
              <Summary name={name} osId={osId ?? "custom"} family={family} iso={iso} specLine={specLine} settings={settings} backend={backend} />
            </>
          )}

          {step === 3 && phase !== "form" && (
            <div className="mx-auto max-w-[480px]">
              {phase === "done" ? (
                <div className="flex flex-col items-center text-center anim-rise">
                  <span className="flex size-16 items-center justify-center rounded-full bg-ok-soft anim-pop">
                    <Check className="size-8 text-ok" strokeWidth={2.6} />
                  </span>
                  <h1 className="mt-5 font-display text-[26px] font-semibold tracking-[-0.02em]">„{created?.name}“ läuft</h1>
                  <p className="mt-2 text-[15px] text-text-2">
                    {backend === "hyperv" ? "Das Fenster der VM öffnet sich gleich." : "Das QEMU-Fenster der VM ist geöffnet."} Folge dort der Installation des Betriebssystems.
                  </p>
                  {family === "windows" && (
                    <div className="mt-5 flex gap-3 rounded-xl border border-line bg-surface p-4 text-left text-[13.5px]">
                      <Sparkles className="mt-0.5 size-4 shrink-0 text-accent" />
                      <span>
                        <b>Tipp:</b> Erscheint „Press any key to boot from CD or DVD“, klicke ins VM-Fenster und drücke sofort eine beliebige Taste.
                      </span>
                    </div>
                  )}
                  <div className="mt-5 flex gap-3 rounded-xl border border-line bg-surface p-4 text-left text-[13.5px]">
                    <Disc3 className="mt-0.5 size-4 shrink-0 text-accent" />
                    <span>Nach der Installation kannst du das Installationsmedium über das Menü der VM („…“ → „Installationsmedium auswerfen“) entfernen.</span>
                  </div>
                </div>
              ) : (
                <>
                  <StepTitle title={phase === "error" ? "Das hat nicht geklappt" : `„${name.trim()}“ wird erstellt …`} sub={phase === "error" ? "Bereits angelegte Teile wurden wieder entfernt." : "Das dauert nur einen Moment."} />
                  <div className="overflow-hidden rounded-2xl border border-line bg-surface">
                    {progress.map((p, i) => (
                      <div key={p.step} className={cx("flex items-center gap-3 px-5 py-3.5", i > 0 && "border-t border-line")}>
                        <span className="flex size-6 items-center justify-center">
                          {p.state === "done" ? (
                            <span className="flex size-6 items-center justify-center rounded-full bg-ok-soft anim-pop">
                              <Check className="size-3.5 text-ok" strokeWidth={3} />
                            </span>
                          ) : p.state === "active" ? (
                            <Loader2 className="size-5 anim-spin text-accent" />
                          ) : p.state === "error" ? (
                            <span className="flex size-6 items-center justify-center rounded-full bg-err-soft">
                              <X className="size-3.5 text-err" strokeWidth={3} />
                            </span>
                          ) : (
                            <span className="size-2 rounded-full bg-line-strong" />
                          )}
                        </span>
                        <span className={cx("text-[14px]", p.state === "done" ? "text-text" : p.state === "active" ? "font-medium text-text" : "text-muted")}>{p.label}</span>
                      </div>
                    ))}
                  </div>
                  {createError && (
                    <div className="mt-4">
                      <ErrorPanel error={createError} />
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>
      </main>

      {/* Fußzeile */}
      <footer className="flex h-[72px] shrink-0 items-center justify-between border-t border-line px-6">
        <div className="text-[13px] text-muted">{step === 2 && phase === "form" && <span className="tabular-nums">{specLine}</span>}</div>
        <div className="flex gap-2">
          {phase === "form" && step > 0 && (
            <Button variant="ghost" icon={<ArrowLeft className="size-4" />} onClick={() => go(step - 1)}>
              Zurück
            </Button>
          )}
          {phase === "form" && step < 3 && (
            <Button variant="primary" disabled={!canNext} onClick={() => go(step + 1)}>
              Weiter <ArrowRight className="size-4" />
            </Button>
          )}
          {phase === "form" && step === 3 && (
            <Button variant="primary" size="lg" icon={<Rocket className="size-4" />} onClick={create}>
              Erstellen &amp; starten
            </Button>
          )}
          {phase === "error" && (
            <>
              <Button variant="ghost" icon={<ArrowLeft className="size-4" />} onClick={() => setPhase("form")}>
                Zurück
              </Button>
              <Button variant="primary" onClick={create}>
                Erneut versuchen
              </Button>
            </>
          )}
          {phase === "done" && (
            <Button variant="primary" size="lg" onClick={onClose}>
              Zur Übersicht <ArrowRight className="size-4" />
            </Button>
          )}
        </div>
      </footer>
    </div>
  );
}

function StepTitle({ title, sub }: { title: string; sub: string }) {
  return (
    <div className="mb-7">
      <h1 className="font-display text-[26px] font-semibold tracking-[-0.02em]">{title}</h1>
      <p className="mt-1.5 text-[15px] text-text-2">{sub}</p>
    </div>
  );
}

function Spec({ icon, text }: { icon: React.ReactNode; text: string }) {
  return (
    <div className="flex items-center gap-2">
      <span className="text-muted">{icon}</span>
      {text}
    </div>
  );
}

function Summary({
  name,
  osId,
  family,
  iso,
  specLine,
  settings,
  backend,
}: {
  name: string;
  osId: string;
  family: OsFamily;
  iso: IsoInfo | null;
  specLine: string;
  settings: Settings;
  backend: BackendKind;
}) {
  const os = osById(osId);
  const rows: [string, React.ReactNode][] = [
    ["Betriebssystem", os ? os.name : family === "windows" ? "Windows (eigene ISO)" : "Linux (eigene ISO)"],
    ["Installationsmedium", <span className="selectable break-all">{iso?.fileName}</span>],
    ["Leistung", specLine],
    ["Speicherort", <span className="selectable break-all">{settings.vmDir}\{name.trim()}</span>],
    [
      "Technik",
      backend === "hyperv" ? (
        <span className="inline-flex items-center">
          Hyper-V, Generation 2 · Secure Boot{family === "windows" ? " · TPM" : ""} · Netzwerk über „Default Switch“
          <Info text="Generation 2 = moderne UEFI-VM. Secure Boot und TPM sind Sicherheitsfunktionen, die z. B. Windows 11 voraussetzt. Der „Default Switch“ gibt der VM automatisch Internetzugang." />
        </span>
      ) : (
        <span className="inline-flex items-center">
          QEMU mit WHPX-Beschleunigung · UEFI · Netzwerk (NAT)
          <Info text="WHPX ist die Schnittstelle, über die QEMU die Virtualisierung von Windows nutzt. NAT gibt der VM Internetzugang über deinen PC." />
        </span>
      ),
    ],
  ];
  return (
    <div className="overflow-hidden rounded-2xl border border-line bg-surface">
      <div className="flex items-center gap-4 border-b border-line bg-surface-2 px-5 py-5">
        <OsLogo osId={osId} family={family} size={52} />
        <div>
          <div className="font-display text-[20px] font-semibold tracking-[-0.01em]">{name.trim()}</div>
          <div className="text-[13px] text-muted">Startet direkt nach dem Erstellen vom Installationsmedium</div>
        </div>
      </div>
      <dl className="divide-y divide-line">
        {rows.map(([k, v]) => (
          <div key={k} className="flex gap-4 px-5 py-3 text-[13.5px]">
            <dt className="w-40 shrink-0 text-muted">{k}</dt>
            <dd className="min-w-0 flex-1 text-text">{v}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
