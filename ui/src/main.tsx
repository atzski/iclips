import "@fontsource-variable/nunito";
import "./styles.css";
import { createRoot } from "react-dom/client";
import { lang, t } from "./i18n";
import { Studio } from "./Studio";

const scheme = new URLSearchParams(location.search).get("scheme") === "light" ? "light" : "dark";
document.documentElement.style.colorScheme = scheme;
document.documentElement.lang = lang;
document.title = t("iClips Studio");

createRoot(document.getElementById("root")!).render(<Studio />);
