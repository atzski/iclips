import { locale, t } from "./i18n";

/** 0:07, 1:05 */
export function duration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "";
  const s = Math.round(seconds);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export function size(bytes: number): string {
  if (!bytes) return "";
  const fr = locale === "fr-FR";
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} ${fr ? "Ko" : "KB"}`;
  return `${(bytes / 1024 / 1024).toLocaleString(locale, { maximumFractionDigits: 1 })} ${fr ? "Mo" : "MB"}`;
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
