import "@fontsource-variable/nunito";
import "./styles.css";
import { createRoot } from "react-dom/client";
import { lang, t } from "./i18n";
import { Studio } from "./Studio";

const scheme = new URLSearchParams(location.search).get("scheme") === "light" ? "light" : "dark";
// comme iBox : la règle html[data-theme] (le CSS compilé remplace light-dark() par des variables
// qui ne suivent que cette règle, pas un color-scheme posé en style)
document.documentElement.dataset.theme = scheme;
document.documentElement.lang = lang;
document.title = t("iClips Studio");

createRoot(document.getElementById("root")!).render(<Studio />);
