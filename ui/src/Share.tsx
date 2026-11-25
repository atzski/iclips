/**
 * Partage d'un clip (0.5, permission `clips-share`) : tout part du PC, sans compte.
 * - glisser la vignette (ou une carte de la galerie) vers Discord, un dossier, un navigateur ;
 * - copier le clip, l'enregistrer ailleurs, ouvrir son dossier ;
 * - lien de 72 h chez Litterbox, copié tout de suite, à coller où l'on veut ou à envoyer à un
 *   ami iNetbox en message privé.
 * Le format vertical (TikTok, Shorts, Reels) se règle dans le lecteur (`Player`, mode Vertical).
 */
import { motion } from "motion/react";
import { useEffect, useRef, useState, type DragEvent } from "react";
import { SPRING } from "./ibox/motion";
import { clipUrl, host, onHost, thumbUrl, type Clip, type Friend } from "./host";
import { t } from "./i18n";
import { Button, Icon, type IconName } from "./ui";

/** Glisser un clip hors du Studio : iBox prend le relais (glisser-déposer de l'Explorateur). */
export function dragOut(e: DragEvent, c: Clip) {
  e.preventDefault();
  void host("share.drag", { name: c.name }).catch(() => undefined);
}

/** Lien encore bon (plus d'une heure devant lui). */
export function liveLink(c: Pick<Clip, "link" | "linkUntil">, now = Date.now()): string | null {
  return c.link && c.linkUntil && c.linkUntil > now + 3_600_000 ? c.link : null;
}

type Upload = { job: number; ratio: number } | null;

