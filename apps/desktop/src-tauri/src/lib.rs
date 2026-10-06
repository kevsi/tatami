//! Bibliothèque du cœur de Tatami (cœur Rust du client desktop).
//!
//! **Pourquoi ce fichier existe.** Le crate était un pur binaire (`main.rs` avec
//! ses `mod`), donc `tests/` — les tests d'intégration — ne pouvaient voir
//! **aucun** code de production : ils ne pouvaient valider que le schéma, jamais
//! la façon dont Rust lit réellement une réponse du moteur. Or c'est
//! précisément là que sont les défauts (extension déduite du `contentType`,
//! chaîne vide interprétée comme `SAFE`, URL de page vide…). La tâche 5.3
//! veut des tests de contrat « rejoués contre le schéma **et** contre le code
//! Rust » (AGENTS.md §8) : il faut donc que `tests/` ait accès au code.
//!
//! Le découpage est donc le découpage classique Rust : la bibliothèque contient
//! le code, le binaire ne fait que l'assembler. `main.rs` garde l'assemblage
//! Tauri (contexte, gestionnaire de commandes, protocole `tatami://`).
//!
//! **Ce qui est `pub` ici l'est pour une raison mesurée**, pas par confort :
//!
//! - `commands::into_result` est le seul point où une enveloppe du moteur
//!   devient un résultat ou une erreur. C'est la fonction que les tests de
//!   contrat rejouent sur chaque réponse enregistrée.
//! - `supervisor::is_transient` décide si un échec est rejoué. Une réponse
//!   d'erreur rejouée comme transitoire ferait boucler une source morte.
//! - `supervisor::PROTOCOL_MAJOR` est la constante que `spawn_once` compare au
//!   `protocolVersion` du `hello` : c'est la porte qui refuse un moteur
//!   incompatible, donc elle doit être vérifiable.
//! - `commands::ajouter_jetons_couverture` est l'ajout de `coverToken`. Une
//!   régression ici ne casse rien à la compilation et se voit seulement à
//!   l'écran : une vignette vide.
//! - `scheduler::passe` est la décision « ce chapitre est-il nouveau ? », et
//!   `scheduler::Limites` le débit par source. Les deux sont vérifiables
//!   uniquement avec une horloge injectée, donc hors du binaire : sans `pub`,
//!   les tests ne pourraient pas mesurer un intervalle de 4,9 s.
//!
//! Le reste garde sa visibilité d'origine. Rendre `pub` une fonction « parce
//! que ça peut servir » est exactement ce qui a produit les commandes mortes
//! supprimées le 2026-10-04.

pub mod cbz;
pub mod challenge;
pub mod commands;
pub mod download;
pub mod image;
pub mod journal;
pub mod library;
pub mod moteur;
pub mod pagecache;
pub mod scheduler;
pub mod shelf;
pub mod store;
pub mod supervisor;
pub mod tray;
pub mod zipread;
