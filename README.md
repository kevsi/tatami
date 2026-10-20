# Tatami

Client desktop de lecture de manga pour Windows.

Tatami réunit un catalogue de plus de 1 370 sources, un lecteur vertical, une bibliothèque et le
téléchargement hors ligne dans une seule application. Aucun compte, aucune télémétrie.

> **Avertissement.** Tatami fonctionne comme un navigateur : l'application n'héberge et ne
> distribue aucun contenu protégé. Les sources sont des sites tiers consultés à la demande de
> l'utilisateur, comme un navigateur web.

## ⬇️ Télécharger

[**Dernière version pour Windows**](https://github.com/kevsi/tatami/releases/latest)

Deux installateurs sont proposés : `.msi` (Windows Installer) et `-setup.exe` (NSIS). Windows 10
ou 11, 64 bits. Le moteur et son runtime Java sont inclus : aucune installation de Java n'est
nécessaire.

> **Version non signée.** SmartScreen peut afficher « Windows a protégé votre PC » : cliquez sur
> « Informations complémentaires », puis « Exécuter quand même ». Comparez l'empreinte SHA-256
> publiée dans la release avec celle du fichier téléchargé.

## ✨ Fonctionnalités

- **Catalogue** — plus de 1 370 sources, recherche globale, filtres par langue et tri.
- **Lecteur** — défilement vertical virtualisé, saut de page, zoom, raccourcis clavier et reprise
  à la page exacte.
- **Bibliothèque** — favoris, catégories, historique, signets à la page et masquage du contenu
  adulte sur l'accueil (incognito).
- **Hors ligne** — téléchargement des chapitres en `.cbz`, file d'attente et reprise.
- **Nouveautés** — chapitres récents et notifications pour les favoris.
- **Interface** — français et anglais, thèmes clair et sombre.

## 🚀 Premier lancement

1. Choisir une langue, puis une source dans le catalogue.
2. Chercher une série, ouvrir sa fiche et cliquer sur un chapitre.
3. Pour lire hors ligne, télécharger un chapitre depuis sa fiche.

Les téléchargements sont enregistrés dans `%APPDATA%\Tatami\downloads` (dossier modifiable dans
les réglages), au format `.cbz` : une archive d'images par chapitre.

## 🔒 Données et confidentialité

Tout reste sur votre machine. Aucune télémétrie, aucun compte, aucune donnée envoyée à l'éditeur.

| Donnée | Emplacement |
|---|---|
| Bibliothèque, favoris, historique | `%APPDATA%\Tatami\tatami.db` |
| Cache d'images | `%APPDATA%\Tatami\cache\` |
| Téléchargements | `%APPDATA%\Tatami\downloads\` |

Le réseau n'est utilisé que pour :

- les sources que vous consultez ;
- la vérification des favoris par le planificateur, désactivable dans les réglages ;
- la résolution DNS en DNS-over-HTTPS via Cloudflare ;
- une mise à jour du moteur, uniquement si vous fournissez vous-même le manifeste signé.

## 🩺 Problèmes courants

| Symptôme | Solution |
|---|---|
| SmartScreen bloque l'installation | Version non signée : « Informations complémentaires » → « Exécuter quand même », après vérification du SHA-256. |
| Une fenêtre de vérification s'ouvre | La source demande un contrôle Cloudflare. Résolvez-le dans la fenêtre : Tatami réutilise ensuite les cookies. |
| Une source renvoie une erreur | Le site est peut-être indisponible ou bloqué. Essayez une autre source ; les journaux de diagnostic donnent le détail. |
| Une page reste vide | La source ne sert pas cette page ; le lecteur l'indique au lieu d'afficher une image cassée. |

## 📄 Licence

**GPLv3** — voir [`LICENSE`](LICENSE). La licence est imposée par `kotatsu-parsers-redo`, dont le
moteur JVM dépend.

Tatami s'appuie sur [Kotatsu](https://github.com/KotatsuApp/Kotatsu),
[Kotatsu-Redo](https://github.com/Kotatsu-Redo/Kotatsu-Redo) et
[kotatsu-parsers-redo](https://github.com/Kotatsu-Redo/kotatsu-parsers-redo). Aucun code de ces
projets n'est recopié : le moteur charge la librairie publiée via JitPack.

## 🔏 Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/). Les rôles et la politique de confidentialité sont
dans [`CODE_SIGNING.md`](CODE_SIGNING.md).

## 🛠️ Développement

Construire depuis les sources, architecture et vérifications : [`docs/development.md`](docs/development.md).
