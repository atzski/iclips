/**
 * Panneau en jeu d'iClips : par-dessus la partie (raccourci de l'overlay d'iBox, Alt+V, ou
 * Vue + X maintenus), on garde les dernières secondes, on nomme le clip, on l'étoile, on garde
 * l'image du jeu figée à l'ouverture (capture d'écran), et l'on file au Studio ou à la
 * Communauté. Avec iClips, ce panneau remplace l'overlay de capture d'iBox (0.6). La fenêtre
 * couvre l'écran, transparente ; le panneau glisse depuis le bord. Elle est exclue des captures :
 * le panneau n'apparaît jamais dans un clip.
 *
 * La manette arrive du socle (événement `pad`, XInput natif) ; le clavier marche aussi.
 */
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { clipUrl, host, onHost, shotUrl, type ClipStatus } from "./host";
import { playClip } from "./shutter";
import { t } from "./i18n";
import { SPRING } from "./ibox/motion";
import { padAction, useNav } from "./nav";
import { Button, Icon } from "./ui";
import logo from "./logo.png";

const params = new URLSearchParams(location.search);
const side = params.get("side") === "left" ? "left" : "right";
const width = Math.min(900, Math.max(240, Number(params.get("width")) || 480));

interface Saved { name: string; game: string | null; src: string; title: string; favorite: boolean }

