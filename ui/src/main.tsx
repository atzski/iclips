import "@fontsource-variable/nunito";
import "./styles.css";
import { createRoot } from "react-dom/client";
import { lang, t } from "./i18n";
import { Panel } from "./Panel";
import { Studio } from "./Studio";

const scheme = new URLSearchParams(location.search).get("scheme") === "light" ? "light" : "dark";
// comme iBox : la règle html[data-theme] (le CSS compilé remplace light-dark() par des variables
// qui ne suivent que cette règle, pas un color-scheme posé en style)
document.documentElement.dataset.theme = scheme;
document.documentElement.lang = lang;
// deux pages : le Studio (fenêtre à part) et le panneau en jeu (fenêtre transparente)
const panel = location.pathname.endsWith("/panel.html");
document.title = panel ? "iClips" : t("iClips Studio");
if (panel) document.documentElement.classList.add("panel");

createRoot(document.getElementById("root")!).render(panel ? <Panel /> : <Studio />);
