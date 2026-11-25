# Soumission Microsoft Store — Tatami

Checklist pour publier Tatami au format MSIX sur le Microsoft Store (signature prise en charge par Microsoft après certification).

## 0. Prérequis

- [ ] Compte développeur Microsoft (particulier, ~19 USD, vérification d'identité).
- [ ] Nom de l'application réservé dans le Partner Center (`Tatami`).
- [ ] Politique de confidentialité hébergée (URL publique) — obligatoire si l'app accède au réseau (c'est le cas).
- [ ] Captures d'écran (min. 1, recommandé 4-8) aux résolutions exigées (1366×768 min, 1920×1080 recommandé) pour fr-FR et en-US.
- [ ] Icône Store 300×300 (StoreLogo.png déjà présent dans `src-tauri/icons`).

## 1. Préparer le paquet

- [ ] Build Tauri : `cd apps/desktop && pnpm tauri build --config src-tauri/tauri.store.conf.json`
- [ ] Vérifier que `src-tauri/target/release/tatami-desktop.exe` et `resources/` existent.
- [ ] Éditer `scripts/msix/AppxManifest.xml` : remplacer `__PUBLISHER__`, `__PUBLISHER_DISPLAY__`, `__VERSION__` (4 champs, ex. `0.2.0.7`).
  - Le `Publisher` doit correspondre à l'identité fournie par le Partner Center après réservation du nom.
- [ ] Lancer le packaging : `.\scripts\build-msix.ps1 -Version "0.2.0.7" -Publisher "CN=..." -PublisherDisplay "..." -SkipSign`
- [ ] Paquet produit : `scripts/msix/Tatami-0.2.0.7.msix` (non signé, OK pour le Store).

## 2. Valider localement (WACK)

- [ ] Lancer le Windows App Certification Kit :
  `.\scripts\build-msix.ps1 -Version "0.2.0.7" -Publisher "CN=..." -SkipSign -RunWack`
- [ ] Ouvrir `scripts/msix/wack-report.xml` et corriger tout échec (notamment `runFullTrust` et les dépendances WebView2).
- [ ] Tester l'installation locale avec un certificat de dev si besoin :
  `.\scripts\build-msix.ps1 -Version "0.2.0.7" -Publisher "CN=..." -Sign -PfxPath "dev.pfx" -PfxPassword "..."`

## 3. Points de conformité à vérifier avant soumission

- [ ] **Full trust** : `runFullTrust` déclaré (moteur JVM sidecar). Microsoft accepte les apps full-trust desktop.
- [ ] **WebView2** : dépendance déclarée ou runtime embarqué. Sur Win10 1809+ le runtime est préinstallé.
- [ ] **Moteur JVM** : le dossier `engine/tatami-engine` est embarqué dans le paquet et les chemins relatifs fonctionnent sous MSIX (à tester).
- [ ] **Pas d'auto-mise à jour externe** : l'updater Tauri est désactivé dans `tauri.store.conf.json` (le Store gère les mises à jour).
- [ ] **Pas de téléchargement de code arbitraire** : le moteur est embarqué, les sources personnelles sont des scripts JS évalués par Nashorn (déjà le cas) — vérifier que la certification WACK ne bloque pas.
- [ ] **Réseau** : `internetClient` déclaré (sources de manga).
- [ ] **Confidentialité** : politique de confidentialité liée dans le Partner Center.

## 4. Soumission

- [ ] Partner Center → "Nouvelle soumission" → téléverser le `.msix`.
- [ ] Remplir les métadonnées (description, catégorie "Divertissement" ou "Outils", mots-clés).
- [ ] Définir les marchés (fr-FR en priorité, puis en-US).
- [ ] Soumettre pour certification (délai typique : 1 à 3 jours).
- [ ] Après certification : Microsoft signe le paquet, il devient disponible au Store.

## 5. Coexistence des deux canaux

| Canal | Format | Signature | Mise à jour |
|---|---|---|---|
| Auto-hébergé | `.msi` / `.exe` (NSIS) | Ton certificat (ou non signé) | Plugin updater Tauri |
| Microsoft Store | `.msix` | Microsoft | Store automatique |

Les deux partagent le même code source. La différence se fait au build via `--config src-tauri/tauri.store.conf.json` (Store) vs build par défaut (auto-hébergé).

## Références

- Tauri v2 — Windows code signing : https://v2.tauri.app/distribute/sign/windows/
- MSIX packaging : https://learn.microsoft.com/windows/msix/
- Windows App Certification Kit : https://learn.microsoft.com/windows/uwp/debug-test-perf/windows-app-certification-kit
- Partner Center : https://partner.microsoft.com/dashboard
