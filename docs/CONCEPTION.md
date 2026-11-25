# iClips : conception

Le studio des clips de jeu d'iBox : on garde le moment, on le monte, on le partage. Plugin signé, publié à part d'iBox (comme Music in the Box), Windows seulement (les clips d'iBox passent par Windows Graphics Capture et Media Foundation).

## Principe

| Partie | Où | Pourquoi |
|---|---|---|
| **Socle natif** : capture continue (`clips.rs`), fichiers des clips, vignettes, coupe et rendu Media Foundation, reconnaissance vocale, envoi pair à pair | iBox, endormi sans plugin | Le code natif ne se télécharge pas (sécurité, anti-triche). |
| **Plugin** : Studio (galerie, lecteur, montage), panneau en jeu, réglages | Ce dépôt, paquet `.iboxplugin` signé | Interface et logique se corrigent sans republier iBox. |

La fenêtre du plugin n'appelle aucune commande d'iBox : tout passe par `POST /iclips/__host/<appel>`, vérifié contre les permissions accordées (`ibox/docs/PLUGINS.md`).

## Fonctions

1. **Galerie et lecteur** (C1, C2) : clips du profil par date, par jeu et favoris, vignettes, titre, suppression ; tout au clavier et à la manette.
2. **Panneau en jeu** (C2) : garder le clip, le nommer, l'étoiler sans quitter la partie (Alt+V ou Vue + X maintenus). Raccourci réglable dans iBox : à faire.
3. **Montage** (C3) : couper un passage (sans réencodage, calé sur l'image clé d'avant, une par seconde), assembler plusieurs clips de même qualité, exporter un GIF. Rendu natif par tâches, avec progression. Plus tard : titre incrusté et assemblage de qualités différentes (réencodage).
4. **Clips automatiques** (C4) :
   - **commande vocale « Clip ça ! »** (« Clip that! » en anglais) : le micro écoute seulement pendant une partie, seulement si l'option est activée, et la reconnaissance reste sur le PC (moteur de Windows, grammaire d'une seule phrase : rien n'est envoyé en ligne). Un témoin dit quand le micro écoute. Sur ce PC, le moteur français de Windows est présent (`MS-1036-80-DESK`) ;
   - combo manette, fin de partie ; succès débloqués si iBox peut les voir au moment où ils tombent (à vérifier : `achievements.rs` les lit par requêtes).
5. **Partage entre amis** (C5) : sans stockage sur Neon (payant à l'usage). Choix à faire : pair à pair par la signalisation déjà en place pour Remote Play (gratuit, l'ami doit être en ligne) ou hébergeur externe.

## Étapes

| Étape | Contenu | État |
|---|---|---|
| **C1. Socle et squelette** | iBox : permission `clips`, écran `screen` (fenêtre `plugin-<id>:screen`), appels `clips.*`, lecture des vidéos par morceaux, blocage des commandes d'iBox pour les fenêtres de plugin, bouton « Ouvrir » dans Paramètres › Plugins. Plugin : Studio (galerie, lecteur, titre, favori, suppression), paquet signé. | Fait le 25/11/2026, non publié ; essayé dans la copie isolée d’iBox (`app.ibox.manualtest`, `Documents\iBox-sauvegarde\plugin-test\essai-iclips.ps1`) |
| **C2. Panneau en jeu** | iBox : panneau empaqueté (`overlay.entry`, Alt+V, Vue + X), permission `clips-capture` (`clips.status`, `clips.save`), `screen.open`, vignettes (`clips.thumb`, `__thumbs/`). Plugin 0.2.0 : panneau (garder, nommer, étoiler, ouvrir le Studio), vignettes faites dans la page, filtre par jeu. Pas d'index SQLite : le dossier et les fichiers json suffisent pour des centaines de clips. | Fait le 25/11/2026, non publié ; essayé dans la copie isolée (Alt+V ouvre et ferme, clip gardé, mise à jour 0.1 → 0.2) |
| **C3. Montage** | iBox : `clip_edit.rs` (permission `clips-edit`, `edit.render`), coupe et assemblage sans réencodage, GIF de 10 s au plus, progression. Plugin 0.3.0 : « Couper » dans le lecteur (début et fin à la lecture, I et O), « Créer le clip », « Créer un GIF », mode « Assembler » dans la galerie (ordre des clics). Pas de titre incrusté ni d'assemblage de formats différents : il faudrait réencoder. | Fait le 25/11/2026, non publié ; essayé dans la copie isolée sur des clips au format d'iBox (coupe 2-6 s → 4,02 s, assemblage 10 + 10 s → 20,01 s, GIF 27 Ko) |
| **C4. Clips automatiques** | iBox : `voice.rs` (moteur hors ligne de Windows, liste de phrases « clip ça » / « clippe ça », « clip that » / « clip it », confiance moyenne ou haute), `clips_auto.rs` (permission `clips-auto`, `auto.get`, `auto.set`, `auto.test`, réglages dans `clips-auto.json`, tous coupés au départ ; micro ouvert seulement en partie ; Vue + A maintenus ; clip de fin de partie gardé avant que la mémoire ne se vide ; pas deux clips à moins de 5 s ; notification « Clip gardé » et témoin « « Clip ça ! » vous écoute », montrés même en « Ne pas déranger » ; notifications d'iBox exclues des captures). Plugin 0.4.0 : fiche « Automatique » du Studio (trois interrupteurs, essai de la voix 8 s), marque sur les cartes, témoin « Le micro écoute » dans le panneau. | Fait le 25/11/2026, non publié. Succès débloqués : plus tard (`achievements.rs` les lit par requêtes). |
| C5. Partage | Pair à pair ou hébergeur | |

## Questions ouvertes

- Partage : pair à pair ou hébergeur externe ?
- Durée : 30 s au plus comme aujourd'hui, ou un enregistrement long (environ 180 Mo de mémoire pour 2 minutes en 1080p60) ?
