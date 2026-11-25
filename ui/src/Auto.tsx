/**
 * Clips automatiques (0.4, permission `clips-auto`) : « Clip ça ! » à la voix, Vue + A à la
 * manette, fin de partie. iBox garde les réglages et fait le travail (src-tauri/src/clips_auto.rs) ;
 * cette fiche les règle et permet d'essayer la voix.
 */
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";
import { host, onHost, type AutoStatus } from "./host";
import { t } from "./i18n";
import { Button, Icon, type IconName } from "./ui";

type Test = "idle" | "listening" | "heard" | "missed";
type Key = "voice" | "pad" | "end";

/** La phrase à dire, dans la langue d'écoute. */
export function phrase(language: string | null | undefined) {
  return language?.toLowerCase().startsWith("en") ? "“Clip that!”" : "« Clip ça ! »";
}

export function AutoSheet({ onClose }: { onClose: () => void }) {
  const [s, setS] = useState<AutoStatus | null>(null);
  const [test, setTest] = useState<Test>("idle");
  const [error, setError] = useState<string | null>(null);
  const back = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    back.current?.focus();
    host<AutoStatus>("auto.get").then(setS).catch((e: Error) => setError(e.message));
    return onHost((event, payload) => {
      if (event === "voice://heard") setTest("heard");
      else if (event === "voice://state") {
        const p = payload as { listening: boolean; error?: string };
        setS((x) => x && { ...x, listening: p.listening, error: p.error ?? (p.listening ? null : x.error) });
      }
    });
  }, []);
  // l'essai dure 8 s ; sans phrase reconnue, l'état est relu (erreur éventuelle de l'écoute)
  useEffect(() => {
    if (test !== "listening") return;
    const id = setTimeout(() => {
      setTest("missed");
      void host<AutoStatus>("auto.get").then(setS).catch(() => undefined);
    }, 8500);
    return () => clearTimeout(id);
  }, [test]);

  const toggle = (key: Key) => {
    if (!s) return;
    const next = !s[key];
    setS({ ...s, [key]: next });
    host<AutoStatus>("auto.set", { [key]: next }).then(setS).catch((e: Error) => { setS(s); setError(e.message); });
  };
  const tryVoice = () => {
    setTest("listening");
    host("auto.test").catch((e: Error) => { setTest("idle"); setError(e.message); });
  };
  const say = phrase(s?.language);
  const voiceError = s?.error ?? null;

  return (
    <motion.div data-nav-scope className="fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-8" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <motion.div role="dialog" aria-modal="true" aria-label={t("Clips automatiques")} className="flex max-h-full w-full max-w-[40rem] flex-col gap-2 overflow-y-auto rounded-xl bg-white p-6 shadow-lg" initial={{ scale: 0.96, y: 12 }} animate={{ scale: 1, y: 0 }} exit={{ scale: 0.97, y: 8 }} transition={SPRING.smooth}>
        <header className="mb-2 flex items-center gap-3">
          <Button ref={back} icon="back" variant="ghost" onClick={onClose} aria-label={t("Retour")} className="px-3" />
          <div className="min-w-0 flex-1">
            <h2 className="font-display text-[1.35rem] leading-tight text-ink">{t("Clips automatiques")}</h2>
            <p className="text-[0.85rem] font-semibold text-ink-soft">{t("Pendant vos parties, iBox garde le clip sans que vous quittiez le jeu.")}</p>
          </div>
        </header>

        <Row icon="mic" title={t("Dire {0}", say)} on={!!s?.voice} disabled={!s} onToggle={() => toggle("voice")}>
          {t("Le micro écoute seulement pendant vos parties. Windows reconnaît la phrase sur ce PC : rien n’est envoyé ni enregistré. Une notification dit quand l’écoute commence.")}
          <div className="mt-3 flex flex-wrap items-center gap-3">
            <Button disabled={!s || test === "listening"} onClick={tryVoice}>{t("Essayer")}</Button>
            <span role="status" className={`text-[0.88rem] font-bold ${test === "heard" ? "text-accent-text" : test === "missed" ? "text-peach-deep" : "text-ink-soft"}`}>
              {test === "listening" ? t("Dites {0}…", say) : test === "heard" ? t("Entendu !") : test === "missed" ? (voiceError ?? t("Rien entendu. Parlez plus près du micro, puis réessayez.")) : ""}
            </span>
          </div>
          {voiceError && test !== "missed" && <p className="mt-2 text-[0.85rem] font-bold text-peach-deep">{voiceError}</p>}
          {s?.listening && test !== "listening" && <p className="mt-2 text-[0.85rem] font-bold text-accent-text">{t("Le micro écoute en ce moment.")}</p>}
        </Row>
        <Row icon="pad" title={t("Maintenir Vue + A")} on={!!s?.pad} disabled={!s} onToggle={() => toggle("pad")}>
          {t("Sur la manette, pendant la partie, sans ouvrir le panneau.")}
        </Row>
        <Row icon="flag" title={t("Fin de partie")} on={!!s?.end} disabled={!s} onToggle={() => toggle("end")}>
          {t("En quittant un jeu, ses dernières secondes sont gardées.")}
        </Row>

        <p className="mt-2 text-[0.8rem] font-semibold text-ink-faint">{t("Chaque clip automatique est annoncé par une notification et marqué dans la galerie. Deux clips ne partent pas à moins de 5 secondes d’écart.")}</p>
        {error && <p role="alert" className="text-[0.85rem] font-bold text-peach-deep">{t("Action impossible : {0}", error)}</p>}
      </motion.div>
    </motion.div>
  );
}

function Row({ icon, title, on, disabled, onToggle, children }: { icon: IconName; title: string; on: boolean; disabled: boolean; onToggle: () => void; children: React.ReactNode }) {
  return (
    <section className="flex gap-4 rounded-lg bg-cream-deep/60 p-4">
      <span className="grid h-11 w-11 shrink-0 place-items-center rounded-full bg-accent-soft text-accent-text"><Icon name={icon} size="1.35em" /></span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-3">
          <h3 className="flex-1 text-[1rem] font-extrabold text-ink">{title}</h3>
          <button
            type="button"
            role="switch"
            data-nav
            aria-checked={on}
            aria-label={title}
            disabled={disabled}
            onClick={onToggle}
            className={`relative h-8 w-14 shrink-0 rounded-full transition disabled:opacity-55 ${on ? "bg-accent" : "bg-line"}`}
          >
            <motion.span className="absolute top-1 left-1 h-6 w-6 rounded-full bg-white shadow" animate={{ x: on ? 24 : 0 }} transition={SPRING.snappy} />
          </button>
        </div>
        <div className="mt-1 text-[0.86rem] font-semibold text-ink-soft">{children}</div>
      </div>
    </section>
  );
}
