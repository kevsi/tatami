# 0020 — Mise à jour automatique de l'application

- Date : 2026-10-20
- Statut : proposée

## Contexte

Tatami 0.1.0 est un client desktop Windows (Tauri v2, interface Next.js exportée,
moteur JVM). La mise à jour du **moteur** est déjà un mécanisme distinct : la CLI
`tatami-moteur` installe un manifeste signé et une archive fournis par
l'utilisateur, sans téléchargement automatique. Rien, en revanche, ne met à jour
l'**application** : chaque version impose de retélécharger l'installeur à la main.

Mesures et vérifications au 2026-10-20 :

- `tauri-cli 2.12.1` ; aucun `tauri-plugin-updater` dans `Cargo.toml`, aucun
  `plugins.updater` ni `createUpdaterArtifacts` dans `tauri.conf.json`.
- Aucun workflow de release : la 0.1.0 a été construite localement ; ses
  installeurs MSI et NSIS ne sont pas signés (tâche 5.6 ouverte).
- Le dépôt est **privé** : `https://github.com/kevsi/tatami/releases/latest/download/latest.json`
  n'est pas lisible sans authentification. C'est un préalable à la mise à jour
  publique, pas un détail d'implémentation.
- SignPath Foundation n'est pas encore obtenu. La signature Authenticode et la
  signature Tauri sont deux choses distinctes : la première est vérifiée par
  Windows au lancement de l'installeur, la seconde par l'updater **avant** de
  l'exécuter.
- `tauri signer sign` existe dans la CLI 2.12.1, avec `--private-key-path`,
  `--password` et `--app-version`. Une signature Tauri peut donc être produite
  **après** une signature Authenticode.
- Documentation officielle Tauri v2 : plugin `tauri-plugin-updater`, API Rust
  `UpdaterExt::check`, `Update::download_and_install`, ou `download()` puis
  `install()` ; `app.restart()` ; manifeste `latest.json` avec
  `platforms.windows-x86_64.signature` (le **contenu** du fichier `.sig`) et
  `.url` ; `bundle.createUpdaterArtifacts: true` ;
  `plugins.updater.windows.installMode` (`passive` par défaut, recommandé).
- Le schéma de configuration Tauri 2.12.1 confirme que le mode d'installation
  NSIS par défaut est `currentUser` (installation par utilisateur, sans UAC).

## Décision

1. **Plugin officiel, côté Rust.** Ajouter `tauri-plugin-updater` et exposer à
   l'interface des commandes Tauri (`etat_maj`, `verifier_maj`,
   `installer_maj`), plus des événements de progression. L'interface ne parle
   pas directement au plugin JS ; elle reste sur le contrat commandes/événements
   du projet. Le redémarrage se fait par `app.restart()` côté Rust, sans
   `tauri-plugin-process`. L'alternative API JS est étudiée plus bas ; elle
   reste possible si le coût Rust se révèle disproportionné.

2. **Deux signatures, dans cet ordre.** Build → installeurs non signés →
   **SignPath** (Authenticode) → **`tauri signer sign`** sur l'installeur
   **signé** → `latest.json` avec le nouveau `.sig`. Dans la chaîne de
   production, le `.sig` produit par `createUpdaterArtifacts` avant SignPath est
   **temporaire** et ne doit jamais être publié : SignPath modifie les octets,
   donc il ne valide plus l'installeur final. En mode test sans SignPath, ce
   `.sig` est valide et peut être publié.

3. **Clé Tauri dédiée.** Une paire minisign de **production** générée une fois,
   distincte de SignPath. La clé publique va dans `tauri.conf.json`
   (`plugins.updater.pubkey`). La clé privée et son mot de passe vivent dans
   l'environnement GitHub `release` (secrets protégés, approbation manuelle) et
   dans une sauvegarde hors ligne. Une paire de **test** distincte sert aux
   builds de prérelease : sa publique ne vit que dans une configuration de test,
   sa privée dans un environnement de test. La paire de production n'est jamais
   utilisée pour un build de test. Aucune clé privée dans le dépôt ; les motifs
   de fichier de clé sont ajoutés au `.gitignore`. La perte ou la compromission
   de la clé impose une nouvelle version installée manuellement : les versions
   déjà installées ne peuvent pas vérifier une nouvelle clé publique.

4. **Canal stable unique.** Endpoint
   `https://github.com/kevsi/tatami/releases/latest/download/latest.json`,
   cible `windows-x86_64`, installeur **NSIS** (`-setup.exe`) pour l'updater.
   Pas de canal beta, pas de delta, pas de mise à jour forcée. Le `latest.json`
   est un asset de la release publiée ; GitHub `releases/latest` ignore les
   préreleases, ce qui permet de tester une version avant de la promouvoir.

5. **Recherche automatique, installation consentie.** Au plus une vérification
   automatique par 24 h, 5 s après l'affichage de l'accueil, si le réglage
   (proposé activé par défaut) le permet. Un bouton « Rechercher des mises à
   jour » reste toujours disponible. Le téléchargement et l'installation ne
   partent que sur action explicite (« Télécharger et installer »), avec
   confirmation que l'application va se fermer. Aucune installation pendant une
   file de téléchargements active : le bouton est désactivé et la raison est
   affichée. Les notes de version sont visibles avant l'action.

6. **Confidentialité.** La vérification est une requête HTTPS vers GitHub
   Releases ; GitHub reçoit l'adresse IP et l'agent utilisateur, pas de
   télémétrie ni d'identifiant. Le réglage peut être désactivé ; une
   vérification manuelle reste possible. `CODE_SIGNING.md` et le README sont
   mis à jour **avant** la première release publique avec updater.

