/**
 * iClips Studio : la galerie des clips du profil iBox (vignettes, filtre par jeu et favoris),
 * un lecteur, le titre et le favori de chaque clip, la suppression, et le montage (0.3) :
 * couper un passage, en faire un GIF, assembler plusieurs clips. Clips automatiques (0.4) :
 * réglages dans `Auto.tsx`, marque sur les cartes. Le partage vient ensuite (docs/CONCEPTION.md).
 */
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";
import { clipUrl, host, onHost, thumbUrl, type Clip } from "./host";
import { makeThumb } from "./thumbs";
import { joinSegments, useRender, type Span } from "./edit";
import { Trim } from "./Trim";
import { AutoSheet } from "./Auto";
import { duration, size, when } from "./format";
import { t } from "./i18n";
import { useNav } from "./nav";
import { Button, Icon } from "./ui";
import logo from "./logo.png";

type Filter = "all" | "favorites";

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
  const [auto, setAuto] = useState(false);

  const load = useCallback(() => {
    host<Clip[]>("clips.list").then(setClips).catch((e: Error) => { setClips([]); setError(t("Action impossible : {0}", e.message)); });
  }, []);
  useEffect(() => { load(); return onHost((event) => { if (event === "clips://changed") load(); }); }, [load]);
  useEffect(() => { if (!error) return; const id = setTimeout(() => setError(null), 4000); return () => clearTimeout(id); }, [error]);

  const games = useMemo(() => [...new Set((clips ?? []).map((c) => c.game ?? ""))].sort((a, b) => a.localeCompare(b)), [clips]);
  const shown = useMemo(() => (clips ?? []).filter((c) => (filter === "all" || c.favorite) && (game === null || (c.game ?? "") === game)), [clips, filter, game]);

  // vignettes : une à la fois, pour les clips qui n'en ont pas encore
  const thumbing = useRef(false);
  useEffect(() => {
    const todo = (clips ?? []).filter((c) => c.src && !c.thumb && c.kind !== "gif");
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
  const trim = (c: Clip, kind: "mp4" | "gif", span: Span) =>
    kind === "mp4"
      ? startRender({ kind, segments: [{ name: c.name, ...span }], title: c.title ? t("{0} (coupé)", c.title) : undefined }, t("Coupe du clip…"))
      : startRender({ kind, name: c.name, ...span }, t("Création du GIF…"));
  const join = () => {
    const list = (picked ?? []).map((n) => clips?.find((c) => c.name === n)).filter((c): c is Clip => !!c);
    startRender({ kind: "mp4", segments: joinSegments(list) }, t("Assemblage de {0} clips…", list.length));
  };

  const act = async (call: string, arg: Record<string, unknown>) => {
    try { await host(call, arg); load(); } catch (e) { setError(t("Action impossible : {0}", (e as Error).message)); }
  };

  useNav(() => {
    if (auto) setAuto(false);
    else if (confirm) setConfirm(false);
    else if (open) setOpen(null);
    else if (picked) setPicked(null);
    else void host("screen.close");
  });

  // la manette arrive sur la première carte
  useEffect(() => {
    if (clips && !open && !document.activeElement?.hasAttribute("data-nav")) document.querySelector<HTMLElement>("[data-card]")?.focus();
  }, [clips, open]);

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-4 px-8 pt-6 pb-4">
        <img src={logo} alt="" className="h-12 w-12" />
        <div className="min-w-0 flex-1">
          <h1 className="font-display text-[1.6rem] leading-tight text-ink">{t("iClips Studio")}</h1>
          {clips && <p className="text-[0.85rem] font-semibold text-ink-soft">{clips.length === 1 ? t("1 clip") : t("{0} clips", clips.length)}</p>}
        </div>
        <div role="tablist" className="flex rounded-full bg-white p-1 shadow-sm">
          {(["all", "favorites"] as const).map((f) => (
            <button
              key={f}
              role="tab"
              data-nav
              aria-selected={filter === f}
              onClick={() => setFilter(f)}
              className={`h-9 rounded-full px-4 text-[0.9rem] font-bold transition ${filter === f ? "bg-accent text-on-accent" : "text-ink-soft hover:text-ink"}`}
            >
              {f === "all" ? t("Tous") : t("Favoris")}
            </button>
          ))}
        </div>
        <Button variant={picked ? "primary" : "soft"} aria-pressed={!!picked} onClick={() => setPicked((p) => (p ? null : []))}>{t("Assembler")}</Button>
        <Button icon="auto" onClick={() => setAuto(true)}>{t("Automatique")}</Button>
        <Button variant="ghost" icon="x" onClick={() => void host("screen.close")}>{t("Fermer")}</Button>
      </header>

      {games.length > 1 && (
        <nav aria-label={t("Tous les jeux")} className="flex gap-2 overflow-x-auto px-8 pb-4">
          {[null, ...games].map((g) => (
            <button
              key={g ?? "*"}
              data-nav
              aria-pressed={game === g}
              onClick={() => setGame(g)}
              className={`h-9 shrink-0 rounded-full px-4 text-[0.88rem] font-bold transition ${game === g ? "bg-accent-soft text-accent-text" : "bg-white text-ink-soft shadow-sm hover:text-ink"}`}
            >
              {g === null ? t("Tous les jeux") : g || t("Hors jeu")}
            </button>
          ))}
        </nav>
      )}

      <main className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
        {clips && shown.length === 0 ? (
          <Empty favorites={filter === "favorites" && clips.length > 0} />
        ) : (
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(17rem,1fr))] gap-5">
            {shown.map((c, i) => (
              <motion.li key={c.name} initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ ...SPRING.smooth, delay: Math.min(i, 12) * 0.04 }}>
                <Card
                  clip={c}
                  order={picked ? picked.indexOf(c.name) + 1 : 0}
                  onOpen={() => (picked ? c.kind !== "gif" && setPicked((p) => p && (p.includes(c.name) ? p.filter((n) => n !== c.name) : [...p, c.name])) : setOpen(c.name))}
                />
              </motion.li>
            ))}
          </ul>
        )}
      </main>

      <AnimatePresence>
        {current && (
          <Player
            key={current.name}
            clip={current}
            confirm={confirm}
            onClose={() => { setConfirm(false); setOpen(null); }}
            onTitle={(title) => void act("clips.update", { name: current.name, title })}
            onFavorite={() => void act("clips.update", { name: current.name, favorite: !current.favorite })}
            onDelete={() => (confirm ? (setConfirm(false), setOpen(null), void act("clips.delete", { name: current.name })) : setConfirm(true))}
            onCancel={() => setConfirm(false)}
            busy={!!job}
            onRender={(kind, span) => trim(current, kind, span)}
          />
        )}
      </AnimatePresence>

      <AnimatePresence>{auto && <AutoSheet onClose={() => setAuto(false)} />}</AnimatePresence>

      <AnimatePresence>
        {picked && !open && (
          <motion.div data-nav-scope initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.smooth} className="fixed bottom-6 left-1/2 flex -translate-x-1/2 items-center gap-3 rounded-full bg-white py-2 pr-2 pl-6 shadow-lg">
            <span className="text-[0.92rem] font-bold text-ink">{picked.length < 2 ? t("Choisissez les clips dans l’ordre voulu") : t("{0} clips choisis", picked.length)}</span>
            <Button variant="primary" disabled={picked.length < 2 || !!job} onClick={join}>{t("Assembler")}</Button>
            <Button variant="ghost" onClick={() => setPicked(null)}>{t("Annuler")}</Button>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence>
        {job && (
          <motion.div role="status" initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 16 }} transition={SPRING.smooth} className="fixed top-6 left-1/2 z-20 w-[22rem] -translate-x-1/2 rounded-xl bg-white px-5 py-3 shadow-lg">
            <div className="flex justify-between text-[0.88rem] font-bold text-ink"><span>{job.label}</span><span>{Math.round(job.ratio * 100)} %</span></div>
            <div className="mt-2 h-2 overflow-hidden rounded-full bg-cream-deep">
              <motion.div className="h-full rounded-full bg-accent" animate={{ width: `${job.ratio * 100}%` }} transition={SPRING.smooth} />
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
    </div>
  );
}

