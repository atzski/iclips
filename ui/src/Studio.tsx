/**
 * iClips Studio : la galerie des clips du profil iBox (vignettes, filtre par jeu et favoris),
 * un lecteur, le titre et le favori de chaque clip, la suppression, et le montage (0.3) :
 * couper un passage, en faire un GIF, assembler plusieurs clips. Clips automatiques (0.4) :
 * réglages dans `Auto.tsx`, marque sur les cartes. Le partage vient ensuite (docs/CONCEPTION.md).
 */
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";
import { clipUrl, host, onHost, standalone, thumbUrl, type Clip } from "./host";
import { makeThumb } from "./thumbs";
import { joinSegments, useRender, type Span } from "./edit";
import { Trim } from "./Trim";
import { AutoSheet } from "./Auto";
import { dragOut, ShareSheet } from "./Share";
import { Vertical } from "./Vertical";
import { SettingsSheet } from "./Settings";
import { SoundMix } from "./Sound";
import { ClipMenu } from "./Menu";
import { useQuietVoice } from "./quiet";
import { hint, KeysSheet, useKeys } from "./keys";
import { duration, size, when } from "./format";
import { t } from "./i18n";
import { openMenu, useNav } from "./nav";
import { Button, Icon, type Tone } from "./ui";
import logo from "./logo.png";

type Filter = "all" | "videos" | "images" | "favorites";
const FILTERS: Filter[] = ["all", "videos", "images", "favorites"];
const filterLabel = (f: Filter) => (f === "all" ? t("Tous") : f === "videos" ? t("Clips") : f === "images" ? t("Captures") : t("Favoris"));
/** Teinte de chaque filtre (code couleur) : clips en violet, captures en rose, favoris en jaune. */
const FILTER_TONE: Record<Filter, Tone> = { all: "accent", videos: "accent", images: "pink", favorites: "yellow" };
/** Teinte stable par jeu, pour reconnaître ses puces d'un coup d'œil. */
const GAME_TONES: Tone[] = ["blue", "green", "orange", "pink", "teal", "purple", "yellow"];
export function gameTone(name: string): Tone {
  // FNV-1a : des noms proches tombent sur des teintes différentes
  let h = 2166136261;
  for (const ch of name) h = Math.imul(h ^ ch.charCodeAt(0), 16777619) >>> 0;
  return GAME_TONES[h % GAME_TONES.length];
}
const isImage = (c: Clip) => c.kind === "gif" || c.kind === "image";

