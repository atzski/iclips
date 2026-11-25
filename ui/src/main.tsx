import "@fontsource-variable/nunito";
import "./styles.css";
import { createRoot } from "react-dom/client";
import { MotionConfig } from "motion/react";
import { lang, t } from "./i18n";
import { Panel } from "./Panel";
import { Studio } from "./Studio";
import { ToastPage } from "./Toast";

const scheme = new URLSearchParams(location.search).get("scheme") === "light" ? "light" : "dark";
// comme iBox : la règle html[data-theme] (le CSS compilé remplace light-dark() par des variables
// qui ne suivent que cette règle, pas un color-scheme posé en style)
document.documentElement.dataset.theme = scheme;
document.documentElement.lang = lang;
// trois pages : le Studio (fenêtre à part), le panneau en jeu (fenêtre transparente) et la
// notification de l'app autonome (fenêtre transparente, `app/`)
const panel = location.pathname.endsWith("/panel.html");
const toast = location.pathname.endsWith("/toast.html");
document.title = panel || toast ? "iClips" : t("iClips Studio");
if (panel || toast) document.documentElement.classList.add("panel");

// iBox change de langue ou de thème : la fenêtre se recharge avec les nouveaux réglages
window.addEventListener("ibox:host", (e) => {
  const d = (e as CustomEvent<{ event: string; payload: { lang?: string; scheme?: string } }>).detail;
  if (d?.event !== "look://changed") return;
  const url = new URL(location.href);
  if (d.payload.lang) url.searchParams.set("lang", d.payload.lang);
  if (d.payload.scheme) url.searchParams.set("scheme", d.payload.scheme);
  if (url.href !== location.href) location.replace(url.href);
});

// animations réduites si Windows le demande
createRoot(document.getElementById("root")!).render(<MotionConfig reducedMotion="user">{toast ? <ToastPage /> : panel ? <Panel /> : <Studio />}</MotionConfig>);