function label(c: Clip) {
  return c.title ?? c.game ?? t("Hors jeu");
}

function autoLabel(how: NonNullable<Clip["auto"]>) {
  return how === "voice" ? t("Gardé à la voix") : how === "pad" ? t("Gardé à la manette") : t("Gardé en fin de partie");
}

function Card({ clip: c, order, onOpen }: { clip: Clip; order: number; onOpen: () => void }) {
  const [length, setLength] = useState(c.duration ?? 0);
  const src = clipUrl(c);
  const thumb = thumbUrl(c);
  return (
    <button type="button" data-nav data-card onClick={onOpen} className="group block w-full overflow-hidden rounded-xl bg-white text-left shadow-sm transition hover:-translate-y-0.5 hover:shadow-md">
      <div className="relative aspect-video bg-cream-deep">
        {/* première image du clip, sans le son : rien ne joue avant l'ouverture */}
        {c.kind === "gif" ? (
          src && <img src={src} alt="" className="absolute inset-0 h-full w-full object-cover" />
        ) : thumb ? (
          <img src={thumb} alt="" className="absolute inset-0 h-full w-full object-cover" />
        ) : (
          src && <video src={`${src}#t=0.5`} preload="metadata" muted playsInline tabIndex={-1} onLoadedMetadata={(e) => setLength(e.currentTarget.duration)} className="absolute inset-0 h-full w-full object-cover" />
        )}
        <span className="absolute inset-0 grid place-items-center opacity-0 transition group-hover:opacity-100 group-focus:opacity-100">
          <span className="grid h-14 w-14 place-items-center rounded-full bg-black/55 text-white"><Icon name="play" size="1.6em" /></span>
        </span>
        {length > 0 && <span className="absolute right-2 bottom-2 rounded-full bg-black/60 px-2 py-0.5 text-[0.75rem] font-extrabold text-white">{duration(length)}</span>}
        {c.favorite && <span className="absolute top-2 left-2 text-accent drop-shadow" aria-label={t("Favori")}><Icon name="star" filled /></span>}
        {c.auto && (
          <span className="absolute bottom-2 left-2 inline-flex items-center gap-1 rounded-full bg-black/60 px-2 py-0.5 text-[0.75rem] font-extrabold text-white" title={autoLabel(c.auto)}>
            <Icon name={c.auto === "voice" ? "mic" : c.auto === "pad" ? "pad" : "flag"} size="1.1em" />
            <span className="sr-only">{autoLabel(c.auto)}</span>
          </span>
        )}
        {order > 0 && <span className="absolute top-2 right-2 grid h-8 w-8 place-items-center rounded-full bg-accent text-[0.95rem] font-extrabold text-on-accent shadow-md">{order}</span>}
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

function Player({ clip: c, confirm, onClose, onTitle, onFavorite, onDelete, onCancel, busy, onRender }: {
  clip: Clip; confirm: boolean; onClose: () => void; onTitle: (t: string) => void; onFavorite: () => void; onDelete: () => void; onCancel: () => void;
  busy: boolean; onRender: (kind: "mp4" | "gif", span: Span) => void;
}) {
  const [title, setTitle] = useState(c.title ?? "");
  const [failed, setFailed] = useState(false);
  const [cutting, setCutting] = useState(false);
  const [length, setLength] = useState(c.duration ?? 0);
  const video = useRef<HTMLVideoElement>(null);
  const back = useRef<HTMLButtonElement>(null);
  useEffect(() => back.current?.focus(), []);
  const commit = () => { if (title.trim() !== (c.title ?? "")) onTitle(title); };
  const src = clipUrl(c);
  return (
    <motion.div data-nav-scope className="fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-8" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <motion.div role="dialog" aria-modal="true" aria-label={label(c)} className="flex max-h-full w-full max-w-[min(78rem,100%)] flex-col overflow-hidden rounded-xl bg-white shadow-lg" initial={{ scale: 0.96, y: 12 }} animate={{ scale: 1, y: 0 }} exit={{ scale: 0.97, y: 8 }} transition={SPRING.smooth}>
        <div className="relative min-h-0 flex-1 bg-black">
          {src && c.kind === "gif" ? (
            <img src={src} alt="" className="mx-auto max-h-[60vh] object-contain" />
          ) : src && !failed ? (
            <video ref={video} src={src} controls autoPlay playsInline onError={() => setFailed(true)} onLoadedMetadata={(e) => setLength(e.currentTarget.duration)} className="max-h-[60vh] w-full bg-black object-contain" />
          ) : (
            <div className="grid aspect-video place-items-center text-[0.95rem] font-bold text-white/70">{src ? t("Lecture impossible.") : c.game ?? ""}</div>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-3 px-6 py-4">
          <Button ref={back} icon="back" variant="ghost" onClick={onClose}>{t("Retour")}</Button>
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
          {c.kind !== "gif" && <Button aria-pressed={cutting} variant={cutting ? "primary" : "soft"} onClick={() => setCutting((v) => !v)}>{t("Couper")}</Button>}
          <Button icon="star" filled={c.favorite} onClick={onFavorite} aria-pressed={c.favorite}>{c.favorite ? t("Retirer des favoris") : t("Ajouter aux favoris")}</Button>
          {confirm ? (
            <>
              <span className="text-[0.9rem] font-bold text-ink">{t("Supprimer ce clip ? Il est effacé de ce PC.")}</span>
              <Button variant="danger" icon="trash" onClick={onDelete} autoFocus>{t("Supprimer")}</Button>
              <Button variant="ghost" onClick={onCancel}>{t("Annuler")}</Button>
            </>
          ) : (
            <Button icon="trash" onClick={onDelete}>{t("Supprimer")}</Button>
          )}
        </div>
        {cutting && <Trim video={video} src={src} length={length} busy={busy} onRender={onRender} />}
        <div className="px-6 pb-4 text-[0.8rem] font-semibold text-ink-soft">{[c.game, when(c.at), size(c.size), c.auto ? autoLabel(c.auto) : null].filter(Boolean).join(" · ")}</div>
      </motion.div>
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
          {favorites ? t("Marquez un clip d’une étoile pour le retrouver ici.") : t("Pendant une partie, ouvrez l’overlay d’iBox et gardez les dernières secondes : le clip arrive ici.")}
        </p>
      </div>
    </div>
  );
}
