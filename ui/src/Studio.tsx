/**
 * iClips Studio : la galerie des clips du profil iBox (vignettes, filtre par jeu et favoris),
 * un lecteur, le titre et le favori de chaque clip, la suppression, et le montage (0.3) :
 * couper un passage, en faire un GIF, assembler plusieurs clips. Clips automatiques (0.4) :
 * réglages dans `Auto.tsx`, marque sur les cartes. Le partage vient ensuite (docs/CONCEPTION.md).
 *
 * Studio 0.9, sur le modèle des studios de clips de jeu :
 * - galerie rangée par date (aujourd'hui, hier, 7 derniers jours, puis par mois), vignettes en
 *   trois tailles (+ et -), aperçu muet au survol, actions rapides sur chaque carte (favori,
 *   copier, partager, supprimer) ;
 * - lecteur de studio qui prend toute la fenêtre : outils sous l'image, liste des clips à
 *   droite, clip précédent ou suivant (Page préc., Page suiv. ; LB, RB), vitesse, image par
 *   image, plein écran ;
 * - assemblage dans une bande de montage à réordonner en glissant les vignettes.
 */
import { AnimatePresence, motion, Reorder } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";
import { clipUrl, host, onHost, standalone, thumbUrl, type Clip } from "./host";
import { makeThumb } from "./thumbs";
import { joinSegments, totalLength, useRender, type Span } from "./edit";
import { Trim } from "./Trim";
import { AutoSheet } from "./Auto";
import { dragOut, ShareSheet } from "./Share";
import { Vertical } from "./Vertical";
import { SettingsSheet } from "./Settings";
import { SoundMix } from "./Sound";
import { ClipMenu } from "./Menu";
import { useQuietVoice } from "./quiet";
import { useLiveMix } from "./mix";
import { exitFullscreen, frameStep, isFullscreen, nextSpeed, Playback, seekBy, SKIP, toggle as playPause, toggleFullscreen } from "./Playback";
import { hint, KeysSheet, useKeys } from "./keys";
import { byDate, duration, size, when } from "./format";
import { t } from "./i18n";
import { openMenu, useNav } from "./nav";
import { Button, Icon, type IconName, type Tone } from "./ui";
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

/** Largeur mini des vignettes : petites, moyennes, grandes (+ et -). */
const GRID = ["12.5rem", "17rem", "24rem"] as const;
/** Hauteur de la zone des outils du lecteur : celle de Couper, l'outil le plus haut (l'image ne bouge pas d'un outil à l'autre). */
const TOOL_HEIGHT = "15.5rem";
const GRID_KEY = "iclips.studio.grid";
function readGrid(): number { try { const n = Number(localStorage.getItem(GRID_KEY)); return n >= 0 && n < GRID.length && localStorage.getItem(GRID_KEY) !== null ? n : 1; } catch { return 1; } }
function writeGrid(n: number) { try { localStorage.setItem(GRID_KEY, String(n)); } catch { /* sans stockage : non gardée */ } }

