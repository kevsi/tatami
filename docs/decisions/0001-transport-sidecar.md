# 0001 — Transport Rust ↔ moteur : NDJSON sur stdio
- Date : 2026-10-02
- Statut : acceptée
## Contexte
Le cœur Rust supervise le sidecar JVM (le lance, détecte les crashs, le relance) et est son seul client
(AGENTS.md §4). Il faut un transport local simple, sans état, qui ne bloque pas les décisions de packaging
(moteur remplaçable indépendamment de l'app, JAR mis à jour séparément du runtime). Les échanges sont des
appels discrets (`hello`, `search`, `fetchImage`…) avec timeout et annulation, plus des erreurs typées
(`CHALLENGE_REQUIRED` → fenêtre de résolution 0.11). Volume : métadonnées JSON petites ; images en base64
dans `fetchImage` v0 (quelques Mo par page au pire).
## Décision
**NDJSON sur stdio** : chaque requête = une ligne JSON sur stdin, chaque réponse = une ligne JSON sur stdout.
**stdout est réservé au protocole**, tous les logs partent sur stderr. Poignée de main `hello`
(`protocolVersion`, `engineVersion`, `parsersVersion`) ; Rust refuse si le MAJEUR diffère.
Schéma versionné dans `contract/` : toute évolution cassante = MAJEUR + 1 et nouvelle ADR.
## Alternatives étudiées
- **HTTP local + jeton** (ex. `127.0.0.1:PORT` éphémère, `Authorization: Bearer <jeton jetable>` passé en argv) :
  plus outillé (timeouts, annulation et streaming triviaux, debug au navigateur), mais ouvre un port local
  (surface d'attaque + conflits de ports + pare-feu Windows qui demande), et exige de gérer le cycle de vie
  du jeton. À reconsidérer si : les images base64 saturent le pipe, ou si le debug réseau devient quotidien.
- **gRPC / protobuf** : binaire, typé, streaming ; mais chaîne de génération supplémentaire (protoc, plugins
  Kotlin + Rust), sur-ingénierie pour ~10 méthodes JSON. Rejeté pour la v1.
- **Framing binaire maison sur stdio** : aucun bénéfice face à NDJSON pour nos volumes, debug moins lisible. Rejeté.
## Conséquences
- Moteur 0.6 : boucle stdin → dispatch → stdout, stderr = logs ; jamais un `println` de debug sur stdout
  (une ligne parasite invalide le flux — test de contrat là-dessus en 5.3).
- `fetchImage` base64 : si une source sert des pages > ~10 Mo, le pipe et la RAM du message gonflent ;
  repli prévu = chemins de fichiers temp + ADR avant de changer.
- Supervision Rust simplifiée : crash = EOF sur stdout ; relance = respawn + `hello`.
- Sécurité : aucune socket, aucun port ; le moteur n'est joignable que par son parent (stdin hérité, non déplaçable).
