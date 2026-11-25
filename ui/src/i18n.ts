/**
 * Textes en français, traduits ici en anglais. La langue vient d'iBox (`?lang=` posé à
 * l'ouverture du Studio). Les paramètres s'écrivent {0}, {1}…
 */

const EN: Record<string, string> = {
  "iClips Studio": "iClips Studio",
  "Tous": "All",
  "Favoris": "Favorites",
  "Fermer": "Close",
  "Retour": "Back",
  "Supprimer": "Delete",
  "Annuler": "Cancel",
  "Favori": "Favorite",
  "Retirer des favoris": "Remove from favorites",
  "Ajouter aux favoris": "Add to favorites",
  "Titre du clip": "Clip title",
  "Sans titre": "Untitled",
  "Hors jeu": "Outside a game",
  "Supprimer ce clip ? Il est effacé de ce PC.": "Delete this clip? It is erased from this PC.",
  "Aucun clip pour l’instant": "No clips yet",
  "Pendant une partie, ouvrez l’overlay d’iBox et gardez les dernières secondes : le clip arrive ici.": "During a game, open the iBox overlay and keep the last seconds: the clip lands here.",
  "Aucun favori": "No favorites",
  "Marquez un clip d’une étoile pour le retrouver ici.": "Star a clip to find it here.",
  "{0} clips": "{0} clips",
  "1 clip": "1 clip",
  "Lecture impossible.": "Playback failed.",
  "Action impossible : {0}": "Action failed: {0}",
  "Aujourd’hui, {0}": "Today, {0}",
  "Hier, {0}": "Yesterday, {0}",
  // panneau en jeu
  "Hors partie": "Not in a game",
  "Clip gardé": "Clip kept",
  "Ouvrir dans le Studio": "Open in the Studio",
  "Ouvrir le Studio": "Open the Studio",
  "Enregistrement…": "Saving…",
  "Garder les {0} dernières secondes": "Keep the last {0} seconds",
  "Garder le clip": "Keep the clip",
  "Les clips sont désactivés dans Paramètres › Captures.": "Clips are turned off in Settings › Captures.",
  "Les clips se gardent pendant une partie lancée depuis iBox.": "Clips are kept during a game launched from iBox.",
  "La mémoire du clip se remplit…": "The clip memory is filling up…",
  "Alt+V ou Vue + X : ouvrir et fermer": "Alt+V or View + X: open and close",
  // Studio
  "Tous les jeux": "All games",
};

export const lang: "fr" | "en" = typeof location !== "undefined" && new URLSearchParams(location.search).get("lang") === "en" ? "en" : "fr";

export function t(fr: string, ...args: (string | number)[]): string {
  const s = lang === "en" ? (EN[fr] ?? fr) : fr;
  return args.reduce<string>((acc, a, i) => acc.replace(`{${i}}`, String(a)), s);
}

export const locale = lang === "en" ? "en-GB" : "fr-FR";