7. **Déploiement progressif.** La 0.1.0 ne peut pas se mettre à jour : la
   première version avec updater (0.2.0 proposée) s'installe manuellement une
   fois. La mise à jour publique est activée quand le dépôt est public (sinon
   l'endpoint répond 404) et, autant que possible, après 5.6 (installeur signé
   Authenticode) pour éviter SmartScreen à chaque mise à jour. Si la 0.2.0
   précède 5.6, elle est marquée beta/prérelease et les notes disent que
   l'installeur n'est pas encore signé.

8. **Tests.** La chaîne est prouvée sur une VM Windows propre ou un compte
   Windows dédié, jamais sur la bibliothèque de l'utilisateur :
   `0.2.0-rc.1` → `0.2.0-rc.2` via une prérelease, puis `0.2.0` → version
   suivante en production. Tests négatifs : installeur altéré, signature
   altérée, version inférieure, endpoint injoignable. Aucun de ces cas ne doit
   installer.

## Architecture proposée

```text
tag v0.2.0
  build moteur + application
  MSI + NSIS non signés
  └─ SignPath (Authenticode) ─────────────► installeurs signés
       └─ tauri signer sign ──────────────► .sig (minisign, sur les octets signés)
            └─ latest.json { version, pub_date,
                 platforms.windows-x86_64 { signature, url } }
                 └─ GitHub Release (asset latest.json + installeur)
                      └─ application : GET latest.json (HTTPS, ≤ 1/24 h)
                           └─ vérifie la signature minisign, puis exécute l'installeur
                                └─ Windows vérifie Authenticode (SignPath)
```

- `latest.json` : `version` SemVer, `pub_date` RFC 3339,
  `platforms.windows-x86_64.signature` = **contenu** du `.sig` (pas un chemin),
  `url` = URL HTTPS de l'installeur NSIS signé.
- `plugins.updater.windows.installMode: "passive"` : petite fenêtre de
  progression, aucune interaction ; c'est le défaut et le mode recommandé.
  `quiet` est écarté : il ne peut pas demander l'élévation et donne moins de
  retour.
- NSIS en mode `currentUser` (défaut confirmé) pour éviter UAC ; le fixer
  explicitement et le tester.
- Sous Windows, l'updater ferme l'application avant d'installer. Le hook
  `on_before_exit` vide la file de téléchargements et journalise ; l'interface
  refuse l'installation si la file est active.
- Pas de rollback automatique : si l'installation échoue, l'utilisateur
  réinstalle la version précédente ; les données `%APPDATA%\Tatami` sont
  séparées. Une version fautive se corrige par une version **supérieure** ; on
  ne réutilise jamais un numéro de version.

## Alternatives étudiées

1. **Mise à jour manuelle seulement (statu quo).** Simple, aucun secret, aucun
   CI. Rejetée : l'utilisateur doit retélécharger 50–60 Mo à chaque version, et
   la 0.1.0 est déjà pénible à mettre à jour.
2. **Updater officiel Tauri v2 (retenu).** Standard, signature minisign,
   `latest.json`, GitHub Releases. Coût : une clé à garder, un workflow de
   release, deux signatures.
3. **Updater maison** (télécharger l'installeur, vérifier un SHA-256 signé,
   lancer). Plus de contrôle, mais réinvente la vérification de signature, la
   progression, la fermeture propre et la compatibilité Windows ; plus de
   surface d'erreur pour aucun gain en v1.
4. **Microsoft Store / winget.** Le Store gère les mises à jour, mais
   l'application n'y est pas et winget demande une action de l'utilisateur ;
   hors périmètre v1.
5. **SignPath seul, sans clé Tauri.** Ne satisfait pas l'updater : l'application
   doit vérifier l'octet avant de l'exécuter ; Authenticode est vérifié par
   Windows au lancement, pas par l'updater. Rejetée.
6. **Clé Tauri seule, sans SignPath.** Fonctionne, mais SmartScreen avertit à
   chaque mise à jour ; acceptable en beta, pas comme état public. SignPath
   reste la voie gratuite (5.6).

## Conséquences

- **Positives** : une seule action pour mettre à jour, plus de retéléchargement
  manuel, intégrité vérifiée par minisign, workflow reproductible et signé.
- **Négatives** : dépendance à GitHub Releases et à HTTPS ; gestion d'une clé
  privée dont la perte bloque les mises à jour ; workflow de release plus long
  (SignPath + re-signature) ; pas de rollback automatique ; première mise à
  jour manuelle depuis la 0.1.0 ; Windows uniquement.
- **Confidentialité** : une requête GitHub au plus par 24 h (IP et agent
  utilisateur), désactivable ; à déclarer dans `CODE_SIGNING.md`.
- **Exploitation** : chaque release exige une approbation manuelle SignPath et
  un environnement GitHub protégé ; une release ne part pas si une signature
  est invalide.
- **Hors périmètre** : la mise à jour du moteur reste la CLI `tatami-moteur`
  avec manifeste signé fourni par l'utilisateur ; elle n'est pas concernée par
  cette décision.

## Décisions ouvertes (à valider)

1. Implémentation Rust (recommandée) ou API JS du plugin ?
2. Vérification automatique activée par défaut (recommandée) ou désactivée par
   défaut ?
3. 0.2.0 publique après 5.6 (recommandé) ou avant, en beta/prérelease non
   signée ?
4. Téléchargement + installation en une action (recommandé) ou téléchargement
   en arrière-plan puis installation sur clic ?
