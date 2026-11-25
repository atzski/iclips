# iClips, application Windows autonome

iClips sans iBox : la même interface que le plugin (`../ui`), servie par une petite application Tauri 2 qui reprend le socle natif d'iBox (capture Windows Graphics Capture, encodeur H.264 Media Foundation matériel, son WASAPI, montage, « Clip ça ! », partage).

## Construire

```powershell
pwsh -NoProfile -File scripts/build-app.ps1
```

Depuis la racine du dépôt : tests de l'interface et du natif, interface construite dans `dist/ui`, chemins personnels effacés du binaire, analyse anti-fuite bloquante, installeur copié dans `dist/iClips_<version>_x64-setup.exe`. Ne jamais publier un `npx tauri build` nu (chemins de compilation dans l'exécutable).

## Comment l'interface parle au natif

Exactement comme dans iBox : `POST http://iboxplugin.localhost/iclips/__host/<appel>`, fichiers par `__clips/`, `__thumbs/`, `__shot`, événements par `ibox:host`. Les pages sont servies sous `/iclips/ui/` depuis l'exécutable (`frontendDist`). L'adresse porte `?app=1` : l'interface cache alors ce qui demande un compte iNetbox (envoyer à un ami, Communauté) et montre les réglages de l'application (`app.get`, `app.set`, `app.folder`).

| Module (`src-tauri/src`) | Rôle | Repris d'iBox |
|---|---|---|
| `protocol.rs`, `host.rs` | protocole `iboxplugin` et appels | `plugins/protocol.rs`, `plugins/host.rs` |
| `game.rs` | partie en cours : fenêtre au premier plan, plein écran sans bordure, ni Explorateur, ni navigateur, ni Wallpaper Engine | nouveau (iBox lit sa propre partie) |
| `capture.rs`, `clips.rs`, `clip_edit.rs` | mémoire des clips, fichiers, montage | oui |
| `clips_auto.rs`, `voice.rs`, `voice_sapi.rs` | « Clip ça ! », Vue + A, fin de partie | oui |
| `share.rs` | glisser, copier, enregistrer, lien Litterbox | oui (sans iNetbox) |
| `panel.rs`, `toast.rs`, `win.rs` | panneau en jeu, notification « clip », manette | `plugins/panel.rs`, `toast.rs`, `music/overlay.rs` |
| `remote_play/` | WGC, Media Foundation, WASAPI | oui |
| `prefs.rs`, `shortcut.rs` | réglages gardés, raccourci global | Music in the Box |

Réglages : `%APPDATA%\com.atzski.iclips\settings.json` (raccourci, dossier, qualité, appareils, langue, thème) et `clips-auto.json`. Clips : `Vidéos\iClips` par défaut.
