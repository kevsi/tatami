# Politique de confidentialité — Tatami

**Dernière mise à jour : 25 novembre 2026**

Tatami est un client de bureau (Windows) de lecture de manga. Cette politique décrit comment l'application traite les données. Tatami est un logiciel libre sous licence GPLv3.

## 1. Résumé

- Tatami **ne collecte, ne transmet et ne vend aucune donnée personnelle** à l'éditeur ni à des tiers.
- Toutes les données (bibliothèque, historique, favoris, téléchargements) restent **sur votre machine**, dans un fichier SQLite local et sur votre système de fichiers.
- L'application se connecte uniquement aux **sources de manga que vous choisissez** et, si vous l'activez, aux **services de suivi** (AniList, MyAnimeList, Shikimori, Kitsu) avec votre consentement explicite.
- Aucun télémètre, aucune analytique, aucune publicité.

## 2. Données stockées localement

Tatami conserve sur votre ordinateur, dans le dossier de données de l'application (`%APPDATA%\Tatami` par défaut, configurable) :

- **Bibliothèque** : séries et chapitres ajoutés, catégories, statuts de lecture.
- **Historique et progression** : position de lecture par chapitre, dates de consultation.
- **Favoris et signets** : pages marquées.
- **File de téléchargement** : chemins des fichiers téléchargés (CBZ ou dossiers) et leur état.
- **Préférences** : thème, langue, réglages du lecteur, sources activées/désactivées, dossier de téléchargement.
- **Cache d'images** : pages et couvertures mises en cache pour la lecture hors ligne.
- **Cookies et en-têtes** nécessaires au fonctionnement des sources que vous utilisez (gérés par le moteur de sources).

Ces données ne quittent jamais votre machine, sauf si vous les exportez vous-même (sauvegarde/restauration).

## 3. Connexions réseau

Tatami effectue des requêtes réseau uniquement dans les cas suivants :

- **Sources de manga** : pour rechercher, lister les chapitres et télécharger les pages des sources que vous sélectionnez. Les en-têtes HTTP (agent utilisateur, cookies) sont envoyés à ces sites selon leurs propres conditions d'utilisation.
- **Services de suivi (optionnels)** : si vous connectez un compte AniList, MyAnimeList, Shikimori ou Kitsu, Tatami échange des données avec ces services (listes, progression) via leurs API officielles, avec votre jeton d'accès. Ces jetons sont stockés dans le **trousseau de l'OS** (Windows Credential Manager), jamais en clair dans la base de données.
- **Mises à jour** : si vous utilisez la version auto-hébergée, Tatami peut vérifier la disponibilité d'une nouvelle version auprès du dépôt officiel. La version Microsoft Store est mise à jour par le Store.

Tatami n'envoie aucune donnée à l'éditeur. Aucun identifiant, aucune télémétrie, aucune statistique d'usage.

## 4. Ce que Tatami ne fait pas

- Pas de collecte de données personnelles.
- Pas de création de compte utilisateur chez l'éditeur.
- Pas de publicité ni de pistage publicitaire.
- Pas de vente ni de partage de données avec des tiers.
- Pas d'accès à vos fichiers en dehors du dossier de données de l'application et du dossier de téléchargement que vous avez choisi.

## 5. Contenu et âge

Tatami est un lecteur de manga. Certaines sources proposent du contenu destiné à un public adulte. L'application propose un mode « incognito » qui masque les séries adultes de l'accueil, des favoris et des signets. La responsabilité du choix des sources et du contenu consulté incombe à l'utilisateur. Tatami n'est pas destiné aux enfants de moins de 13 ans.

## 6. Vos droits

Puisque toutes les données sont locales, vous gardez le contrôle total :

- **Consulter / exporter** : la base de données SQLite et le dossier de téléchargement sont des fichiers ordinaires, accessibles dans le dossier de données.
- **Supprimer** : désinstaller l'application et supprimer le dossier de données (`%APPDATA%\Tatami`) efface l'ensemble des données locales. Pour la version Microsoft Store, la désinstallation supprime aussi les données de l'application.
- **Révoquer l'accès aux services de suivi** : déconnecter le compte dans les réglages de Tatami révoque l'utilisation du jeton ; vous pouvez aussi révoquer l'autorisation côté service (AniList, MAL, etc.).