export function Studio() {
  const [clips, setClips] = useState<Clip[] | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  /** Jeu choisi (`""` : hors jeu), `null` : tous. */
  const [game, setGame] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Assemblage : clips choisis, dans l'ordre (null : pas en cours). */
  const [picked, setPicked] = useState<string[] | null>(null);
  /** Sélection (0.7.1) : clips cochés pour les supprimer d'un coup (null : pas en cours). */
  const [selected, setSelected] = useState<string[] | null>(null);
  /** Dernier clip coché : Maj + clic coche la plage jusqu'à lui. */
  const anchor = useRef<string | null>(null);
  /** Suppression de la sélection : confirmation, puis en cours. */
  const [askDelete, setAskDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [auto, setAuto] = useState(false);
  const [settings, setSettings] = useState<false | "all" | "sound">(false);
  /** Menu d'un clip (clic droit, touche Menu, Y) : où l'ouvrir, et la carte qui rend le focus. */
  const [menu, setMenu] = useState<{ name: string; x: number; y: number; from: HTMLElement } | null>(null);
  const closeMenu = useCallback(() => setMenu((m) => { m?.from.focus(); return null; }), []);
  /** Outil ouvert d'office dans le lecteur (« Modifier le clip » : le montage). */
  const [startMode, setStartMode] = useState<"cut" | null>(null);
  const [help, setHelp] = useState(false);
  /** Mémoire des clips coupée : rien ne peut être gardé en partie (bandeau). */
  const [memoryOff, setMemoryOff] = useState(false);
  const checkMemory = useCallback(() => {
    host<{ enabled: boolean }>("capture.get").then((c) => setMemoryOff(!c.enabled)).catch(() => undefined);
  }, []);
  useEffect(() => { if (!settings) checkMemory(); }, [settings, checkMemory]);
  /** Fiche Partager du lecteur (ici : Échap la ferme seule, pas le lecteur). */
  const [sharing, setSharing] = useState(false);

  const load = useCallback(() => {
    host<Clip[]>("clips.list").then(setClips).catch((e: Error) => { setClips([]); setError(t("Action impossible : {0}", e.message)); });
  }, []);
  useEffect(() => { load(); return onHost((event) => { if (event === "clips://changed") load(); }); }, [load]);
  useEffect(() => { if (!error) return; const id = setTimeout(() => setError(null), 4000); return () => clearTimeout(id); }, [error]);
  useEffect(() => { if (!notice) return; const id = setTimeout(() => setNotice(null), 3000); return () => clearTimeout(id); }, [notice]);

  const games = useMemo(() => [...new Set((clips ?? []).map((c) => c.game ?? ""))].sort((a, b) => a.localeCompare(b)), [clips]);
  const shown = useMemo(() => (clips ?? []).filter((c) =>
    (filter === "all" || (filter === "favorites" && c.favorite) || (filter === "videos" && c.kind !== "image") || (filter === "images" && c.kind === "image"))
    && (game === null || (c.game ?? "") === game)), [clips, filter, game]);

  // vignettes : une à la fois, pour les clips qui n'en ont pas encore
  const thumbing = useRef(false);
  useEffect(() => {
    const todo = (clips ?? []).filter((c) => c.src && !c.thumb && !isImage(c));
    if (!todo.length || thumbing.current) return;
    thumbing.current = true;
    void (async () => {
      for (const c of todo) {
        const made = await makeThumb(clipUrl(c)).catch(() => null);
        if (made) await host("clips.thumb", { name: c.name, ...made }).catch(() => undefined);
      }
      thumbing.current = false;
      load();
    })();
  }, [clips, load]);
  const current = clips?.find((c) => c.name === open) ?? null;

  const { job, render } = useRender(
    (name) => { load(); setPicked(null); setOpen(name); },
    (message) => setError(t("Montage impossible : {0}", message)),
  );
  const startRender = (arg: Record<string, unknown>, label: string) =>
    void render(arg, label).catch((e: Error) => setError(t("Montage impossible : {0}", e.message)));
  const trim = (c: Clip, kind: "mp4" | "gif" | "vertical", span: Span, focus = 0.5) =>
    kind === "mp4"
      ? startRender({ kind, segments: [{ name: c.name, ...span }], title: c.title ? t("{0} (coupé)", c.title) : undefined }, t("Coupe du clip…"))
      : kind === "vertical"
        ? startRender({ kind, name: c.name, ...span, focus }, t("Format vertical…"))
        : startRender({ kind, name: c.name, ...span }, t("Création du GIF…"));
  const join = () => {
    const list = (picked ?? []).map((n) => clips?.find((c) => c.name === n)).filter((c): c is Clip => !!c);
    startRender({ kind: "mp4", segments: joinSegments(list) }, t("Assemblage de {0} clips…", list.length));
  };

  const act = async (call: string, arg: Record<string, unknown>) => {
    try { await host(call, arg); load(); } catch (e) { setError(t("Action impossible : {0}", (e as Error).message)); }
  };

  // sélection : entrer (avec un premier clip), cocher, plage (Maj), tout, supprimer
  const startSelect = (first?: string) => { setPicked(null); setAskDelete(false); setSelected(first ? [first] : []); anchor.current = first ?? null; };
  const endSelect = () => { setSelected(null); setAskDelete(false); anchor.current = null; };
  const toggle = (name: string, range = false) => {
    setAskDelete(false);
    // l'ancre est lue maintenant : React appelle la mise à jour plus tard, après son changement
    const from = range ? anchor.current : null;
    setSelected((cur) => {
      const list = cur ?? [];
      if (from) {
        const names = shown.map((c) => c.name);
        const [a, b] = [names.indexOf(from), names.indexOf(name)].sort((x, y) => x - y);
        if (a >= 0) return [...new Set([...list, ...names.slice(a, b + 1)])];
      }
      return list.includes(name) ? list.filter((n) => n !== name) : [...list, name];
    });
    anchor.current = name;
  };
  const allShown = shown.length > 0 && !!selected && shown.every((c) => selected.includes(c.name));
  const selectAll = () => { setAskDelete(false); setSelected(allShown ? [] : shown.map((c) => c.name)); };
  const deleteSelected = async () => {
    const list = selected ?? [];
    if (!list.length) return;
    setDeleting(true);
    let failed = 0;
    for (const name of list) await host("clips.delete", { name }).catch(() => { failed++; });
    setDeleting(false);
    endSelect();
    load();
    const done = list.length - failed;
    if (failed) setError(t("{0} clips non supprimés", failed));
    else setNotice(done === 1 ? t("1 clip supprimé") : t("{0} clips supprimés", done));
  };

  // galerie : 1 à 4 (filtres), Ctrl+A (assembler), A (automatique), R (réglages) ; ? partout
  useKeys(!open && !auto && !settings && !help && !menu, (k) => {
    const n = Number(k);
    if (n >= 1 && n <= FILTERS.length) { setFilter(FILTERS[n - 1]); return true; }
    // en sélection : Ctrl+A coche tout (ou rien), Suppr demande la suppression
    if (selected && k === "ctrl+a") { selectAll(); return true; }
    if (selected && k === "Delete") { if (selected.length) setAskDelete(true); return true; }
    if (k === "s") { if (selected) endSelect(); else startSelect(); return true; }
    if (k === "ctrl+a") { setSelected(null); setPicked((p) => (p ? null : [])); return true; }
    if (k === "a") { setAuto(true); return true; }
    if (k === "r") { setSettings("all"); return true; }
    return false;
  });
  useKeys(!help, (k) => (k === "?" ? (setHelp(true), true) : false));

  useNav(() => {
    if (menu) closeMenu();
    else if (help) setHelp(false);
    else if (settings) setSettings(false);
    else if (auto) setAuto(false);
    else if (sharing) setSharing(false);
    else if (confirm) setConfirm(false);
    else if (open) setOpen(null);
    else if (askDelete) setAskDelete(false);
    else if (selected) endSelect();
    else if (picked) setPicked(null);
    else void host("screen.close");
  });

  // la manette arrive sur la première carte
  useEffect(() => {
    if (clips && !open && !document.activeElement?.hasAttribute("data-nav")) document.querySelector<HTMLElement>("[data-card]")?.focus();
  }, [clips, open]);

  return (
    <div className="flex h-full flex-col">
      <div className="ibox-rainbow h-1 w-full opacity-80" aria-hidden="true" />
      <header className="flex flex-wrap items-center gap-x-4 gap-y-3 px-8 pt-5 pb-4">
        <motion.img src={logo} alt="" className="h-12 w-12 drop-shadow-[0_6px_14px_var(--accent-glow)]" initial={{ rotate: -12, scale: 0.8 }} animate={{ rotate: 0, scale: 1 }} transition={SPRING.pop} />
        <div className="min-w-[11rem] flex-1">
          <h1 className="font-display text-[1.6rem] leading-tight text-ink">{t("iClips Studio")}</h1>
          {clips && <p className="text-[0.85rem] font-semibold text-ink-soft">{clips.length === 1 ? t("1 clip") : t("{0} clips", clips.length)}</p>}
        </div>
        <div role="tablist" className="flex rounded-full bg-white p-1 shadow-sm">
          {FILTERS.map((f, i) => (
            <button
              key={f}
              role="tab"
              data-nav
              title={hint(filterLabel(f), String(i + 1))}
              aria-selected={filter === f}
              onClick={() => setFilter(f)}
              className={`tone-${FILTER_TONE[f]} relative h-9 rounded-full px-4 text-[0.9rem] font-bold transition-colors ${filter === f ? "text-[var(--tone-on)]" : "text-ink-soft hover:text-[var(--tone-ink)]"}`}
            >
              {/* la pastille glisse d'un onglet à l'autre */}
              {filter === f && <motion.span layoutId="filter-pill" className="absolute inset-0 rounded-full bg-[var(--tone)] shadow-[0_6px_16px_-8px_var(--tone)]" transition={SPRING.snappy} />}
              <span className="relative">{filterLabel(f)}</span>
            </button>
          ))}
        </div>
        <Button tone="blue" icon="check" title={hint(t("Sélectionner"), "S")} variant={selected ? "primary" : "soft"} aria-pressed={!!selected} onClick={() => (selected ? endSelect() : startSelect())}>{t("Sélectionner")}</Button>
        <Button tone="orange" icon="copy" title={hint(t("Assembler"), "Ctrl+A")} variant={picked ? "primary" : "soft"} aria-pressed={!!picked} onClick={() => { setSelected(null); setPicked((p) => (p ? null : [])); }}>{t("Assembler")}</Button>
        <Button tone="teal" icon="auto" title={hint(t("Automatique"), "A")} onClick={() => setAuto(true)}>{t("Automatique")}</Button>
        <Button tone="orange" icon="gear" title={hint(t("Réglages"), "R")} onClick={() => setSettings("all")}>{t("Réglages")}</Button>
        <Button tone="blue" variant="ghost" title={hint(t("Raccourcis"), "?")} aria-label={t("Raccourcis")} onClick={() => setHelp(true)} className="px-4 font-extrabold">?</Button>
        <Button tone="red" variant="ghost" icon="x" onClick={() => void host("screen.close")}>{t("Fermer")}</Button>
      </header>

      <AnimatePresence initial={false}>
        {memoryOff && (
          <motion.div role="alert" className="tone-red mx-8 mb-3 flex items-center gap-3 overflow-hidden rounded-xl bg-[color-mix(in_srgb,var(--tone)_14%,var(--ibox-white))] px-4 py-2.5 shadow-[inset_0_0_0_1.5px_color-mix(in_srgb,var(--tone)_35%,transparent)]"
            initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0, transition: { duration: 0.16 } }} transition={SPRING.smooth}>
            <Icon name="x" />
            <span className="min-w-0 flex-1 text-[0.9rem] font-bold text-ink">{t("La mémoire des clips est coupée : aucun clip ne peut être gardé pendant vos parties.")}</span>
            <Button tone="green" variant="primary" icon="check" onClick={() => void host("capture.set", { enabled: true }).then(checkMemory)}>{t("Rallumer")}</Button>
          </motion.div>
        )}
      </AnimatePresence>

      {games.length > 1 && (
        <nav aria-label={t("Tous les jeux")} className="-mt-2 flex gap-2 overflow-x-auto px-8 pt-2 pb-3">
          {[null, ...games].map((g) => (
            <button
              key={g ?? "*"}
              data-nav
              aria-pressed={game === g}
              onClick={() => setGame(g)}
              className={`tone-${g === null ? "accent" : gameTone(g)} relative inline-flex h-9 shrink-0 items-center gap-2 rounded-full bg-white px-4 text-[0.88rem] font-bold shadow-sm transition-colors ${game === g ? "text-ink" : "text-ink-soft hover:text-ink"}`}
            >
              {game === g && <motion.span layoutId="game-pill" className="absolute inset-0 rounded-full bg-[color-mix(in_srgb,var(--tone)_24%,var(--ibox-white))] shadow-[inset_0_0_0_2px_color-mix(in_srgb,var(--tone)_55%,transparent)]" transition={SPRING.snappy} />}
              <span className="relative h-2.5 w-2.5 rounded-full bg-[var(--tone)]" aria-hidden="true" />
              <span className="relative">{g === null ? t("Tous les jeux") : g || t("Hors jeu")}</span>
            </button>
          ))}
        </nav>
      )}

      <main className="min-h-0 flex-1 overflow-y-auto px-8 pt-3 pb-8">
        {clips && shown.length === 0 ? (
          <Empty favorites={filter === "favorites" && clips.length > 0} />
        ) : (
          <motion.ul layout className="grid grid-cols-[repeat(auto-fill,minmax(17rem,1fr))] gap-5">
            <AnimatePresence mode="popLayout">
            {shown.map((c, i) => (
              <motion.li key={c.name} layout initial={{ opacity: 0, y: 14, scale: 0.97 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.15 } }} transition={{ ...SPRING.smooth, delay: Math.min(i, 12) * 0.035 }}>
                <Card
                  clip={c}
                  order={picked ? picked.indexOf(c.name) + 1 : 0}
                  check={selected ? selected.includes(c.name) : null}
                  onOpen={(e) => {
                    // Ctrl + clic : cocher sans entrer dans le lecteur ; Maj + clic : la plage
                    if (selected || (!picked && (e.ctrlKey || e.shiftKey))) { if (!selected) startSelect(); toggle(c.name, e.shiftKey); return; }
                    if (picked) { if (!isImage(c)) setPicked((p) => p && (p.includes(c.name) ? p.filter((n) => n !== c.name) : [...p, c.name])); return; }
                    setStartMode(null);
                    setOpen(c.name);
                  }}
                  onMenu={(x, y, from) => setMenu({ name: c.name, x, y, from })}
                />
              </motion.li>
            ))}
            </AnimatePresence>
          </motion.ul>
        )}
      </main>

      <AnimatePresence>
        {current && (
          <Player
            key={current.name}
            clip={current}
            confirm={confirm}
            startMode={startMode}
            onSettings={() => setSettings("sound")}
            onClose={() => { setConfirm(false); setSharing(false); setStartMode(null); setOpen(null); }}
            sharing={sharing}
            setSharing={setSharing}
            onTitle={(title) => void act("clips.update", { name: current.name, title })}
            onFavorite={() => void act("clips.update", { name: current.name, favorite: !current.favorite })}
            onDelete={() => (confirm ? (setConfirm(false), setOpen(null), void act("clips.delete", { name: current.name })) : setConfirm(true))}
            onCancel={() => setConfirm(false)}
            busy={!!job}
            keys={!auto && !settings && !help}
            onRender={(kind, span, focus) => trim(current, kind, span, focus)}
            onMix={(gameVol, mic) => startRender({ kind: "mix", name: current.name, game: gameVol, mic }, t("Mixage du son…"))}
          />
        )}
      </AnimatePresence>

      <AnimatePresence>{auto && <AutoSheet onClose={() => setAuto(false)} />}</AnimatePresence>
      <AnimatePresence>{settings && <SettingsSheet focus={settings === "sound" ? "sound" : undefined} onClose={() => setSettings(false)} />}</AnimatePresence>
      <AnimatePresence>{help && <KeysSheet onClose={() => setHelp(false)} />}</AnimatePresence>

      <AnimatePresence>
        {selected && !open && (
          <motion.div role="toolbar" aria-label={t("Sélection")} initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.smooth}
            className="fixed bottom-6 left-1/2 z-[5] flex -translate-x-1/2 items-center gap-3 rounded-full bg-white py-2 pr-2 pl-6 shadow-lg">
            <AnimatePresence mode="popLayout" initial={false}>
              {askDelete ? (
                <motion.div key="ask" className="flex items-center gap-3" initial={{ opacity: 0, x: 12 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -12 }} transition={SPRING.smooth}>
                  <span className="tone-red grid h-8 w-8 place-items-center rounded-full bg-[color-mix(in_srgb,var(--tone)_18%,var(--ibox-white))] text-[var(--tone-ink)]"><Icon name="trash" size="1.05em" /></span>
                  <span className="text-[0.92rem] font-bold whitespace-nowrap text-ink">{selected.length === 1 ? t("Supprimer 1 clip ? Il est effacé de ce PC.") : t("Supprimer {0} clips ? Ils sont effacés de ce PC.", selected.length)}</span>
                  <Button variant="danger" tone="red" icon="trash" disabled={deleting} onClick={() => void deleteSelected()}>{deleting ? t("Suppression…") : t("Supprimer")}</Button>
                  <Button data-cancel tone="blue" variant="ghost" disabled={deleting} onClick={() => setAskDelete(false)} autoFocus>{t("Annuler")}</Button>
                </motion.div>
              ) : (
                <motion.div key="bar" className="flex items-center gap-3" initial={{ opacity: 0, x: -12 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 12 }} transition={SPRING.smooth}>
                  <motion.span key={selected.length} initial={{ scale: 1.15 }} animate={{ scale: 1 }} transition={SPRING.pop} className="min-w-[9rem] text-[0.92rem] font-bold text-ink tabular-nums">
                    {selected.length === 0 ? t("Touchez les clips à choisir") : selected.length === 1 ? t("1 sélectionné") : t("{0} sélectionnés", selected.length)}
                  </motion.span>
                  <Button tone="blue" variant="soft" icon="check" title={hint(allShown ? t("Tout désélectionner") : t("Tout sélectionner"), "Ctrl+A")} disabled={!shown.length} onClick={selectAll}>{allShown ? t("Tout désélectionner") : t("Tout sélectionner")}</Button>
                  <Button tone="red" variant="primary" icon="trash" title={hint(t("Supprimer"), t("Suppr"))} disabled={!selected.length} onClick={() => setAskDelete(true)}>{selected.length ? t("Supprimer ({0})", selected.length) : t("Supprimer")}</Button>
                  <Button tone="blue" variant="ghost" onClick={endSelect}>{t("Terminé")}</Button>
                </motion.div>
              )}
            </AnimatePresence>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {picked && !open && (
          <motion.div data-nav-scope initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.smooth} className="fixed bottom-6 left-1/2 flex -translate-x-1/2 items-center gap-3 rounded-full bg-white py-2 pr-2 pl-6 shadow-lg">
            <span className="text-[0.92rem] font-bold text-ink">{picked.length < 2 ? t("Choisissez les clips dans l’ordre voulu") : t("{0} clips choisis", picked.length)}</span>
            <Button tone="orange" variant="primary" icon="copy" disabled={picked.length < 2 || !!job} onClick={join}>{t("Assembler")}</Button>
            <Button tone="red" variant="ghost" onClick={() => setPicked(null)}>{t("Annuler")}</Button>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {job && (
          <motion.div role="status" initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.smooth} className="fixed top-6 left-1/2 z-20 w-[22rem] -translate-x-1/2 rounded-xl bg-white px-5 py-3 shadow-lg">
            <div className="flex justify-between text-[0.88rem] font-bold text-ink"><span>{job.label}</span><span>{Math.round(job.ratio * 100)} %</span></div>
            <div className="mt-2 h-2 overflow-hidden rounded-full bg-cream-deep">
              <motion.div className="ibox-progress h-full rounded-full" animate={{ width: `${Math.max(4, job.ratio * 100)}%` }} transition={SPRING.smooth} />
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {error && (
          <motion.div role="alert" initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.snappy} className="fixed bottom-6 left-1/2 -translate-x-1/2 rounded-full bg-ink px-5 py-3 text-[0.9rem] font-bold text-cream shadow-lg">
            {error}
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {notice && (
          <motion.div role="status" initial={{ opacity: 0, y: -12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -12 }} transition={SPRING.snappy} className="tone-green fixed top-6 left-1/2 z-20 inline-flex -translate-x-1/2 items-center gap-2 rounded-full bg-[var(--tone)] px-5 py-3 text-[0.9rem] font-bold text-[var(--tone-on)] shadow-lg">
            <Icon name="check" />{notice}
          </motion.div>
        )}
      </AnimatePresence>

      {/* menu d'un clip : dernier de la page, il garde la navigation pour lui (nav.ts) */}
      <AnimatePresence>
        {menu && (() => {
          const c = clips?.find((x) => x.name === menu.name);
          return c && (
            <ClipMenu
              key={menu.name}
              clip={c}
              at={menu}
              onClose={closeMenu}
              onOpen={() => { setMenu(null); setStartMode(null); setOpen(c.name); }}
              onEdit={() => { setMenu(null); setStartMode("cut"); setOpen(c.name); }}
              onDelete={() => { setMenu(null); void act("clips.delete", { name: c.name }); }}
              onSelect={() => { setMenu(null); if (selected) { if (!selected.includes(c.name)) toggle(c.name); } else startSelect(c.name); }}
            />
          );
        })()}
      </AnimatePresence>
    </div>
  );
}

function label(c: Clip) {
  return c.title ?? c.game ?? t("Hors jeu");
}

/** Teinte d'une carte : clip en violet, capture et GIF en rose. */
function kindTone(c: Clip): Tone {
  return isImage(c) ? "pink" : "accent";
}

function autoLabel(how: NonNullable<Clip["auto"]>) {
  return how === "voice" ? t("Gardé à la voix") : how === "pad" ? t("Gardé à la manette") : t("Gardé en fin de partie");
}

function Card({ clip: c, order, check, onOpen, onMenu }: { clip: Clip; order: number; check: boolean | null; onOpen: (e: React.MouseEvent) => void; onMenu: (x: number, y: number, from: HTMLElement) => void }) {
  const [length, setLength] = useState(c.duration ?? 0);
  const src = clipUrl(c);
  const thumb = thumbUrl(c);
  return (
    <button type="button" data-nav data-card draggable aria-haspopup="menu" aria-pressed={check ?? undefined} onDragStart={(e) => dragOut(e, c)} onClick={onOpen}
      onContextMenu={(e) => {
        // clic droit : au pointeur ; touche Menu, Maj+F10 ou manette (Y, Menu) : au centre de la carte
        e.preventDefault();
        const el = e.currentTarget;
        const r = el.getBoundingClientRect();
        const pointer = e.button === 2 && e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
        el.focus();
        onMenu(pointer ? e.clientX : r.left + r.width / 2, pointer ? e.clientY : r.top + r.height / 2, el);
      }}
      onKeyDown={(e) => {
        // touche Menu ou Maj+F10 : Windows ne l'envoie pas toujours comme un clic droit
        if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) { e.preventDefault(); openMenu(e.currentTarget); }
      }}
      className={`tone-${kindTone(c)} ibox-tile group relative block w-full overflow-hidden rounded-xl text-left transition-transform duration-200 ${check ? "scale-[0.97] shadow-[0_0_0_3px_var(--accent)]" : ""}`}>
      <div className="relative aspect-video bg-cream-deep">
        {/* première image du clip, sans le son : rien ne joue avant l'ouverture */}
        {isImage(c) ? (
          src && <img src={src} alt="" draggable={false} className="absolute inset-0 h-full w-full object-cover" />
        ) : thumb ? (
          <img src={thumb} alt="" className="absolute inset-0 h-full w-full object-cover" />
        ) : (
          src && <video src={`${src}#t=0.5`} preload="metadata" muted playsInline tabIndex={-1} onLoadedMetadata={(e) => setLength(e.currentTarget.duration)} className="absolute inset-0 h-full w-full object-cover" />
        )}
        <span className="absolute inset-0 grid place-items-center opacity-0 transition group-hover:opacity-100 group-focus:opacity-100">
          <span className="grid h-14 w-14 scale-75 place-items-center rounded-full bg-[var(--tone)] text-[#fff] shadow-[0_8px_24px_-6px_var(--tone)] transition-transform duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] group-hover:scale-100 group-focus:scale-100"><Icon name={isImage(c) ? "camera" : "play"} size="1.5em" /></span>
        </span>
        {length > 0 && <span className="absolute right-2 bottom-2 rounded-full bg-black/60 px-2 py-0.5 text-[0.75rem] font-extrabold text-[#fff]">{duration(length)}</span>}
        {c.favorite && <span className="tone-yellow absolute top-2 left-2 grid h-8 w-8 place-items-center rounded-full bg-[var(--tone)] text-[var(--tone-on)] shadow-md" aria-label={t("Favori")}><Icon name="star" filled size="1.05em" /></span>}
        {check === null && c.kind !== "video" && c.kind && (
          <span className="absolute top-2 right-2 rounded-full bg-[var(--tone)] px-2.5 py-0.5 text-[0.72rem] font-extrabold tracking-wide text-[var(--tone-on)] uppercase shadow-md">{c.kind === "gif" ? "GIF" : t("Capture")}</span>
        )}
        {c.auto && (
          <span className="absolute bottom-2 left-2 inline-flex items-center gap-1 rounded-full bg-black/60 px-2 py-0.5 text-[0.75rem] font-extrabold text-[#fff]" title={autoLabel(c.auto)}>
            <Icon name={c.auto === "voice" ? "mic" : c.auto === "pad" ? "pad" : "flag"} size="1.1em" />
            <span className="sr-only">{autoLabel(c.auto)}</span>
          </span>
        )}
        {/* sélection : voile et case ronde en haut à droite */}
        <AnimatePresence>
          {check !== null && (
            <motion.span aria-hidden="true" initial={{ scale: 0, opacity: 0 }} animate={{ scale: 1, opacity: 1 }} exit={{ scale: 0, opacity: 0 }} transition={SPRING.pop}
              className={`absolute top-2 right-2 z-[1] grid h-8 w-8 place-items-center rounded-full border-2 shadow-md transition-colors ${check ? "border-[var(--accent)] bg-[var(--accent)] text-[#fff]" : "border-[#fff] bg-black/35 text-transparent"}`}>
              <Icon name="check" size="1.05em" />
            </motion.span>
          )}
        </AnimatePresence>
        {check && <span aria-hidden="true" className="absolute inset-0 bg-[var(--accent)]/15" />}
        {order > 0 && <motion.span initial={{ scale: 0 }} animate={{ scale: 1 }} transition={SPRING.pop} className="tone-orange absolute top-2 right-2 grid h-8 w-8 place-items-center rounded-full bg-[var(--tone)] text-[0.95rem] font-extrabold text-[#fff] shadow-md">{order}</motion.span>}
      </div>
      <div className="px-4 py-3">
        <div className="truncate text-[0.98rem] font-bold text-ink">{label(c)}</div>
        <div className="truncate text-[0.8rem] font-semibold text-ink-soft">
          {c.title && c.game ? `${c.game} · ` : ""}{when(c.at)} · {size(c.size)}
        </div>
      </div>
    </button>
  );
}

function Player({ clip: c, confirm, onClose, onTitle, onFavorite, onDelete, onCancel, busy, keys, sharing, setSharing, onRender, onMix, startMode, onSettings }: {
  startMode: "cut" | null; onSettings: () => void;
  clip: Clip; confirm: boolean; onClose: () => void; onTitle: (t: string) => void; onFavorite: () => void; onDelete: () => void; onCancel: () => void;
  busy: boolean; keys: boolean; sharing: boolean; setSharing: (v: boolean) => void; onRender: (kind: "mp4" | "gif" | "vertical", span: Span, focus?: number) => void; onMix: (game: number, mic: number) => void;
}) {
  const [title, setTitle] = useState(c.title ?? "");
  const [failed, setFailed] = useState(false);
  /** Couper, ou format vertical (cadre 9:16 à placer sur l'image). */
  const [mode, setMode] = useState<"cut" | "vertical" | "sound" | null>(isImage(c) ? null : startMode);
  /** Outil déplié : le masque de l'animation tombe (les bulles dépassent du panneau). */
  const [settled, setSettled] = useState(false);
  const [focus, setFocus] = useState(0.5);
  /** Vidéo déjà verticale (ou carrée) : pas de format vertical à en tirer. */
  const [portrait, setPortrait] = useState(false);
  const [length, setLength] = useState(c.duration ?? 0);
  const video = useRef<HTMLVideoElement>(null);
  // le son du clip rejoué ne doit pas déclencher « Clip ça ! » (quiet.ts)
  useQuietVoice(video, failed);
  const back = useRef<HTMLButtonElement>(null);
  const [note, setNote] = useState<string | null>(null);
  useEffect(() => { if (!note) return; const id = setTimeout(() => setNote(null), 3000); return () => clearTimeout(id); }, [note]);
  const toggle = (m: "cut" | "vertical" | "sound") => setMode((cur) => (cur === m ? null : m));
  // lecteur : Espace, J et L, C, V, S, P, F, Suppr, Ctrl+C (I et O : dans Couper)
  useKeys(keys && !sharing, (k) => {
    const v = video.current;
    const media = !isImage(c);
    switch (k) {
      case "space": if (v) { if (v.paused) void v.play(); else v.pause(); } return true;
      case "j": if (v) v.currentTime = Math.max(0, v.currentTime - 1); return true;
      case "l": if (v) v.currentTime = Math.min(v.duration || 0, v.currentTime + 1); return true;
      case "c": if (media) toggle("cut"); return true;
      case "v": if (media && !portrait) toggle("vertical"); return true;
      case "s": if (media) toggle("sound"); return true;
      case "p": setSharing(true); return true;
      case "f": onFavorite(); return true;
      case "Delete": onDelete(); return true;
      case "ctrl+c":
        void host("share.copy", { name: c.name }).then(() => setNote(t("Clip copié : collez-le (Ctrl+V) dans Discord ou un dossier."))).catch((e: Error) => setNote(e.message));
        return true;
    }
    return false;
  });
  useEffect(() => back.current?.focus(), []);
  const commit = () => { if (title.trim() !== (c.title ?? "")) onTitle(title); };
  const src = clipUrl(c);
  return (
    <motion.div data-nav-scope className="fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-8" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <motion.div role="dialog" aria-modal="true" aria-label={label(c)} className="flex max-h-full w-full max-w-[min(78rem,100%)] flex-col overflow-hidden rounded-xl bg-white shadow-lg" initial={{ scale: 0.96, y: 12 }} animate={{ scale: 1, y: 0 }} exit={{ scale: 0.97, y: 8, transition: { duration: 0.15 } }} transition={SPRING.smooth}>
        <div className="relative min-h-0 flex-1 bg-black">
          {src && isImage(c) ? (
            <img src={src} alt="" className="mx-auto max-h-[60vh] object-contain" />
          ) : src && !failed ? (
            <>
              <video ref={video} src={src} controls={mode !== "vertical"} autoPlay playsInline draggable onDragStart={(e) => dragOut(e, c)} onError={() => setFailed(true)} onLoadedMetadata={(e) => { const v = e.currentTarget; setLength(v.duration); setPortrait(v.videoWidth > 0 && v.videoWidth * 16 <= v.videoHeight * 9 + 16); }} className="max-h-[60vh] w-full bg-black object-contain" />
              {mode === "vertical" && <Vertical video={video} focus={focus} onFocus={setFocus} />}
            </>
          ) : (
            <div className="grid aspect-video place-items-center text-[0.95rem] font-bold text-[#fff]/70">{src ? t("Lecture impossible.") : c.game ?? ""}</div>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-3 px-6 py-4">
          <Button ref={back} tone="blue" icon="back" variant="ghost" onClick={onClose}>{t("Retour")}</Button>
          <input
            data-nav
            value={title}
            maxLength={80}
            placeholder={c.game ?? t("Sans titre")}
            aria-label={t("Titre du clip")}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => { if (e.key === "Enter") { e.currentTarget.blur(); } }}
            className="h-11 min-w-0 flex-1 rounded-full bg-cream-deep px-5 text-[0.95rem] font-bold text-ink outline-none placeholder:text-ink-faint"
          />
          {!isImage(c) && <Button tone="orange" icon="sound" title={hint(t("Son"), "S")} aria-pressed={mode === "sound"} variant={mode === "sound" ? "primary" : "soft"} onClick={() => setMode((m) => (m === "sound" ? null : "sound"))}>{t("Son")}</Button>}
          {!isImage(c) && <Button tone="orange" icon="cut" title={hint(t("Couper"), "C")} aria-pressed={mode === "cut"} variant={mode === "cut" ? "primary" : "soft"} onClick={() => setMode((m) => (m === "cut" ? null : "cut"))}>{t("Couper")}</Button>}
          {!isImage(c) && !portrait && <Button tone="pink" icon="phone" title={hint(t("Vertical"), "V")} aria-pressed={mode === "vertical"} variant={mode === "vertical" ? "primary" : "soft"} onClick={() => setMode((m) => (m === "vertical" ? null : "vertical"))}>{t("Vertical")}</Button>}
          <Button tone="green" variant="primary" icon="share" title={hint(t("Partager"), "P")} onClick={() => setSharing(true)}>{t("Partager")}</Button>
          <Button tone="yellow" icon="star" title={hint(t("Favori"), "F")} filled={c.favorite} onClick={onFavorite} aria-pressed={c.favorite}>{c.favorite ? t("Retirer des favoris") : t("Ajouter aux favoris")}</Button>
          {confirm ? (
            <>
              <span className="text-[0.9rem] font-bold text-ink">{t("Supprimer ce clip ? Il est effacé de ce PC.")}</span>
              <Button variant="danger" icon="trash" onClick={onDelete} autoFocus>{t("Supprimer")}</Button>
              <Button tone="blue" variant="ghost" onClick={onCancel}>{t("Annuler")}</Button>
            </>
          ) : (
            <Button tone="red" icon="trash" title={hint(t("Supprimer"), t("Suppr"))} onClick={onDelete}>{t("Supprimer")}</Button>
          )}
        </div>
        {/* outils du lecteur : ils se déplient et se replient sur un ressort */}
        <AnimatePresence initial={false} mode="wait">
          {mode && (
            <motion.div key={mode} className={`border-t border-line ${settled ? "" : "overflow-hidden"}`}
              onAnimationStart={() => setSettled(false)} onAnimationComplete={() => setSettled(true)}
              initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0, transition: { duration: 0.16 } }}
              transition={{ height: SPRING.smooth, opacity: { duration: 0.2 } }}>
              {mode === "sound" ? (
                <SoundMix clip={c} video={video} busy={busy} onMix={onMix} onSettings={onSettings} />
              ) : (
                <Trim video={video} src={src} length={length} busy={busy} vertical={mode === "vertical"} onRender={(kind, span) => onRender(kind, span, focus)} />
              )}
            </motion.div>
          )}
        </AnimatePresence>
        <div className="flex items-center gap-3 px-6 pb-4 text-[0.8rem] font-semibold text-ink-soft">
          <span className="min-w-0 flex-1 truncate">{[c.game, when(c.at), size(c.size), c.auto ? autoLabel(c.auto) : null].filter(Boolean).join(" · ")}</span>
          <AnimatePresence>{note && <motion.span role="status" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={SPRING.snappy} className="font-bold text-accent-text">{note}</motion.span>}</AnimatePresence>
          <span className="shrink-0 text-ink-faint">{t("? : raccourcis")}</span>
        </div>
      </motion.div>
      <AnimatePresence>
        {sharing && <ShareSheet clip={c} onClose={() => setSharing(false)} onVertical={isImage(c) || portrait ? null : () => { setSharing(false); setMode("vertical"); }} />}
      </AnimatePresence>
    </motion.div>
  );
}

function Empty({ favorites }: { favorites: boolean }) {
  return (
    <div className="grid h-full place-items-center">
      <div className="max-w-md text-center">
        <img src={logo} alt="" className="mx-auto mb-5 h-24 w-24 opacity-90" />
        <h2 className="font-display text-[1.3rem] text-ink">{favorites ? t("Aucun favori") : t("Aucun clip pour l’instant")}</h2>
        <p className="mt-2 text-[0.95rem] font-semibold text-ink-soft">
          {favorites ? t("Marquez un clip d’une étoile pour le retrouver ici.") : standalone ? t("Pendant une partie, ouvrez le panneau (Alt+X) ou dites « Clip ça ! » : le clip arrive ici.") : t("Pendant une partie, ouvrez l’overlay d’iBox et gardez les dernières secondes : le clip arrive ici.")}
        </p>
      </div>
    </div>
  );
}
