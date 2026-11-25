# iClips : conception

Le studio des clips de jeu d'iBox : on garde le moment, on le monte, on le partage. Plugin signé, publié à part d'iBox (comme Music in the Box), Windows seulement (les clips d'iBox passent par Windows Graphics Capture et Media Foundation).

## Principe

| Partie | Où | Pourquoi |
|---|---|---|
| **Socle natif** : capture continue (`clips.rs`), fichiers des clips, vignettes, coupe et rendu Media Foundation, reconnaissance vocale, envoi pair à pair | iBox, endormi sans plugin | Le code natif ne se télécharge pas (sécurité, anti-triche). |
| **Plugin** : Studio (galerie, lecteur, montage), panneau en jeu, réglages | Ce dépôt, paquet `.iboxplugin` signé | Interface et logique se corrigent sans republier iBox. |

La fenêtre du plugin n'appelle aucune commande d'iBox : tout passe par `POST /iclips/__host/<appel>`, vérifié contre les permissions accordées (`ibox/docs/PLUGINS.md`).

## Fonctions

1. **Galerie et lecteur** (C1, C2) : clips du profil par date, par jeu et favoris, titre, suppression ; tout au clavier et à la manette.
2. **Panneau en jeu** (C2) : garder le clip, le nommer, l'étoiler sans quitter la partie (raccourci et combo manette, comme Music in the Box).
3. **Montage** (C3) : couper le début et la fin (sans réencodage, aux images clés), découper, assembler plusieurs clips, titre, export MP4 ou GIF. Rendu natif par tâches, avec progression.
4. **Clips automatiques** (C4) :
   - **commande vocale « Clip ça ! »** (« Clip that! » en anglais) : le micro écoute seulement pendant une partie, seulement si l'option est activée, et la reconnaissance reste sur le PC (moteur de Windows, grammaire d'une seule phrase : rien n'est envoyé en ligne). Un témoin dit quand le micro écoute. Sur ce PC, le moteur français de Windows est présent (`MS-1036-80-DESK`) ;
   - combo manette, fin de partie ; succès débloqués si iBox peut les voir au moment où ils tombent (à vérifier : `achievements.rs` les lit par requêtes).
5. **Partage entre amis** (C5) : sans stockage sur Neon (payant à l'usage). Choix à faire : pair à pair par la signalisation déjà en place pour Remote Play (gratuit, l'ami doit être en ligne) ou hébergeur externe.

## Étapes

| Étape | Contenu | État |
|---|---|---|
| **C1. Socle et squelette** | iBox : permission `clips`, écran `screen` (fenêtre `plugin-<id>:screen`), appels `clips.*`, lecture des vidéos par morceaux, blocage des commandes d'iBox pour les fenêtres de plugin, bouton « Ouvrir » dans Paramètres › Plugins. Plugin : Studio (galerie, lecteur, titre, favori, suppression), paquet signé. | Fait le 25/11/2026, non publié ; essayé dans la copie isolée d’iBox (`app.ibox.manualtest`, `DocumentsiBox-sauvegardeplugin-testessai-iclips.ps1`) |
| C2. Panneau en jeu | Overlay générique empaqueté (`overlay.entry`), vignettes natives, index des clips | |
| C3. Montage | Coupe, assemblage, titre, export | |
| C4. Clips automatiques | « Clip ça ! », combo, fin de partie | |
| C5. Partage | Pair à pair ou hébergeur | |

## Questions ouvertes

- Partage : pair à pair ou hébergeur externe ?
- Durée : 30 s au plus comme aujourd'hui, ou un enregistrement long (environ 180 Mo de mémoire pour 2 minutes en 1080p60) ?
