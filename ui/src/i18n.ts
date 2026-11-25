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
  // montage
  "Assembler": "Join",
  "Couper": "Trim",
  "Choisissez les clips dans l’ordre voulu": "Pick the clips in the order you want",
  "{0} clips choisis": "{0} clips picked",
  "Montage impossible : {0}": "Edit failed: {0}",
  "{0} (coupé)": "{0} (trimmed)",
  "Coupe du clip…": "Trimming the clip…",
  "Création du GIF…": "Making the GIF…",
  "Assemblage de {0} clips…": "Joining {0} clips…",
  // clips automatiques
  "Automatique": "Automatic",
  "Clips automatiques": "Automatic clips",
  "Pendant vos parties, iBox garde le clip sans que vous quittiez le jeu.": "During your games, iBox keeps the clip without you leaving the game.",
  "Dire {0}": "Say {0}",
  "Le micro écoute seulement pendant vos parties. Windows reconnaît la phrase sur ce PC : rien n’est envoyé ni enregistré. Une notification dit quand l’écoute commence.": "The microphone only listens during your games. Windows recognizes the phrase on this PC: nothing is sent or recorded. A notification tells you when listening starts.",
  "Essayer": "Try it",
  "Dites {0}…": "Say {0}…",
  "Entendu !": "Heard!",
  "Rien entendu. Parlez plus près du micro, puis réessayez.": "Nothing heard. Speak closer to the microphone, then try again.",
  "Le micro écoute en ce moment.": "The microphone is listening right now.",
  "Le micro écoute": "Mic listening",
  "Maintenir Vue + A": "Hold View + A",
  "Sur la manette, pendant la partie, sans ouvrir le panneau.": "On the controller, during the game, without opening the panel.",
  "Fin de partie": "Game over",
  "En quittant un jeu, ses dernières secondes sont gardées.": "When you quit a game, its last seconds are kept.",
  "Chaque clip automatique est annoncé par une notification et marqué dans la galerie. Deux clips ne partent pas à moins de 5 secondes d’écart.": "Each automatic clip is announced by a notification and marked in the gallery. Two clips are never kept less than 5 seconds apart.",
  "Gardé à la voix": "Kept by voice",
  "Gardé à la manette": "Kept with the controller",
  "Gardé en fin de partie": "Kept when the game ended",
  "Passage : {0} à {1} ({2})": "Passage: {0} to {1} ({2})",
  "Début du passage": "Start of the passage",
  "Fin du passage": "End of the passage",
  "Lire le passage": "Play the passage",
  "Créer le clip": "Make the clip",
  "Créer un GIF": "Make a GIF",
  "Un GIF dure 10 secondes au plus : raccourcissez le passage.": "A GIF lasts 10 seconds at most: shorten the passage.",
  "Faites glisser les poignées pour choisir le passage (à la manette : gauche et droite sur une poignée). Le clip commence sur l’image clé qui précède le début (une par seconde).": "Drag the handles to choose the passage (on a controller: left and right on a handle). The clip starts on the keyframe before the start (one per second).",
};

export const lang: "fr" | "en" = typeof location !== "undefined" && new URLSearchParams(location.search).get("lang") === "en" ? "en" : "fr";

export function t(fr: string, ...args: (string | number)[]): string {
  const s = lang === "en" ? (EN[fr] ?? fr) : fr;
  return args.reduce<string>((acc, a, i) => acc.replace(`{${i}}`, String(a)), s);
}

export const locale = lang === "en" ? "en-GB" : "fr-FR";
