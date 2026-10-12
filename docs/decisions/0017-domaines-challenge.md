# 0017 — Domaines déclarés pour les fenêtres de challenge
- Date : 2026-10-11
- Statut : acceptée

## Contexte

`open_challenge` reçoit une URL distante et la charge dans une fenêtre WebView. Rust ne recevait
jusqu'ici aucun domaine de référence associé au nom de source ; la validation HTTP(S) seule ne
permettait donc pas de vérifier que l'URL venait bien de la source concernée.

## Décision

Étendre `listSources` de manière additive avec `domains?`, un tableau optionnel d'hôtes normalisés.
Pour les parsers natifs, le moteur parse le domaine courant et les valeurs préconfigurées de
`ConfigKey.Domain` comme des URL : il ajoute `https://` si aucun schéma n'est présent, puis ne
conserve que l'hôte. Les identifiants et les hôtes invalides sont refusés ; port, chemin, requête
et fragment sont ignorés. Les noms internationalisés sont convertis en ASCII, puis les hôtes sont
mis en minuscules et dédupliqués. Pour les sources scriptées, le moteur expose l'hôte HTTP(S)
déclaré dans `baseUrl`. Le contrat passe à `0.2` ; la compatibilité reste définie par le même
numéro majeur.

Rust conserve les domaines de la dernière réponse `listSources`. `open_challenge` n'accepte que
l'hôte déclaré ou l'un de ses sous-domaines, refuse les URL avec identifiants et renvoie
`CHALLENGE_DOMAIN_UNAVAILABLE` si la source ou son domaine n'est pas connu. Le front traduit cette
erreur. Les redirections et la navigation ultérieure dans la fenêtre ne sont pas contrôlées par
cette décision.

## Alternatives étudiées

- **Continuer à accepter toute URL HTTP(S)** : rejeté, car une réponse du moteur pourrait ouvrir
  une page distante sans rapport avec la source.
- **Faire transmettre les domaines par le front avec chaque appel `open_challenge`** : rejeté,
  car Rust ferait confiance à la même couche qui fournit l'URL à valider.
- **Ajouter une requête dédiée au moteur pour chaque challenge** : rejeté, car `listSources`
  fournit déjà les domaines de toutes les sources et évite un aller-retour supplémentaire.

## Conséquences

- `listSources` reste compatible avec un moteur qui n'émet pas `domains` : le challenge est alors
  refusé explicitement et localisé, plutôt que d'ouvrir une URL non vérifiée.
- Une valeur native contenant un chemin n'élargit l'autorisation qu'à son hôte parsé, jamais au
  chemin ni à la chaîne complète.
- La validation ne suit pas les redirections ; cette limite reste à traiter séparément.
