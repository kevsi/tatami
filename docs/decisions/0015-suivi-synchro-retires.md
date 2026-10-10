# 0015 — Retrait du suivi externe et de la synchronisation cross-device du périmètre v1
- Date : 2026-10-10
- Statut : acceptée

## Contexte
La v1 de Tatami vise la parité fonctionnelle avec Kotatsu-Redo pour l'usage quotidien de lecture. Le suivi externe (AniList, MAL, Shikimori, Kitsu) et la synchronisation cross-device étaient listés dans le périmètre v1 initial mais aucune ligne de code n'existe pour ces deux fonctionnalités.

## Décision
Ces deux fonctionnalités sont retirées du périmètre v1 :
- Le suivi externe exige OAuth2 par service, une table de correspondance d'identifiants, et une gestion des conflits — effort estimé : plusieurs semaines.
- La synchronisation cross-device exige un serveur ou un protocole pair-à-pair, une résolution de conflits CRDT ou LWW, et une gestion des jetons — effort estimé : plusieurs semaines.

L'application est utilisable sans ces deux fonctionnalités pour l'usage principal (lire, télécharger, gérer une bibliothèque).

## Alternatives étudiées
- Implémenter en v1 : écarté, trop coûteux pour l'utilisateur cible qui n'utilise pas ces services.
- Reporter en v1.1 : retenu. Ces fonctionnalités peuvent être ajoutées sans modifier l'architecture existante.

## Conséquences
- PLAN.md est mis à jour pour retirer ces items du périmètre.
- Le README sera mis à jour pour ne pas les promettre.
- Aucun code à supprimer (aucune ligne n'existait).
