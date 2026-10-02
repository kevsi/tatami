# Tatami (nom provisoire)

Client desktop de lecture de manga (Windows d'abord, puis Linux/macOS) avec parité fonctionnelle avec l'app mobile **Kotatsu-Redo**.

> **Avertissement :** Tatami est un navigateur. L'application n'héberge, ne stocke et ne distribue aucun contenu protégé. Les sources sont des sites tiers consultés à la demande de l'utilisateur, comme un navigateur web.

## Stack

| Couche | Techno | Rôle |
|---|---|---|
| Interface | Next.js (export statique) + TypeScript, dans la webview Tauri | Affichage uniquement |
| Cœur | Rust, Tauri v2 | Données, téléchargements, tâches de fond, sync, suivi |
| Moteur de sources | Kotlin/JVM, sidecar | Charge `kotatsu-parsers-redo`, requêtes vers les sites |

```
Interface (Next.js statique)
        │  invoke() / événements Tauri uniquement
        ▼
Cœur Rust (Tauri)  ── SQLite, téléchargements, planificateur, sync, suivi
        │  contrat versionné (NDJSON sur stdio)
        ▼
Moteur JVM (sidecar) ── kotatsu-parsers-redo ──► sites de manga
```

## Structure du dépôt

```
tatami/
  AGENTS.md  PLAN.md  README.md  LICENSE (GPLv3)
  apps/desktop/    Next.js (export statique) + src-tauri (Rust)
  engine/          Projet Gradle Kotlin : le sidecar
  contract/        Schéma JSON partagé -> types Rust et TypeScript
  scripts/         Scripts PowerShell de build (jlink, packaging)
  docs/            notes/, decisions/ (ADR), smoke-test.md, spike-phase0.md
```

Voir `PLAN.md` pour la feuille de route (phase en cours : 0).

## Licence

GPLv3 — voir `LICENSE`. Tout le code du dépôt est sous GPLv3 (imposée par les parsers).

## Crédits

- [Kotatsu](https://github.com/KotatsuApp/Kotatsu) — lecteur manga Android d'origine.
- [Kotatsu-Redo](https://github.com/Kotatsu-Redo/Kotatsu-Redo) — fork dont l'app reprend les fonctionnalités.
- [kotatsu-parsers-redo](https://github.com/Kotatsu-Redo/kotatsu-parsers-redo) — librairie de sources (1200+ sources) chargée par le moteur JVM.