type Quick = "favorite" | "copy" | "share" | "delete";

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
  const [startMode, setStartMode] = useState<"cut" | "vertical" | null>(null);
  const [help, setHelp] = useState(false);
  /** Mémoire des clips coupée : rien ne peut être gardé en partie (bandeau). */
  const [memoryOff, setMemoryOff] = useState(false);
  const checkMemory = useCallback(() => {
    host<{ enabled: boolean }>("capture.get").then((c) => setMemoryOff(!c.enabled)).catch(() => undefined);
  }, []);
  useEffect(() => { if (!settings) checkMemory(); }, [settings, checkMemory]);
  /** Fiche Partager du lecteur (ici : Échap la ferme seule, pas le lecteur). */
  const [sharing, setSharing] = useState(false);
  /** Fiche Partager ouverte depuis une carte de la galerie. */
  const [shareOf, setShareOf] = useState<string | null>(null);
  /** Taille des vignettes (0 à 2) et vitesse du lecteur, gardée d'un clip à l'autre. */
  const [grid, setGridState] = useState(readGrid);
  const setGrid = (n: number) => { const v = Math.min(GRID.length - 1, Math.max(0, n)); setGridState(v); writeGrid(v); };
  const [speed, setSpeed] = useState(1);

  const load = useCallback(() => {
    host<Clip[]>("clips.list").then(setClips).catch((e: Error) => { setClips([]); setError(t("Action impossible : {0}", e.message)); });
  }, []);
  useEffect(() => { load(); return onHost((event) => { if (event === "clips://changed") load(); }); }, [load]);
  useEffect(() => { if (!error) return; const id = setTimeout(() => setError(null), 4000); return () => clearTimeout(id); }, [error]);
  useEffect(() => { if (!notice) return; const id = setTimeout(() => setNotice(null), 3000); return () => clearTimeout(id); }, [notice]);

  const games = useMemo(() => [...new Set((clips ?? []).map((c) => c.game ?? ""))].sort((a, b) => a.localeCompare(b)), [clips]);
  /** Groupes par date ; `shown` suit l'ordre affiché (plages de Maj + clic, clip suivant). */
  const groups = useMemo(() => byDate((clips ?? []).filter((c) =>
    (filter === "all" || (filter === "favorites" && c.favorite) || (filter === "videos" && c.kind !== "image") || (filter === "images" && c.kind === "image"))
    && (game === null || (c.game ?? "") === game))), [clips, filter, game]);
  const shown = useMemo(() => groups.flatMap((g) => g.items), [groups]);

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
  /** Clips que le lecteur fait défiler : ceux de la galerie, ou le clip seul s'il n'y est plus. */
  const queue = current && !shown.some((c) => c.name === current.name) ? [current] : shown;

  /** Coupe qui remplace l'original : effacé seulement une fois le nouveau clip prêt. */
  const replacing = useRef<Clip | null>(null);
  const { job, render } = useRender(
    (name) => {
      const orig = replacing.current;
      replacing.current = null;
      void (async () => {
        if (orig) {
          if (orig.favorite) await host("clips.update", { name, favorite: true }).catch(() => undefined);
          try { await host("clips.delete", { name: orig.name }); setNotice(t("Clip coupé : il remplace l’original.")); }
          catch (e) { setError(t("L’original n’a pas pu être effacé : {0}", (e as Error).message)); }
        }
        load(); setPicked(null); setOpen(name);
      })();
    },
    (message) => { replacing.current = null; setError(t("Montage impossible : {0}", message)); },
  );
  const startRender = (arg: Record<string, unknown>, label: string) =>
    void render(arg, label).catch((e: Error) => { replacing.current = null; setError(t("Montage impossible : {0}", e.message)); });
  const trim = (c: Clip, kind: "mp4" | "gif" | "vertical", span: Span, focus = 0.5, replace = false) => {
    if (kind === "mp4") {
      replacing.current = replace ? c : null;
      const title = replace ? c.title ?? undefined : c.title ? t("{0} (coupé)", c.title) : undefined;
      startRender({ kind, segments: [{ name: c.name, ...span }], title }, t("Coupe du clip…"));
    } else if (kind === "vertical") startRender({ kind, name: c.name, ...span, focus }, t("Format vertical…"));
    else startRender({ kind, name: c.name, ...span }, t("Création du GIF…"));
  };
  const pickedClips = (picked ?? []).map((n) => clips?.find((c) => c.name === n)).filter((c): c is Clip => !!c);
  const join = () => startRender({ kind: "mp4", segments: joinSegments(pickedClips) }, t("Assemblage de {0} clips…", pickedClips.length));

  const act = async (call: string, arg: Record<string, unknown>) => {
    try { await host(call, arg); load(); } catch (e) { setError(t("Action impossible : {0}", (e as Error).message)); }
  };
  const copy = (c: Clip) => void host("share.copy", { name: c.name }).then(() => setNotice(t("Clip copié : collez-le (Ctrl+V) dans Discord."))).catch((e: Error) => setError(e.message));
  const quick = (c: Clip, what: Quick) => {
    if (what === "favorite") void act("clips.update", { name: c.name, favorite: !c.favorite });
    else if (what === "copy") copy(c);
    else if (what === "share") setShareOf(c.name);
    else void act("clips.delete", { name: c.name }).then(() => setNotice(t("1 clip supprimé")));
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

  // galerie : 1 à 4 (filtres), Ctrl+A (assembler), A (automatique), R (réglages), + et - (vignettes) ; ? partout
  useKeys(!open && !auto && !settings && !help && !menu && !shareOf, (k) => {
    const n = Number(k);
    if (n >= 1 && n <= FILTERS.length) { setFilter(FILTERS[n - 1]); return true; }
    // en sélection : Ctrl+A coche tout (ou rien), Suppr demande la suppression
    if (selected && k === "ctrl+a") { selectAll(); return true; }
    if (selected && k === "Delete") { if (selected.length) setAskDelete(true); return true; }
    if (k === "s") { if (selected) endSelect(); else startSelect(); return true; }
    if (k === "ctrl+a") { setSelected(null); setPicked((p) => (p ? null : [])); return true; }
    if (k === "a") { setAuto(true); return true; }
    if (k === "r") { setSettings("all"); return true; }
    if (k === "+" || k === "=") { setGrid(grid + 1); return true; }
    if (k === "-") { setGrid(grid - 1); return true; }
    return false;
  });
  useKeys(!help, (k) => (k === "?" ? (setHelp(true), true) : false));

  useNav(() => {
    if (isFullscreen()) exitFullscreen();
    else if (menu) closeMenu();
    else if (help) setHelp(false);
    else if (settings) setSettings(false);
    else if (auto) setAuto(false);
    else if (shareOf) setShareOf(null);
    else if (sharing) setSharing(false);
    else if (confirm) setConfirm(false);
    else if (open) setOpen(null);
    else if (askDelete) setAskDelete(false);
    else if (selected) endSelect();
    else if (picked) setPicked(null);
    else void host("screen.close");
  });

  // la manette arrive sur la première carte ; en sortant du lecteur, sur la carte du clip vu
  const lastOpen = useRef<string | null>(null);
  useEffect(() => {
    if (open) { lastOpen.current = open; return; }
    if (!clips || document.activeElement?.hasAttribute("data-nav")) return;
    const back = lastOpen.current && document.querySelector<HTMLElement>(`[data-card="${CSS.escape(lastOpen.current)}"]`);
    (back || document.querySelector<HTMLElement>("[data-card]"))?.focus();
  }, [clips, open]);

  const go = (name: string) => { setConfirm(false); setSharing(false); setStartMode(null); setOpen(name); };
  /** Supprime un clip ; s'il est dans le lecteur, le suivant prend sa place. */
  const remove = (name: string) => {
    if (open === name) {
      const i = queue.findIndex((c) => c.name === name);
      const after = queue[i + 1] ?? queue[i - 1] ?? null;
      setOpen(after?.name ?? null);
    }
    void act("clips.delete", { name });
  };
  const sharedClip = shareOf ? clips?.find((c) => c.name === shareOf) ?? null : null;

  return (
    <div className="flex h-full flex-col">
      <div className="ibox-rainbow h-1 w-full opacity-80" aria-hidden="true" />
      <header className="flex flex-wrap items-center gap-x-4 gap-y-3 px-8 pt-5 pb-4">
        <motion.img src={logo} alt="" className="h-12 w-12 drop-shadow-[0_6px_14px_var(--accent-glow)]" initial={{ rotate: -12, scale: 0.8 }} animate={{ rotate: 0, scale: 1 }} transition={SPRING.pop} />
        <div className="min-w-[11rem] flex-1">
          <h1 className="font-display text-[1.6rem] leading-tight text-ink">{t("iClips Studio")}</h1>
          {clips && <p className="text-[0.85rem] font-semibold text-ink-soft">{clips.length === 1 ? t("1 clip") : t("{0} clips", clips.length)}{clips.length > 0 ? ` · ${size(clips.reduce((s, c) => s + c.size, 0))}` : ""}</p>}
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
        <GridSize value={grid} onChange={setGrid} />
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

      <main className={`min-h-0 flex-1 overflow-y-auto px-8 pb-8 transition-[padding] ${picked ? "pb-56" : ""}`}>
        {clips && shown.length === 0 ? (
          <Empty favorites={filter === "favorites" && clips.length > 0} />
        ) : (
          groups.map((g, gi) => (
            <section key={g.key} aria-label={g.label} className="mb-4">
              {/* titre du groupe : il reste en haut pendant qu'on fait défiler ses clips */}
              <h2 className="sticky top-0 z-[6] -mx-2 mb-2 flex items-baseline gap-2.5 bg-[color-mix(in_srgb,var(--ibox-cream)_92%,transparent)] px-2 pt-3 pb-2 backdrop-blur-md">
                <span className="font-display text-[1.08rem] text-ink">{g.label}</span>
                <span className="text-[0.8rem] font-bold text-ink-faint">{g.items.length === 1 ? t("1 clip") : t("{0} clips", g.items.length)}</span>
              </h2>
              <motion.ul layout className="grid gap-5" style={{ gridTemplateColumns: `repeat(auto-fill, minmax(min(${GRID[grid]}, 100%), 1fr))` }}>
                <AnimatePresence mode="popLayout">
                  {g.items.map((c, i) => (
                    <motion.li key={c.name} layout initial={{ opacity: 0, y: 14, scale: 0.97 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.15 } }} transition={{ ...SPRING.smooth, delay: Math.min(i + gi * 2, 12) * 0.035 }}>
                      <Card
                        clip={c}
                        order={picked ? picked.indexOf(c.name) + 1 : 0}
                        check={selected ? selected.includes(c.name) : null}
                        quick={!selected && !picked}
                        onQuick={(what) => quick(c, what)}
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
            </section>
          ))
        )}
      </main>

      <AnimatePresence>
        {current && (
          <Player
            key="player"
            clip={current}
            queue={queue}
            onGo={go}
            speed={speed}
            onSpeed={setSpeed}
            confirm={confirm}
            startMode={startMode}
            onSettings={() => setSettings("sound")}
            onClose={() => { setConfirm(false); setSharing(false); setStartMode(null); setOpen(null); }}
            sharing={sharing}
            setSharing={setSharing}
            onTitle={(title) => void act("clips.update", { name: current.name, title })}
            onFavorite={() => void act("clips.update", { name: current.name, favorite: !current.favorite })}
            onCopy={() => copy(current)}
            onDelete={() => {
              if (!confirm) { setConfirm(true); return; }
              setConfirm(false);
              remove(current.name);
            }}
            onMenu={(name, x, y, from) => setMenu({ name, x, y, from })}
            onCancel={() => setConfirm(false)}
            busy={!!job}
            keys={!auto && !settings && !help && !menu && !shareOf}
            onRender={(kind, span, focus, replace) => trim(current, kind, span, focus, replace)}
            onMix={(gameVol, mic) => startRender({ kind: "mix", name: current.name, game: gameVol, mic }, t("Mixage du son…"))}
          />
        )}
      </AnimatePresence>

      <AnimatePresence>
        {sharedClip && <ShareSheet clip={sharedClip} onClose={() => setShareOf(null)} onVertical={isImage(sharedClip) ? null : () => { setShareOf(null); setStartMode("vertical"); setOpen(sharedClip.name); }} />}
      </AnimatePresence>
      <AnimatePresence>{auto && <AutoSheet onClose={() => setAuto(false)} />}</AnimatePresence>
      <AnimatePresence>{settings && <SettingsSheet focus={settings === "sound" ? "sound" : undefined} onClose={() => setSettings(false)} />}</AnimatePresence>
      <AnimatePresence>{help && <KeysSheet onClose={() => setHelp(false)} />}</AnimatePresence>

      <AnimatePresence>
        {selected && !open && (
          <motion.div role="toolbar" aria-label={t("Sélection")} initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.smooth}
            className="fixed bottom-6 left-1/2 z-[7] flex -translate-x-1/2 items-center gap-3 rounded-full bg-white py-2 pr-2 pl-6 shadow-lg">
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
          <MontageStrip clips={pickedClips} busy={!!job} onReorder={setPicked} onRemove={(name) => setPicked((p) => p && p.filter((n) => n !== name))} onJoin={join} onCancel={() => setPicked(null)} />
        )}
      </AnimatePresence>

      <AnimatePresence>
        {job && (
          <motion.div role="status" initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.smooth} className="fixed top-6 left-1/2 z-40 w-[22rem] -translate-x-1/2 rounded-xl bg-white px-5 py-3 shadow-lg">
            <div className="flex justify-between text-[0.88rem] font-bold text-ink"><span>{job.label}</span><span>{Math.round(job.ratio * 100)} %</span></div>
            <div className="mt-2 h-2 overflow-hidden rounded-full bg-cream-deep">
              <motion.div className="ibox-progress h-full rounded-full" animate={{ width: `${Math.max(4, job.ratio * 100)}%` }} transition={SPRING.smooth} />
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {error && (
          <motion.div role="alert" initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.snappy} className="fixed bottom-6 left-1/2 z-40 -translate-x-1/2 rounded-full bg-ink px-5 py-3 text-[0.9rem] font-bold text-cream shadow-lg">
            {error}
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {notice && (
          <motion.div role="status" initial={{ opacity: 0, y: -12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -12 }} transition={SPRING.snappy} className="tone-green fixed top-6 left-1/2 z-40 inline-flex -translate-x-1/2 items-center gap-2 rounded-full bg-[var(--tone)] px-5 py-3 text-[0.9rem] font-bold text-[var(--tone-on)] shadow-lg">
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
              onOpen={() => { setMenu(null); go(c.name); }}
              onEdit={() => { setMenu(null); go(c.name); setStartMode("cut"); }}
              onDelete={() => { setMenu(null); remove(c.name); }}
              // dans le lecteur, pas de sélection : elle se fait dans la galerie
              onSelect={open ? undefined : () => { setMenu(null); if (selected) { if (!selected.includes(c.name)) toggle(c.name); } else startSelect(c.name); }}
              onFavorite={() => { setMenu(null); quick(c, "favorite"); }}
              onCopy={() => { setMenu(null); copy(c); }}
              onShare={() => { setMenu(null); setShareOf(c.name); }}
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

/** Taille des vignettes : trois carrés de plus en plus grands (+ et -). */
function GridSize({ value, onChange }: { value: number; onChange: (n: number) => void }) {
  const names = [t("Petites vignettes"), t("Vignettes moyennes"), t("Grandes vignettes")];
  return (
    <div role="radiogroup" aria-label={t("Taille des vignettes")} className="flex h-11 items-center gap-0.5 rounded-full bg-white px-1.5 shadow-sm">
      {GRID.map((_, i) => (
        <button key={i} type="button" role="radio" data-nav aria-checked={value === i} title={hint(names[i], "+ -")} aria-label={names[i]} onClick={() => onChange(i)}
          className={`grid h-8 w-8 place-items-center rounded-full transition-colors ${value === i ? "bg-accent-soft text-accent-text" : "text-ink-faint hover:text-ink-soft"}`}>
          <span className="rounded-[3px] border-2 border-current" style={{ width: 8 + i * 4, height: 8 + i * 4 }} />
        </button>
      ))}
    </div>
  );
}

/**
 * Menu d'un clip (cartes de la galerie, liste du lecteur) : clic droit au pointeur ; touche Menu,
 * Maj+F10 ou manette (Y, Menu) au centre de l'élément.
 */
function menuHandlers(onMenu: (x: number, y: number, from: HTMLElement) => void) {
  return {
    onContextMenu: (e: React.MouseEvent<HTMLElement>) => {
      e.preventDefault();
      const el = e.currentTarget;
      const r = el.getBoundingClientRect();
      const pointer = e.button === 2 && e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      el.focus();
      onMenu(pointer ? e.clientX : r.left + r.width / 2, pointer ? e.clientY : r.top + r.height / 2, el);
    },
    onKeyDown: (e: React.KeyboardEvent<HTMLElement>) => {
      // touche Menu ou Maj+F10 : Windows ne l'envoie pas toujours comme un clic droit
      if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) { e.preventDefault(); openMenu(e.currentTarget); }
    },
  };
}

function Card({ clip: c, order, check, quick, onOpen, onMenu, onQuick }: {
  clip: Clip; order: number; check: boolean | null; quick: boolean;
  onOpen: (e: React.MouseEvent) => void; onMenu: (x: number, y: number, from: HTMLElement) => void; onQuick: (what: Quick) => void;
}) {
  const [length, setLength] = useState(c.duration ?? 0);
  /** Aperçu au survol : la vidéo se lit muette dans la carte (après un court instant). */
  const [preview, setPreview] = useState(false);
  const [ready, setReady] = useState(false);
  const [progress, setProgress] = useState(0);
  /** Suppression demandée depuis la carte : confirmation sur l'image. */
  const [ask, setAsk] = useState(false);
  const timer = useRef(0);
  const src = clipUrl(c);
  const thumb = thumbUrl(c);
  const media = !isImage(c) && !!src;
  useEffect(() => () => clearTimeout(timer.current), []);
  const enter = () => {
    if (!media) return;
    clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setPreview(true), 320);
  };
  const leave = () => {
    clearTimeout(timer.current);
    setPreview(false);
    setReady(false);
    setProgress(0);
    setAsk(false);
  };
  return (
    <div className="ibox-card group relative" onPointerEnter={enter} onPointerLeave={leave}>
      <button type="button" data-nav data-card={c.name} draggable aria-haspopup="menu" aria-pressed={check ?? undefined} onDragStart={(e) => dragOut(e, c)} onClick={onOpen}
        {...menuHandlers(onMenu)}
        className={`tone-${kindTone(c)} ibox-tile relative block w-full overflow-hidden rounded-xl text-left transition-transform duration-200 ${check ? "scale-[0.97] shadow-[0_0_0_3px_var(--accent)]" : ""}`}>
        <div className="relative aspect-video bg-cream-deep">
          {/* première image du clip, sans le son : rien ne joue avant l'ouverture */}
          {isImage(c) ? (
            src && <img src={src} alt="" draggable={false} className="absolute inset-0 h-full w-full object-cover" />
          ) : thumb ? (
            <img src={thumb} alt="" className="absolute inset-0 h-full w-full object-cover" />
          ) : (
            src && <video src={`${src}#t=0.5`} preload="metadata" muted playsInline tabIndex={-1} onLoadedMetadata={(e) => setLength(e.currentTarget.duration)} className="absolute inset-0 h-full w-full object-cover" />
          )}
          {/* aperçu muet au survol, avec sa progression */}
          {preview && (
            <video src={src} muted autoPlay loop playsInline preload="auto" tabIndex={-1} aria-hidden="true"
              onPlaying={() => setReady(true)}
              onTimeUpdate={(e) => { const v = e.currentTarget; setProgress(v.duration ? v.currentTime / v.duration : 0); }}
              className={`absolute inset-0 h-full w-full object-cover transition-opacity duration-300 ${ready ? "opacity-100" : "opacity-0"}`} />
          )}
          {preview && ready && (
            <span className="absolute inset-x-0 bottom-0 z-[1] h-1 bg-black/35" aria-hidden="true">
              <span className="block h-full bg-[var(--tone)] transition-[width] duration-300 ease-linear" style={{ width: `${progress * 100}%` }} />
            </span>
          )}
          <span className={`absolute inset-0 grid place-items-center opacity-0 transition ${ready ? "" : "group-hover:opacity-100 group-focus-within:opacity-100"}`}>
            <span className="grid h-14 w-14 scale-75 place-items-center rounded-full bg-[var(--tone)] text-[#fff] shadow-[0_8px_24px_-6px_var(--tone)] transition-transform duration-300 ease-[cubic-bezier(0.34,1.56,0.64,1)] group-hover:scale-100 group-focus-within:scale-100"><Icon name={isImage(c) ? "camera" : "play"} size="1.5em" /></span>
          </span>
          {length > 0 && <span className="absolute right-2 bottom-2 z-[1] rounded-full bg-black/60 px-2 py-0.5 text-[0.75rem] font-extrabold text-[#fff]">{duration(length)}</span>}
          {/* au survol, l'étoile des actions rapides prend le relais : une seule étoile visible */}
          {c.favorite && <span className={`tone-yellow absolute top-2 left-2 grid h-8 w-8 place-items-center rounded-full bg-[var(--tone)] text-[var(--tone-on)] shadow-md transition-opacity duration-150 ${quick ? "group-hover:opacity-0" : ""}`} aria-label={t("Favori")}><Icon name="star" filled size="1.05em" /></span>}
          {check === null && c.kind !== "video" && c.kind && (
            <span className={`absolute top-2 right-2 rounded-full bg-[var(--tone)] px-2.5 py-0.5 text-[0.72rem] font-extrabold tracking-wide text-[var(--tone-on)] uppercase shadow-md transition-opacity ${quick ? "group-hover:opacity-0" : ""}`}>{c.kind === "gif" ? "GIF" : t("Capture")}</span>
          )}
          {c.auto && (
            <span className="absolute bottom-2 left-2 z-[1] inline-flex items-center gap-1 rounded-full bg-black/60 px-2 py-0.5 text-[0.75rem] font-extrabold text-[#fff]" title={autoLabel(c.auto)}>
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

      {/* actions rapides au survol : à la souris ; clavier et manette passent par le menu (Y) */}
      {quick && !ask && (
        <div className="absolute top-2 right-2 z-[2] flex gap-1.5 opacity-0 transition-[opacity,transform] duration-200 group-hover:-translate-y-0.5 group-hover:opacity-100">
          <QuickButton icon="star" filled={c.favorite} label={c.favorite ? t("Retirer des favoris") : t("Ajouter aux favoris")} onClick={() => onQuick("favorite")} active={c.favorite} />
          <QuickButton icon="copy" label={t("Copier le clip")} onClick={() => onQuick("copy")} />
          <QuickButton icon="share" label={t("Partager")} onClick={() => onQuick("share")} />
          <QuickButton icon="trash" label={t("Supprimer")} danger onClick={() => setAsk(true)} />
        </div>
      )}
      <AnimatePresence>
        {ask && (
          <motion.div role="alertdialog" aria-label={t("Supprimer")} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0, transition: { duration: 0.12 } }} transition={{ duration: 0.15 }}
            className="absolute inset-x-0 top-0 z-[3] flex aspect-video -translate-y-0.5 flex-col items-center justify-center gap-3 rounded-t-xl bg-black/75 p-3 text-center backdrop-blur-sm">
            <span className="text-[0.92rem] font-bold text-[#fff]">{t("Supprimer de ce PC ?")}</span>
            <motion.div className="flex gap-2" initial={{ y: 6, scale: 0.95 }} animate={{ y: 0, scale: 1 }} transition={SPRING.pop}>
              <Button variant="danger" tone="red" icon="trash" autoFocus onClick={() => { setAsk(false); onQuick("delete"); }}>{t("Supprimer")}</Button>
              <Button tone="blue" variant="soft" onClick={() => setAsk(false)}>{t("Annuler")}</Button>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function QuickButton({ icon, label, onClick, filled, active, danger }: { icon: IconName; label: string; onClick: () => void; filled?: boolean; active?: boolean; danger?: boolean }) {
  return (
    <button type="button" tabIndex={-1} title={label} aria-label={label} onClick={onClick}
      className={`grid h-8 w-8 place-items-center rounded-full shadow-md backdrop-blur-md transition-[background-color,color,transform] duration-150 hover:scale-110 active:scale-90 ${active ? "bg-[var(--tone-yellow)] text-[#2a1f00]" : danger ? "bg-black/60 text-[#fff] hover:bg-[var(--tone-red)]" : "bg-black/60 text-[#fff] hover:bg-[var(--accent)]"}`}>
      <Icon name={icon} filled={filled} size="1.05em" />
    </button>
  );
}

/** Bande de montage : les clips à assembler, dans l'ordre, à réordonner en les glissant. */
function MontageStrip({ clips, busy, onReorder, onRemove, onJoin, onCancel }: {
  clips: Clip[]; busy: boolean; onReorder: (names: string[]) => void; onRemove: (name: string) => void; onJoin: () => void; onCancel: () => void;
}) {
  const total = totalLength(clips);
  return (
    <motion.div role="region" aria-label={t("Bande de montage")} initial={{ opacity: 0, y: 24 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 24 }} transition={SPRING.smooth}
      className="tone-orange fixed bottom-5 left-1/2 z-[7] flex w-[min(64rem,calc(100%-3rem))] -translate-x-1/2 flex-col gap-3 rounded-2xl bg-white p-4 shadow-lg">
      <div className="flex flex-wrap items-center gap-3">
        <span className="grid h-9 w-9 place-items-center rounded-full bg-[color-mix(in_srgb,var(--tone)_18%,var(--ibox-white))] text-[var(--tone-ink)]"><Icon name="copy" size="1.1em" /></span>
        <div className="min-w-0 flex-1">
          <div className="text-[0.98rem] font-extrabold text-ink">{t("Bande de montage")}</div>
          <div className="text-[0.8rem] font-semibold text-ink-soft">
            {clips.length < 2 ? t("Touchez les clips dans l’ordre voulu ; glissez les vignettes pour changer l’ordre.") : total.seconds > 0 ? t("{0} clips · {1} au total", clips.length, `${total.exact ? "" : "≈ "}${duration(total.seconds)}`) : t("{0} clips", clips.length)}
          </div>
        </div>
        <Button tone="orange" variant="primary" icon="copy" disabled={clips.length < 2 || busy} onClick={onJoin}>{t("Assembler")}</Button>
        <Button tone="red" variant="ghost" onClick={onCancel}>{t("Annuler")}</Button>
      </div>
      {clips.length > 0 ? (
        <Reorder.Group axis="x" values={clips.map((c) => c.name)} onReorder={onReorder} layoutScroll className="flex gap-2.5 overflow-x-auto pb-1">
          {clips.map((c, i) => (
            <Reorder.Item key={c.name} value={c.name} whileDrag={{ scale: 1.06, zIndex: 2, boxShadow: "0 16px 30px -12px rgba(0,0,0,0.45)" }} transition={SPRING.snappy}
              className="group/strip relative aspect-video w-40 shrink-0 cursor-grab touch-none overflow-hidden rounded-lg bg-cream-deep select-none active:cursor-grabbing">
              {thumbUrl(c) ? <img src={thumbUrl(c)} alt="" draggable={false} className="pointer-events-none h-full w-full object-cover" />
                : c.src ? <video src={`${clipUrl(c)}#t=0.5`} preload="metadata" muted playsInline tabIndex={-1} className="pointer-events-none h-full w-full object-cover" />
                : null}
              <span className="absolute inset-x-0 bottom-0 truncate bg-gradient-to-t from-black/70 to-transparent px-2 pt-4 pb-1 text-[0.72rem] font-bold text-[#fff]">{label(c)}</span>
              <span className="absolute top-1.5 left-1.5 grid h-6 min-w-6 place-items-center rounded-full bg-[var(--tone)] px-1.5 text-[0.78rem] font-extrabold text-[#fff] shadow">{i + 1}</span>
              {c.duration ? <span className="absolute top-1.5 right-8 rounded-full bg-black/60 px-1.5 text-[0.72rem] font-extrabold text-[#fff]">{duration(c.duration)}</span> : null}
              <button type="button" aria-label={t("Retirer {0}", label(c))} title={t("Retirer")} onPointerDown={(e) => e.stopPropagation()} onClick={() => onRemove(c.name)}
                className="absolute top-1.5 right-1.5 grid h-6 w-6 place-items-center rounded-full bg-black/60 text-[#fff] opacity-0 transition-opacity group-hover/strip:opacity-100 hover:bg-[var(--tone-red)] focus-visible:opacity-100">
                <Icon name="x" size="0.9em" />
              </button>
            </Reorder.Item>
          ))}
        </Reorder.Group>
      ) : null}
    </motion.div>
  );
}

type PlayerProps = {
  clip: Clip; queue: Clip[]; onGo: (name: string) => void; speed: number; onSpeed: (s: number) => void;
  /** Menu d'un clip de la liste (clic droit), le même que dans la galerie. */
  onMenu: (name: string, x: number, y: number, from: HTMLElement) => void;
  startMode: "cut" | "vertical" | null; onSettings: () => void;
  confirm: boolean; onClose: () => void; onTitle: (t: string) => void; onFavorite: () => void; onCopy: () => void; onDelete: () => void; onCancel: () => void;
  busy: boolean; keys: boolean; sharing: boolean; setSharing: (v: boolean) => void;
  onRender: (kind: "mp4" | "gif" | "vertical", span: Span, focus?: number, replace?: boolean) => void; onMix: (game: number, mic: number) => void;
};

/** Lecteur de studio : il prend toute la fenêtre ; le contenu change de clip sans refaire l'entrée. */
function Player(props: PlayerProps) {
  return (
    <motion.div data-nav-scope className="fixed inset-0 z-10 flex flex-col bg-cream" initial={{ opacity: 0, scale: 0.985 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.99, transition: { duration: 0.14 } }} transition={SPRING.smooth}>
      <div className="ibox-rainbow h-1 w-full shrink-0 opacity-80" aria-hidden="true" />
      <PlayerBody key={props.clip.name} {...props} />
    </motion.div>
  );
}

function PlayerBody({ clip: c, queue, onGo, onMenu, speed, onSpeed, confirm, onClose, onTitle, onFavorite, onCopy, onDelete, onCancel, busy, keys, sharing, setSharing, onRender, onMix, startMode, onSettings }: PlayerProps) {
  const [title, setTitle] = useState(c.title ?? "");
  const [failed, setFailed] = useState(false);
  /** Couper, ou format vertical (cadre 9:16 à placer sur l'image). */
  const [mode, setMode] = useState<"cut" | "vertical" | "sound" | null>(isImage(c) ? null : startMode);
  /** Outil déplié : le masque de l'animation tombe (les bulles dépassent du panneau). */
  const [focus, setFocus] = useState(0.5);
  /** Vidéo déjà verticale (ou carrée) : pas de format vertical à en tirer. */
  const [portrait, setPortrait] = useState(false);
  const [length, setLength] = useState(c.duration ?? 0);
  const video = useRef<HTMLVideoElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  // le son du clip rejoué ne doit pas déclencher « Clip ça ! » (quiet.ts)
  useQuietVoice(video, failed);
  // volumes du panneau Son, entendus pendant la lecture (mix.ts)
  const [mix, setMix] = useState({ game: 1, mic: 1 });
  useLiveMix(video, isImage(c) || failed ? "" : clipUrl(c), c.tracks, mix);
  const [note, setNote] = useState<string | null>(null);
  useEffect(() => { if (!note) return; const id = setTimeout(() => setNote(null), 3000); return () => clearTimeout(id); }, [note]);
  // vitesse gardée d'un clip à l'autre
  useEffect(() => { const v = video.current; if (v) { v.defaultPlaybackRate = speed; v.playbackRate = speed; } }, [speed]);
  const index = queue.findIndex((x) => x.name === c.name);
  const prev = index > 0 ? queue[index - 1] : null;
  const next = index >= 0 && index < queue.length - 1 ? queue[index + 1] : null;
  const toggle = (m: "cut" | "vertical" | "sound") => setMode((cur) => (cur === m ? null : m));
  const media = !isImage(c);
  // lecteur : Espace, K, J et L, virgule et point, - et +, Entrée, Page préc. et suiv., C, V, S, P, F, Suppr, Ctrl+C (I, O, Z : dans Couper)
  useKeys(keys && !sharing, (k) => {
    const v = video.current;
    switch (k) {
      case "space": case "k": playPause(v); return true;
      case "j": seekBy(v, -SKIP); return true;
      case "l": seekBy(v, SKIP); return true;
      case ",": frameStep(v, -1); return true;
      case ".": frameStep(v, 1); return true;
      case "-": onSpeed(nextSpeed(speed, -1)); return true;
      case "+": case "=": onSpeed(nextSpeed(speed, 1)); return true;
      case "PageUp": if (prev) onGo(prev.name); return true;
      case "PageDown": if (next) onGo(next.name); return true;
      case "Enter": {
        // Entrée garde son rôle sur un bouton ou un champ ; ailleurs, plein écran
        const el = document.activeElement;
        if (el && el !== document.body && el !== stage.current) return false;
        if (media) toggleFullscreen(stage.current);
        return true;
      }
      case "c": if (media) toggle("cut"); return true;
      case "v": if (media && !portrait) toggle("vertical"); return true;
      case "s": if (media) toggle("sound"); return true;
      case "p": setSharing(true); return true;
      case "f": onFavorite(); return true;
      case "Delete": onDelete(); return true;
      case "ctrl+c": onCopy(); return true;
    }
    return false;
  });
  // le clip arrive avec le focus sur l'image (A ou Entrée sur l'image : lecture, plein écran)
  useEffect(() => { stage.current?.focus({ preventScroll: true }); }, []);
  // plein écran fermé en quittant le clip
  useEffect(() => () => exitFullscreen(), []);
  const commit = () => { if (title.trim() !== (c.title ?? "")) onTitle(title); };
  const src = clipUrl(c);
  const tools: { id: "cut" | "vertical" | "sound"; icon: IconName; label: string; key: string; tone: Tone }[] = media
    ? [
        { id: "cut", icon: "cut", label: t("Couper"), key: "C", tone: "orange" },
        ...(!portrait ? [{ id: "vertical" as const, icon: "phone" as const, label: t("Vertical"), key: "V", tone: "pink" as const }] : []),
        { id: "sound", icon: "sound", label: t("Son"), key: "S", tone: "orange" },
      ]
    : [];
  return (
    <div className="flex min-h-0 flex-1 flex-col" role="dialog" aria-modal="true" aria-label={label(c)}>
      <header className="flex flex-wrap items-center gap-3 px-6 pt-4 pb-3">
        <Button tone="blue" icon="back" variant="ghost" title={hint(t("Retour"), t("Échap"))} onClick={onClose}>{t("Retour")}</Button>
        <input
          data-nav
          value={title}
          maxLength={80}
          placeholder={c.game ?? t("Sans titre")}
          aria-label={t("Titre du clip")}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => { if (e.key === "Enter") { e.currentTarget.blur(); } }}
          className="h-11 min-w-[12rem] flex-1 rounded-full bg-white px-5 text-[1rem] font-bold text-ink shadow-sm outline-none placeholder:text-ink-faint focus-visible:ring-4 focus-visible:ring-accent/30"
        />
        {queue.length > 1 && (
          <div className="flex items-center gap-1 rounded-full bg-white p-1 shadow-sm">
            <Button tone="blue" variant="ghost" icon="up" disabled={!prev} title={hint(t("Clip précédent"), t("Page préc."))} aria-label={t("Clip précédent")} onClick={() => prev && onGo(prev.name)} className="h-9 px-2.5" />
            <span className="min-w-[3.6rem] text-center text-[0.85rem] font-extrabold text-ink-soft tabular-nums">{index + 1} / {queue.length}</span>
            <Button tone="blue" variant="ghost" icon="down" disabled={!next} title={hint(t("Clip suivant"), t("Page suiv."))} aria-label={t("Clip suivant")} onClick={() => next && onGo(next.name)} className="h-9 px-2.5" />
          </div>
        )}
        <Button tone="yellow" icon="star" title={hint(c.favorite ? t("Retirer des favoris") : t("Ajouter aux favoris"), "F")} aria-label={c.favorite ? t("Retirer des favoris") : t("Ajouter aux favoris")} variant={c.favorite ? "primary" : "soft"} filled={c.favorite} onClick={onFavorite} aria-pressed={c.favorite} className="px-3.5" />
        <Button tone="blue" icon="copy" title={hint(t("Copier le clip"), "Ctrl+C")} aria-label={t("Copier le clip")} onClick={onCopy} className="px-3.5" />
        <Button tone="green" variant="primary" icon="share" title={hint(t("Partager"), "P")} onClick={() => setSharing(true)}>{t("Partager")}</Button>
        {confirm ? (
          <>
            <span className="text-[0.9rem] font-bold text-ink">{t("Supprimer ce clip ? Il est effacé de ce PC.")}</span>
            <Button variant="danger" icon="trash" onClick={onDelete} autoFocus>{t("Supprimer")}</Button>
            <Button tone="blue" variant="ghost" onClick={onCancel}>{t("Annuler")}</Button>
          </>
        ) : (
          <Button tone="red" icon="trash" title={hint(t("Supprimer"), t("Suppr"))} aria-label={t("Supprimer")} onClick={onDelete} className="px-3.5" />
        )}
      </header>

      <div className="flex min-h-0 flex-1 gap-5 px-6 pb-5">
        <div className="flex min-w-0 flex-1 flex-col gap-3">
          {/* l'image : elle prend toute la place laissée par les outils */}
          <div ref={stage} data-nav tabIndex={0} aria-label={t("Lecture du clip")}
            onClick={(e) => { if (mode !== "vertical" && (e.target === e.currentTarget || e.target === video.current)) playPause(video.current); }}
            onDoubleClick={(e) => { if (media && (e.target === e.currentTarget || e.target === video.current)) toggleFullscreen(stage.current); }}
            className="group/player relative min-h-[12rem] flex-1 overflow-hidden rounded-xl bg-black shadow-md outline-none focus-visible:ring-4 focus-visible:ring-accent/40">
            {src && isImage(c) ? (
              <img src={src} alt="" className="absolute inset-0 h-full w-full object-contain" />
            ) : src && !failed ? (
              <>
                <video ref={video} src={src} autoPlay playsInline draggable onDragStart={(e) => dragOut(e, c)} onError={() => setFailed(true)}
                  onLoadedMetadata={(e) => { const v = e.currentTarget; v.playbackRate = speed; setLength(v.duration); setPortrait(v.videoWidth > 0 && v.videoWidth * 16 <= v.videoHeight * 9 + 16); }}
                  className={`absolute inset-0 h-full w-full bg-black object-contain ${mode === "vertical" ? "" : "cursor-pointer"}`} />
                {mode === "vertical" ? <Vertical video={video} focus={focus} onFocus={setFocus} /> : <Playback video={video} length={length} bar={mode !== "cut"} speed={speed} onSpeed={onSpeed} stage={stage} />}
              </>
            ) : (
              <div className="absolute inset-0 grid place-items-center text-[0.95rem] font-bold text-[#fff]/70">{src ? t("Lecture impossible.") : c.game ?? ""}</div>
            )}
          </div>

          {/* outils : onglets sous l'image, l'outil choisi se déplie sur un ressort */}
          <div className="flex flex-wrap items-center gap-3">
            {tools.length > 0 && (
              <div role="tablist" aria-label={t("Outils")} className="flex rounded-full bg-white p-1 shadow-sm">
                {tools.map((tool) => (
                  <button key={tool.id} type="button" role="tab" data-nav aria-selected={mode === tool.id} title={hint(tool.label, tool.key)} onClick={() => toggle(tool.id)}
                    className={`tone-${tool.tone} relative inline-flex h-9 items-center gap-2 rounded-full px-4 text-[0.9rem] font-bold transition-colors ${mode === tool.id ? "text-[var(--tone-on)]" : "text-ink-soft hover:text-[var(--tone-ink)]"}`}>
                    {mode === tool.id && <motion.span layoutId="tool-pill" className="absolute inset-0 rounded-full bg-[var(--tone)] shadow-[0_6px_16px_-8px_var(--tone)]" transition={SPRING.snappy} />}
                    <span className="relative"><Icon name={tool.icon} size="1.05em" /></span>
                    <span className="relative">{tool.label}</span>
                  </button>
                ))}
              </div>
            )}
            <span className="min-w-0 flex-1 truncate text-[0.82rem] font-semibold text-ink-soft">{[c.game, when(c.at), length > 0 && media ? duration(length) : null, size(c.size), c.auto ? autoLabel(c.auto) : null].filter(Boolean).join(" · ")}</span>
            <AnimatePresence>{note && <motion.span role="status" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={SPRING.snappy} className="text-[0.82rem] font-bold text-accent-text">{note}</motion.span>}</AnimatePresence>
          </div>
          {/* zone des outils : toujours là et de hauteur fixe, l'image ne bouge pas quand on change
              d'outil ; seul le contenu passe de l'un à l'autre en fondu */}
          {media && (
            <div className="relative shrink-0 rounded-xl bg-white shadow-sm" style={{ minHeight: TOOL_HEIGHT }}>
              <AnimatePresence initial={false} mode="popLayout">
                <motion.div key={mode ?? "none"} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.12, ease: "easeOut" }}>
                  {mode === "sound" ? (
                    <SoundMix clip={c} game={mix.game} mic={mix.mic} onChange={setMix} busy={busy} onMix={onMix} onSettings={onSettings} />
                  ) : mode ? (
                    <Trim video={video} src={src} length={length} busy={busy} vertical={mode === "vertical"} onRender={(kind, span, replace) => onRender(kind, span, focus, replace)} />
                  ) : (
                    <div className="grid place-items-center px-6 text-center" style={{ minHeight: TOOL_HEIGHT }}>
                      <p className="max-w-[34rem] text-[0.88rem] font-semibold text-ink-faint">{t("Choisissez un outil : Couper (C) pour garder un passage ou en faire un GIF, Vertical (V) pour TikTok et Shorts, Son (S) pour régler le jeu et le micro.")}</p>
                    </div>
                  )}
                </motion.div>
              </AnimatePresence>
            </div>
          )}
        </div>

        {queue.length > 1 && <Queue queue={queue} current={c.name} onGo={onGo} onMenu={onMenu} />}
      </div>
      <AnimatePresence>
        {sharing && <ShareSheet clip={c} onClose={() => setSharing(false)} onVertical={isImage(c) || portrait ? null : () => { setSharing(false); setMode("vertical"); }} />}
      </AnimatePresence>
    </div>
  );
}

/** Liste des clips à droite du lecteur : le clip vu en avant, un clic pour passer à un autre. */
function Queue({ queue, current, onGo, onMenu }: { queue: Clip[]; current: string; onGo: (name: string) => void; onMenu: (name: string, x: number, y: number, from: HTMLElement) => void }) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.querySelector<HTMLElement>("[aria-current=true]")?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [current]);
  return (
    <aside aria-label={t("Clips")} className="hidden w-[19rem] shrink-0 flex-col overflow-hidden rounded-xl bg-white shadow-sm xl:flex">
      <h2 className="px-4 pt-3 pb-2 text-[0.78rem] font-extrabold tracking-[0.12em] text-ink-faint uppercase">{t("Clips")} · {queue.length}</h2>
      <div ref={list} className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-2 pb-2">
        {queue.map((c) => {
          const on = c.name === current;
          const pic = isImage(c) ? clipUrl(c) : thumbUrl(c);
          return (
            <button key={c.name} type="button" data-nav aria-current={on} aria-haspopup="menu" onClick={() => !on && onGo(c.name)} {...menuHandlers((x, y, from) => onMenu(c.name, x, y, from))}
              className={`group/q relative flex items-center gap-3 rounded-lg p-1.5 text-left transition-colors ${on ? "bg-accent-soft" : "hover:bg-cream-deep/70"}`}>
              {on && <motion.span layoutId="queue-on" className="absolute inset-y-2 left-0 w-1 rounded-full bg-accent" transition={SPRING.snappy} />}
              <span className="relative aspect-video w-24 shrink-0 overflow-hidden rounded-md bg-cream-deep">
                {pic ? <img src={pic} alt="" draggable={false} className="h-full w-full object-cover" />
                  : c.src && <video src={`${clipUrl(c)}#t=0.5`} preload="metadata" muted playsInline tabIndex={-1} className="h-full w-full object-cover" />}
                {c.duration ? <span className="absolute right-1 bottom-1 rounded-full bg-black/60 px-1.5 text-[0.66rem] font-extrabold text-[#fff]">{duration(c.duration)}</span> : null}
              </span>
              <span className="min-w-0 flex-1">
                <span className={`block truncate text-[0.86rem] font-bold ${on ? "text-accent-text" : "text-ink"}`}>{label(c)}</span>
                <span className="block truncate text-[0.74rem] font-semibold text-ink-soft">{when(c.at)}</span>
              </span>
              {c.favorite && <span className="tone-yellow shrink-0 pr-1 text-[var(--tone)]"><Icon name="star" filled size="0.95em" /></span>}
            </button>
          );
        })}
      </div>
    </aside>
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