export function Panel() {
  const [open, setOpen] = useState(true);
  const [status, setStatus] = useState<ClipStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<Saved | null>(null);
  const [error, setError] = useState<string | null>(null);
  const keep = useRef<HTMLButtonElement>(null);
  /** Ouverture en cours (l'image figée change à chaque fois). */
  const [stamp, setStamp] = useState(() => Date.now());
  const [shotKept, setShotKept] = useState(false);

  const refresh = useCallback(() => {
    host<ClipStatus>("clips.status").then(setStatus).catch((e: Error) => setError(e.message));
  }, []);
  const close = useCallback(() => setOpen(false), []);

  useEffect(() => {
    refresh();
    return onHost((event, payload) => {
      if (event === "panel://open") { setSaved(null); setShotKept(false); setStamp(Date.now()); setError(null); setOpen(true); refresh(); }
      else if (event === "panel://close-request") setOpen(false);
      else if (event === "voice://state") refresh();
      else if (event === "pad" && typeof payload === "string") padAction(payload, close);
    });
  }, [refresh, close]);
  useNav(close);
  // la mémoire se remplit pendant que le panneau est ouvert : on relit chaque seconde
  useEffect(() => {
    if (!open) return;
    const id = setInterval(refresh, 1000);
    return () => clearInterval(id);
  }, [open, refresh]);
  useEffect(() => { if (open) setTimeout(() => keep.current?.focus(), 60); }, [open]);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const c = await host<{ name: string; game: string | null; src: string }>("clips.save");
      playClip();
      setSaved({ ...c, title: "", favorite: false });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const saveShot = async () => {
    try {
      await host("shot.save");
      playClip();
      setShotKept(true);
      refresh();
    } catch (e) {
      setError((e as Error).message);
    }
  };
  const update = (patch: Partial<Pick<Saved, "title" | "favorite">>) => {
    if (!saved) return;
    setSaved({ ...saved, ...patch });
    host("clips.update", { name: saved.name, ...patch }).catch((e: Error) => setError(e.message));
  };

  const seconds = status ? Math.min(status.seconds, status.max) : 0;
  const why = !status ? null
    : !status.enabled ? t("Les clips sont désactivés dans Paramètres › Captures.")
    : !status.game ? t("Les clips se gardent pendant une partie lancée depuis iBox.")
    : seconds === 0 ? t("La mémoire du clip se remplit…")
    : null;

  return (
    <div data-nav-scope className="fixed inset-0" onClick={(e) => e.target === e.currentTarget && close()}>
      <AnimatePresence onExitComplete={() => void host("overlay.close")}>
        {open && (
          <motion.aside
            key="panel"
            role="dialog"
            aria-label="iClips"
            initial={{ x: side === "right" ? 40 : -40, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            exit={{ x: side === "right" ? 40 : -40, opacity: 0 }}
            transition={SPRING.smooth}
            style={{ width }}
            className={`absolute top-1/2 -translate-y-1/2 ${side === "right" ? "right-8" : "left-8"} flex max-h-[calc(100%-4rem)] flex-col gap-4 overflow-y-auto rounded-xl bg-white p-6 shadow-lg`}
          >
            <header className="flex items-center gap-3">
              <img src={logo} alt="" className="h-11 w-11" />
              <div className="min-w-0 flex-1">
                <div className="font-display text-[1.25rem] leading-tight text-ink">iClips</div>
                <div className="truncate text-[0.85rem] font-semibold text-ink-soft">{status?.game ?? t("Hors partie")}</div>
              </div>
              {status?.listening && (
                <span role="status" className="inline-flex items-center gap-1.5 rounded-full bg-accent-soft px-3 py-1 text-[0.8rem] font-extrabold text-accent-text">
                  <Icon name="mic" size="1.1em" />
                  {t("Le micro écoute")}
                </span>
              )}
              <Button variant="ghost" icon="x" aria-label={t("Fermer")} onClick={close} className="px-3" />
            </header>

            {saved ? (
              <section className="flex flex-col gap-3">
                <div className="flex items-center gap-2 text-[0.95rem] font-bold text-accent-text"><Icon name="check" /> {t("Clip gardé")}</div>
                {clipUrl(saved) && (
                  // aperçu sans le son, en boucle : rien ne se superpose au jeu
                  <video src={clipUrl(saved)} muted autoPlay loop playsInline className="aspect-video w-full rounded-lg bg-black object-cover" />
                )}
                <input
                  data-nav
                  value={saved.title}
                  maxLength={80}
                  placeholder={saved.game ?? t("Sans titre")}
                  aria-label={t("Titre du clip")}
                  onChange={(e) => setSaved({ ...saved, title: e.target.value })}
                  onBlur={() => update({ title: saved.title })}
                  onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); }}
                  className="h-11 rounded-full bg-cream-deep px-5 text-[0.95rem] font-bold text-ink outline-none placeholder:text-ink-faint"
                />
                <div className="flex flex-wrap gap-2">
                  <Button icon="star" filled={saved.favorite} aria-pressed={saved.favorite} onClick={() => update({ favorite: !saved.favorite })}>
                    {saved.favorite ? t("Retirer des favoris") : t("Ajouter aux favoris")}
                  </Button>
                  <Button ref={keep} variant="primary" icon="play" onClick={() => void host("screen.open")}>{t("Ouvrir dans le Studio")}</Button>
                </div>
              </section>
            ) : (
              <section className="flex flex-col gap-3">
                <Button ref={keep} variant="primary" icon="play" disabled={busy || !!why} onClick={() => void save()} className="h-14 text-[1.05rem]">
                  {busy ? t("Enregistrement…") : seconds > 0 ? t("Garder les {0} dernières secondes", seconds) : t("Garder le clip")}
                </Button>
                {why && <p className="text-[0.85rem] font-semibold text-ink-soft">{why}</p>}
              </section>
            )}

            {(status?.hasShot || shotKept) && (
              // image du jeu figée à l'ouverture : la garder en capture d'écran
              <section className="flex items-center gap-3 rounded-lg bg-cream-deep/60 p-2 pr-3">
                <img src={shotUrl(stamp)} alt="" className="aspect-video w-36 shrink-0 rounded-md bg-black object-cover" />
                {shotKept ? (
                  <span className="flex flex-1 items-center gap-2 text-[0.9rem] font-bold text-accent-text"><Icon name="check" />{t("Capture gardée")}</span>
                ) : (
                  <Button icon="camera" className="flex-1" onClick={() => void saveShot()}>{t("Garder la capture")}</Button>
                )}
              </section>
            )}
            <div className="grid grid-cols-2 gap-2">
              {!saved && <Button variant="soft" icon="play" onClick={() => void host("screen.open")}>{t("Ouvrir le Studio")}</Button>}
              <Button variant="soft" icon="globe" className={saved ? "col-span-2" : ""} onClick={() => void host("community.open")}>{t("Communauté")}</Button>
            </div>

            {error && <p role="alert" className="text-[0.85rem] font-bold text-peach-deep">{error}</p>}
            <p className="text-[0.75rem] font-semibold text-ink-faint">{t("Alt+X, Alt+V ou Vue + X : ouvrir et fermer")}</p>
          </motion.aside>
        )}
      </AnimatePresence>
    </div>
  );
}