## 7. Sécurité

- Les jetons OAuth sont stockés dans le trousseau de l'OS (Windows Credential Manager).
- Les téléchargements sont vérifiés (type de fichier déduit des octets, pas des en-têtes) avant écriture.
- Les noms de fichiers dérivés de données distantes sont assainis (protection contre la traversée de chemin).
- Les mises à jour du moteur de sources sont vérifiées par signature et hash avant installation.

## 8. Modifications de cette politique

Cette politique peut être mise à jour. La date de dernière mise à jour est indiquée en haut. Les modifications substantielles seront signalées dans les notes de version de l'application.

## 9. Contact

Pour toute question relative à la confidentialité : ouvrir une issue sur le dépôt du projet (https://github.com/kevsi/tatami) ou contacter le mainteneur.

---

# Privacy Policy — Tatami (English)

**Last updated: November 25, 2026**

Tatami is a desktop manga reader client (Windows). This policy describes how the application handles data. Tatami is free software under the GPLv3 license.

## 1. Summary

- Tatami **does not collect, transmit, or sell any personal data** to the publisher or third parties.
- All data (library, history, favorites, downloads) stays **on your machine**, in a local SQLite database and on your file system.
- The application connects only to the **manga sources you choose** and, if you enable it, to **tracking services** (AniList, MyAnimeList, Shikimori, Kitsu) with your explicit consent.
- No telemetry, no analytics, no advertising.

## 2. Locally stored data

Tatami keeps on your computer, in the application data folder (`%APPDATA%\Tatami` by default, configurable):

- **Library**: series and chapters added, categories, reading status.
- **History and progress**: reading position per chapter, consultation dates.
- **Favorites and bookmarks**: marked pages.
- **Download queue**: paths of downloaded files (CBZ or folders) and their state.
- **Preferences**: theme, language, reader settings, enabled/disabled sources, download folder.
- **Image cache**: pages and covers cached for offline reading.
- **Cookies and headers** required for the sources you use (managed by the source engine).

This data never leaves your machine unless you export it yourself (backup/restore).

## 3. Network connections

Tatami makes network requests only in the following cases:

- **Manga sources**: to search, list chapters, and download pages from sources you select. HTTP headers (user agent, cookies) are sent to those sites under their own terms of use.
- **Tracking services (optional)**: if you connect an AniList, MyAnimeList, Shikimori, or Kitsu account, Tatami exchanges data with those services (lists, progress) via their official APIs, using your access token. These tokens are stored in the **OS keychain** (Windows Credential Manager), never in plain text in the database.
- **Updates**: if you use the self-hosted version, Tatami may check for a new version from the official repository. The Microsoft Store version is updated by the Store.

Tatami sends no data to the publisher. No identifier, no telemetry, no usage statistics.

## 4. What Tatami does not do

- No collection of personal data.
- No user account creation with the publisher.
- No advertising or ad tracking.
- No sale or sharing of data with third parties.
- No access to your files outside the application data folder and the download folder you chose.

## 5. Content and age

Tatami is a manga reader. Some sources offer content intended for adults. The application offers an "incognito" mode that hides adult series from the home screen, favorites, and bookmarks. Responsibility for the choice of sources and content consulted lies with the user. Tatami is not intended for children under 13.

## 6. Your rights

Since all data is local, you retain full control:

- **View / export**: the SQLite database and download folder are ordinary files, accessible in the data folder.
- **Delete**: uninstalling the application and removing the data folder (`%APPDATA%\Tatami`) erases all local data. For the Microsoft Store version, uninstallation also removes application data.
- **Revoke access to tracking services**: disconnecting the account in Tatami's settings stops use of the token; you can also revoke authorization on the service side (AniList, MAL, etc.).

## 7. Security

- OAuth tokens are stored in the OS keychain (Windows Credential Manager).
- Downloads are verified (file type inferred from bytes, not headers) before writing.
- File names derived from remote data are sanitized (path traversal protection).
- Source engine updates are verified by signature and hash before installation.

## 8. Changes to this policy

This policy may be updated. The last updated date is shown at the top. Substantial changes will be announced in the application's release notes.

## 9. Contact

For any privacy-related question: open an issue on the project repository or contact the maintainer.
