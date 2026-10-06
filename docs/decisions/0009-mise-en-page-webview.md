# 0009 — Mise en page du texte : dans la webview

- Date : 2026-10-06
- Statut : **proposée**, à valider par l'humain

## Contexte

Le texte français doit être posé **dans la bulle**, ajusté à sa forme, et ne
doit **jamais** déborder (exigence n°1 du plan, seuil S1 = 0 %).

La question : où se fait l'algorithme de remplissage — dans le moteur de rendu
de la webview (JavaScript/CSS) ou dans le cœur Rust ?

Les deux candidats sont réels. Rust a la logique, les tests et la mémoire. La
webview a **le moteur de rendu qui affichera effectivement le texte** : c'est
lui qui connaît les métriques réelles de la police, les ligatures, le
shaping, les césures réelles.

## Décision

**La mise en page se fait dans la webview, et les retours à la ligne sont
calculés puis stockés explicitement dans le JSON `PageTranslation`.**

Rust reste propriétaire de tout ce qui est **décision** (découper les blocs,
choisir la taille de police cible, gérer le budget de caractères, marquer
`ok`/`review`/`fallback`). La webview ne fait que **mesurer et rendre**.

## Pourquoi

1. **On mesure avec le moteur qui affichera.** Une largeur calculée par Rust
   avec ses propres métriques de police est une **approximation**. Le seul
   juge est le moteur de rendu réel, avec ses tables de chasse, ses ligatures
   et ses règles de césure. Implémenter ça en Rust, c'est réécrire un moteur de
   mise en page — pour un résultat moins bon.
2. **Le rendu déterministe reste garanti** : les retours à la ligne ne sont pas
   recalculés à chaque affichage. Ils sont **calculés une fois**, inscrits dans
   le JSON, et rejoués tels quels. Redimensionner la fenêtre ou changer de zoom
   ne change donc pas le découpage des lignes ; seule l'échelle change.
3. **Le cache reste valable.** La clé de cache (plan §2.5) inclut la version du
   pipeline et les versions des modèles. Si la mise en page dépendait du moteur
   de rendu, il faudrait aussi y inclure sa version — sinon un changement de
   police ou de WebView2 invaliderait des pages sans raison.

## Alternatives étudiées

- **Tout dans Rust** (largeur estimée, retour à la ligne calculé par Rust) :
  testable et déterministe, mais la mesure est une **estimation**. Le seuil
  S1 vaut **0 %**, et une estimation ne peut pas garantir 0 %. Rejeté pour
  l'exigence elle-même.
- **Tout dans la webview** (y compris les décisions) : simple, mais la logique
  de rétrécissement (T3.4), le glossaire et le budget de caractères sont alors
  hors du cœur, donc **non couverts par les tests Rust** et non pilotables par
  l'orchestrateur. Rejeté.

## Conséquences

- **Le contrôle géométrique est réparti**, et c'est un vrai coût : une
  **mesure** du texte doit remonter de la webview vers Rust avant de pouvoir
  marquer un bloc `ok`. Il faut donc un canal de mesure (une commande ou un
  événement), pas seulement un rendu.
- **Le seuil S1 exige ce canal.** Sans mesure remontée, on ne peut pas prouver
  « 0 % de débordement » : on ne pourrait que le croire. C'est une condition
  de T4.4, pas un détail.
- Les tests de propriétés de T4.3 (texte qui ne sort jamais du masque) sont
  alors **dépendants du moteur de rendu** : à écrire avec une page de test
  réelle, pas seulement en logique pure.
- Une **divergence de rendu** est possible entre la mesure et l'affichage si la
  police police n'est pas chargée au moment de la mesure. À traiter
  explicitement (`document.fonts.ready` avant toute mesure).

## Ce que cette décision ne tranche pas

Le **seuil S7** (part de `review` et `fallback`) : à fixer après les mesures du
corpus, comme indiqué dans `protocole.md` §4. Fixer ce chiffre maintenant
serait l'inventer.