export function ShareSheet({ clip: c, onClose, onVertical }: { clip: Clip; onClose: () => void; onVertical: (() => void) | null }) {
  const [link, setLink] = useState<{ url: string; until: number } | null>(() => (liveLink(c) ? { url: c.link!, until: c.linkUntil! } : null));
  const [upload, setUpload] = useState<Upload>(null);
  const [note, setNote] = useState<{ text: string; bad?: boolean } | null>(null);
  const [friends, setFriends] = useState<Friend[] | null>(null);
  const [sent, setSent] = useState<string[]>([]);
  const back = useRef<HTMLButtonElement>(null);
  const job = useRef<number | null>(null);
  // une fois le lien prêt, l'ami à qui l'envoyer (choisi avant la fin de l'envoi)
  const pendingFriend = useRef<string | null>(null);

  useEffect(() => back.current?.focus(), []);
  useEffect(() => { if (!note) return; const id = setTimeout(() => setNote(null), 4500); return () => clearTimeout(id); }, [note]);
  useEffect(() => onHost((event, payload) => {
    const p = payload as { job?: number; ratio?: number; link?: string; until?: number; error?: string } | null;
    if (!p || p.job !== job.current) return;
    if (event === "share://progress") setUpload({ job: p.job, ratio: p.ratio ?? 0 });
    else if (event === "share://done") {
      job.current = null;
      setUpload(null);
      if (p.link && p.until) ready(p.link, p.until);
      else { pendingFriend.current = null; setNote({ text: t("Lien impossible : {0}", p.error ?? "?"), bad: true }); }
    }
  }), []); // eslint-disable-line react-hooks/exhaustive-deps

  const act = async (call: string, arg: Record<string, unknown>, done: string) => {
    try { await host(call, { name: c.name, ...arg }); setNote({ text: done }); } catch (e) { setNote({ text: (e as Error).message, bad: true }); }
  };
  const ready = (url: string, until: number) => {
    setLink({ url, until });
    void host("share.copyLink", { name: c.name }).then(() => setNote({ text: t("Lien copié : collez-le dans Discord, un message, un navigateur…") })).catch(() => undefined);
    const friend = pendingFriend.current;
    pendingFriend.current = null;
    if (friend) void sendTo(friend);
  };
  const makeLink = async () => {
    if (upload) return;
    try {
      const r = await host<{ job?: number; link?: string; until?: number }>("share.link", { name: c.name });
      if (r.link && r.until) ready(r.link, r.until);
      else if (r.job) { job.current = r.job; setUpload({ job: r.job, ratio: 0 }); }
    } catch (e) { setNote({ text: (e as Error).message, bad: true }); }
  };
  const sendTo = async (friend: string) => {
    try {
      await host("share.send", { name: c.name, friendId: friend });
      setSent((s) => [...s, friend]);
    } catch (e) { setNote({ text: (e as Error).message, bad: true }); }
  };
  const openFriends = () => {
    if (friends) { setFriends(null); return; }
    host<Friend[]>("share.friends").then(setFriends).catch((e: Error) => setNote({ text: e.message, bad: true }));
  };
  const toFriend = (id: string) => {
    if (link) void sendTo(id);
    else { pendingFriend.current = id; void makeLink(); }
  };

  const thumb = thumbUrl(c);
  const src = clipUrl(c);
  const hours = link ? Math.max(1, Math.round((link.until - Date.now()) / 3_600_000)) : 72;

  return (
    <motion.div data-nav-scope className="fixed inset-0 z-20 flex items-center justify-center bg-black/60 p-8" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <motion.div role="dialog" aria-modal="true" aria-label={t("Partager le clip")} className="flex max-h-full w-full max-w-[44rem] flex-col gap-4 overflow-y-auto rounded-xl bg-white p-6 shadow-lg" initial={{ scale: 0.96, y: 12 }} animate={{ scale: 1, y: 0 }} exit={{ scale: 0.97, y: 8 }} transition={SPRING.smooth}>
        <header className="flex items-center gap-3">
          <Button ref={back} icon="back" variant="ghost" onClick={onClose} aria-label={t("Retour")} className="px-3" />
          <h2 className="flex-1 font-display text-[1.35rem] leading-tight text-ink">{t("Partager le clip")}</h2>
        </header>

        {/* la vignette se glisse telle quelle hors du Studio */}
        <div draggable onDragStart={(e) => dragOut(e, c)} className="group flex cursor-grab items-center gap-4 rounded-lg border-2 border-dashed border-accent/50 bg-accent-soft/40 p-3 active:cursor-grabbing" title={t("Glissez le clip")}>
          <div className="relative aspect-video w-40 shrink-0 overflow-hidden rounded-md bg-black">
            {c.kind === "gif" || c.kind === "image" ? (src && <img src={src} alt="" draggable={false} className="h-full w-full object-cover" />)
              : thumb ? <img src={thumb} alt="" draggable={false} className="h-full w-full object-cover" />
              : src && <video src={`${src}#t=0.5`} preload="metadata" muted playsInline tabIndex={-1} className="h-full w-full object-cover" />}
          </div>
          <div className="min-w-0">
            <div className="text-[1rem] font-extrabold text-ink">{t("Glissez le clip")}</div>
            <div className="text-[0.86rem] font-semibold text-ink-soft">{t("vers une discussion Discord, un dossier, WhatsApp Web… Il arrive comme un fichier.")}</div>
          </div>
        </div>

        <div className="grid grid-cols-3 gap-3">
          <Tile icon="copy" label={t("Copier le clip")} hint={t("Ctrl+V dans Discord")} onClick={() => act("share.copy", {}, t("Clip copié : collez-le (Ctrl+V) dans Discord ou un dossier."))} />
          <Tile icon="link" label={link ? t("Copier le lien") : t("Créer un lien")} hint={link ? t("Valable encore {0} h", hours) : t("Valable 72 h")} busy={upload ? upload.ratio : undefined}
            onClick={() => (link ? act("share.copyLink", {}, t("Lien copié.")) : makeLink())} />
          <Tile icon="user" label={t("Envoyer à un ami")} hint={t("En message iBox")} pressed={!!friends} onClick={openFriends} />
          {onVertical && <Tile icon="phone" label={t("Format vertical")} hint={t("TikTok, Shorts, Reels")} onClick={onVertical} />}
          <Tile icon="download" label={t("Enregistrer sous…")} hint={t("Une copie où vous voulez")} onClick={() => host<{ path: string } | null>("share.save", { name: c.name }).then((r) => r && setNote({ text: t("Copie enregistrée.") })).catch((e: Error) => setNote({ text: e.message, bad: true }))} />
          <Tile icon="folder" label={t("Ouvrir le dossier")} hint={t("Dans l’Explorateur")} onClick={() => act("share.reveal", {}, t("Dossier ouvert."))} />
        </div>

        {link && (
          <div className="flex items-center gap-2 rounded-full bg-cream-deep py-1 pr-1 pl-5">
            <span className="min-w-0 flex-1 truncate text-[0.9rem] font-bold text-ink select-all">{link.url}</span>
            <Button variant="primary" icon="copy" onClick={() => act("share.copyLink", {}, t("Lien copié."))}>{t("Copier")}</Button>
          </div>
        )}

        {friends && (
          <section aria-label={t("Envoyer à un ami")} className="flex flex-col gap-2">
            {friends.length === 0 && <p className="text-[0.9rem] font-semibold text-ink-soft">{t("Aucun ami pour l’instant : ajoutez-en dans iBox, menu Amis.")}</p>}
            {friends.map((f) => (
              <div key={f.id} className="flex items-center gap-3 rounded-lg bg-cream-deep/60 px-3 py-2">
                <span className="relative grid h-9 w-9 shrink-0 place-items-center rounded-full bg-accent-soft text-[0.95rem] font-extrabold text-accent-text">
                  {f.name.slice(0, 1).toUpperCase()}
                  {f.online && <span className="absolute -right-0.5 -bottom-0.5 h-3 w-3 rounded-full border-2 border-white bg-[#3fbf7f]" aria-label={t("En ligne")} />}
                </span>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[0.95rem] font-bold text-ink">{f.name}</div>
                  {f.game && <div className="truncate text-[0.8rem] font-semibold text-ink-soft">{f.game}</div>}
                </div>
                {sent.includes(f.id)
                  ? <span className="inline-flex items-center gap-1 px-3 text-[0.88rem] font-bold text-accent-text"><Icon name="check" />{t("Envoyé")}</span>
                  : <Button disabled={!!upload} onClick={() => toFriend(f.id)}>{t("Envoyer")}</Button>}
              </div>
            ))}
          </section>
        )}

        <p className="text-[0.8rem] font-semibold text-ink-faint">
          {t("Le lien est créé chez Litterbox (catbox.moe), un hébergeur gratuit sans compte : toute personne qui a le lien peut voir le clip pendant 72 heures. Copier et glisser restent sur votre PC.")}
        </p>
        <div role="status" className={`min-h-[1.3rem] text-[0.88rem] font-bold ${note?.bad ? "text-peach-deep" : "text-accent-text"}`}>
          {upload ? t("Envoi chez Litterbox… {0} %", Math.round(upload.ratio * 100)) : note?.text ?? ""}
        </div>
      </motion.div>
    </motion.div>
  );
}

function Tile({ icon, label, hint, onClick, busy, pressed }: { icon: IconName; label: string; hint: string; onClick: () => void; busy?: number; pressed?: boolean }) {
  return (
    <button
      type="button"
      data-nav
      aria-pressed={pressed}
      onClick={onClick}
      className={`relative flex flex-col items-start gap-2 overflow-hidden rounded-lg p-4 text-left transition active:scale-[0.98] ${pressed ? "bg-accent text-on-accent" : "bg-cream-deep/70 text-ink hover:bg-accent-soft"}`}
    >
      {busy !== undefined && <motion.span className="absolute inset-y-0 left-0 bg-accent/25" animate={{ width: `${busy * 100}%` }} transition={SPRING.smooth} aria-hidden="true" />}
      <Icon name={icon} size="1.6em" />
      <span className="relative text-[0.95rem] font-extrabold">{label}</span>
      <span className={`relative text-[0.78rem] font-semibold ${pressed ? "text-on-accent/80" : "text-ink-soft"}`}>{hint}</span>
    </button>
  );
}
