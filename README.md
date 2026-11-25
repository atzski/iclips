# iClips

Plugin pour [iBox](https://github.com/atzski/ibox) : le studio de vos clips de jeu. Galerie, lecteur, titres et favoris, montage (coupe, assemblage, GIF), clips automatiques (commande vocale « Clip ça ! », Vue + A à la manette, fin de partie), partage (glisser le clip dans Discord, copier, lien de 72 h, envoi à un ami iBox, format vertical 9:16).

Conception et étapes : [docs/CONCEPTION.md](docs/CONCEPTION.md).

## Développement

```
npm install
npm run dev      # aperçu du Studio avec des clips d'exemple : http://localhost:5173/studio.html
npm test
PACK_LOCAL=1 npm run pack   # paquet signé + dist/catalog.json pour IBOX_PLUGINS_CATALOG
```

`npm run release` publie (Release GitHub et catalogue iNetbox) : sur demande explicite seulement. Le plugin demande iBox 1.0.8 ou plus récent.

`node scripts/icon.mjs "<logo>.png"` refait `icon.png` et `logo.png` (fond transparent) depuis le logo source.

Les fichiers de `ui/src/ibox/` sont des copies d'iBox (couleurs, ressorts, police Cal Sans) : les recopier quand iBox les change.
