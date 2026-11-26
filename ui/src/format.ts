import { locale, t } from "./i18n";

/** 0:07, 1:05 */
export function duration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "";
  const s = Math.round(seconds);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Instant dans l'éditeur, au dixième : `0:02,6` (`0:02.6` en anglais). */
export function instant(seconds: number): string {
  const d = Math.round(Math.max(0, Number.isFinite(seconds) ? seconds : 0) * 10);
  const s = Math.floor(d / 10);
  const sep = locale === "fr-FR" ? "," : ".";
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}${sep}${d % 10}`;
}

export function size(bytes: number): string {
  if (!bytes) return "";
  const fr = locale === "fr-FR";
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} ${fr ? "Ko" : "KB"}`;
  return `${(bytes / 1024 / 1024).toLocaleString(locale, { maximumFractionDigits: 1 })} ${fr ? "Mo" : "MB"}`;
}

const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();

/** Groupe de la galerie (Studio 0.9) : aujourd'hui, hier, les 7 derniers jours, puis le mois. */
export function dateGroup(at: number, now = Date.now()): { key: string; label: string } {
  const d = new Date(at);
  const diff = Math.round((startOfDay(new Date(now)) - startOfDay(d)) / 86_400_000);
  if (diff <= 0) return { key: "today", label: t("Aujourd’hui") };
  if (diff === 1) return { key: "yesterday", label: t("Hier") };
  if (diff < 7) return { key: "week", label: t("7 derniers jours") };
  const month = d.toLocaleDateString(locale, { month: "long", year: "numeric" });
  return { key: `${d.getFullYear()}-${d.getMonth() + 1}`, label: month.charAt(0).toUpperCase() + month.slice(1) };
}

/** Clips rangés par groupe de date, du plus récent au plus ancien. */
export function byDate<T extends { at: number }>(items: T[], now = Date.now()): { key: string; label: string; items: T[] }[] {
  const out: { key: string; label: string; items: T[] }[] = [];
  for (const it of [...items].sort((a, b) => b.at - a.at)) {
    const g = dateGroup(it.at, now);
    const last = out[out.length - 1];
    if (last?.key === g.key) last.items.push(it);
    else out.push({ ...g, items: [it] });
  }
  return out;
}

/** « Aujourd’hui, 14:05 », « Hier, 09:12 », sinon la date. */
export function when(at: number, now = Date.now()): string {
  const d = new Date(at);
  const time = d.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(new Date(now)) - day(d)) / 86_400_000);
  if (diff === 0) return t("Aujourd’hui, {0}", time);
  if (diff === 1) return t("Hier, {0}", time);
  return d.toLocaleDateString(locale, { day: "numeric", month: "long", year: d.getFullYear() === new Date(now).getFullYear() ? undefined : "numeric" });
}
