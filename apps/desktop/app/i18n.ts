'use client';

// i18n minimal, sans dépendance.
//
// Trois garanties, et c'est tout le but de ce fichier :
//  1. **Exhaustivité vérifiée par le compilateur.** `EN` est typé
//     `Record<CleFr, string>` : si une clé française n'a pas de traduction
//     anglaise, `pnpm typecheck` échoue. Impossible d'oublier une clé en
//     prod, contrairement à un dictionnaire `JSON` que rien ne vérifie.
//  2. **Aucune clé morte.** Une clé que rien n'affiche est un mensonge : elle
//     compte dans ledictionnaire, ne se voit nulle part, et donne l'impression
//     que l'écran est traduit. `pnpm i18n:check` échoue si une clé n'est
//     référencée par aucun `t()` dans `page.tsx`. Le type des arguments de `t()`
//     est volontairement `string` : une clé absente devient un **erreur de
//     compilation**, pas un repli silencieux sur une chaîne vide.
//  3. **Conversion possible par écrans.** `t()` retombe sur le français quand
//     une clé manque dans la langue active, donc convertir l'interface écran
//     par écran ne casse jamais l'application : le reste reste en français.
//
// La langue est une simple préférence d'affichage : elle vit dans
// `localStorage` (AGENTS.md §6, qui réserve SQLite aux données métier).

import { useSyncExternalStore } from 'react';

export type Lang = 'fr' | 'en';

const LANG_KEY = 'tatami.lang';

const FR = {
  // --- Navigation
  'nav.accueil': 'Accueil',
  'nav.catalogues': 'Catalogues',
  'nav.telechargements': 'Téléchargements',
  'nav.parametres': 'Paramètres',
  'app.nom': 'Tatami',

  // --- Tri des titres d'une source
  'tri.source': 'Ordre du site',
  'tri.az': 'A → Z',
  'tri.za': 'Z → A',

  // --- Réglages : contenu
  'reglages.telechargement': 'Téléchargement',
  'reglages.catalogues': 'Catalogues affichés',
  'reglages.rating.suggestive': 'Suggestive — défaut, exclut le contenu adulte',
  'reglages.rating.safe': 'Safe — plusieurs sources renvoient alors zéro résultat',
  'reglages.rating.all': 'Toutes — aucun filtre',
  'reglages.contenu': 'Contenu',
  'reglages.contenu.affiche': 'Contenu affiché',
  'reglages.contenu.note':
    'Une seule classification à la fois : les sources n’en acceptent pas plusieurs. Les sources qui n’exposent pas de classification ne sont pas filtrées.',

  // --- Réglages : catégories
  'reglages.categories': 'Catégories de favoris',
  'reglages.categories.note':
    'Un favori appartient à une seule catégorie. Supprimer une catégorie ne supprime pas ses favoris : ils reviennent en « sans catégorie ».',
  'reglages.categories.vide': 'Aucune catégorie pour l’instant.',
  'reglages.categories.nouvelle': 'Nouvelle catégorie…',
  'reglages.categories.ajouter': 'Ajouter',
  'reglages.categories.supprimer': 'Supprimer',
  'reglages.categories.monter': 'Monter la catégorie {nom}',
  'reglages.categories.descendre': 'Descendre la catégorie {nom}',
  'reglages.categories.nom': 'Nom de la catégorie {nom}',
  'reglages.categories.couleur': 'Couleur de {nom}',

  // --- Réglages : réseau
  'reglages.reseau': 'Réseau',
  'reglages.reseau.delai': 'Délai avant abandon',
  'reglages.reseau.secondes': '{n} secondes',
  'reglages.reseau.note':
    'Certaines sources scannent tout leur catalogue avant de répondre (EXHENTA : 36 s). Au-delà du délai, la source est signalée hors service plutôt que de faire attendre.',

  // --- Écran « état des sources » (tâche 5.4) ---
  'etat.titre': 'État des sources',
  'etat.tout.va':
    'Aucune source en échec. Les pannes mémorisées sont effacées dès qu’une visite réussit.',
  // Pluriel et singulier séparés : mesuré le 2026-10-05, l'écran affichait
  // « 1 source(s) » et « 1 months ago » — l'anglais ne prend pas le pluriel
  // comme le français, une seule clé ne peut pas servir les deux.
  'etat.resume1': '1 source en échec lors d’une visite.',
  'etat.resume': '{n} sources en échec lors des visites.',
  'etat.sources1': '1 source',
  'etat.sources': '{n} sources',
  'etat.source': 'Source',
  'etat.code': 'Erreur',
  'etat.dernier': 'Signalée',
  'etat.reessayer': 'Tout réessayer',

  // --- Ancienneté traduite. Elle était en français en dur (`sinceFr`),
  //     contrairement à AGENTS.md §7 : mesuré le 2026-10-05, l’interface en
  //     anglais affichait « 3 mois » pour la date d’une panne. ---
  'temps.aujourdhui': 'aujourd’hui',
  'temps.instance': 'à l’instant',
  'temps.min': 'il y a {n} min',
  'temps.heure': 'il y a {n} h',
  'temps.jour': 'il y a {n} j',
  'temps.mois1': 'il y a 1 mois',
  'temps.mois': 'il y a {n} mois',
  'temps.an': 'il y a 1 an',
  'temps.ans': 'il y a {n} ans',

  // --- Réglages : langue
  'reglages.apparence': 'Apparence',
  'reglages.apparence.sombre': 'Sombre',
  'reglages.apparence.clair': 'Clair',
  'reglages.apparence.note':
    'Le lecteur reste sombre dans les deux thèmes : les pages sont majoritairement blanches, un fond clair ferait disparaître leurs bords.',
  'reglages.langue': 'Langue',
  'reglages.langue.fr': 'Français',
  'reglages.langue.en': 'English',
  'reglages.langue.note': 'Change la langue de toute l’interface. Les titres restent dans leur langue d’origine.',

  // --- Catalogue
  'catalogue.sources.disponibles': '{n} sources disponibles',
  'catalogue.sources.desactivees': '{n} désactivée{s}',
  'catalogue.rechercher': 'Rechercher un catalogue…',
  'catalogue.langue.toutes': 'Toutes',
  'catalogue.desactivees.afficher': 'Afficher les {n} source{s} désactivée{s}',
  'catalogue.reactiver.tout': 'Tout réactiver',
  'catalogue.hors.service': 'hors service',
  'catalogue.desactivees.note':
    'Les sources désactivées restent enregistrées, elles sont juste masquées.',
  'catalogue.vide.titre': 'Aucune source à afficher',
  'catalogue.vide.texte': 'Aucune source ne correspond à « {nom} ».',
  'catalogue.ouvrir': 'Ouvrir la source {nom}',
  'catalogue.desactiver': 'Désactiver la source {nom}',
  'catalogue.reactiver': 'Réactiver la source {nom}',

  // --- Liste des titres

  // --- Fiche
  'fiche.ajouter.favori': '♡ Ajouter aux favoris',
  'fiche.dans.favoris': '♥ Dans vos favoris',
  'fiche.sans.categorie': 'Sans catégorie',

  // --- Accueil
  'accueil.continuer': 'Continuer la lecture',
  'accueil.incognito': 'Incognito',
  'accueil.incognito.note':
    "Masque de l'accueil les titres classés adultes par leur source. Ils restent accessibles en recherche.",
  'accueil.lu.recemment': '{n} lu{s} récemment',
  'accueil.favori': '{n} favori{s}',
  'accueil.reprendre': 'Reprendre',
  'accueil.oublier': 'Oublier',
  'accueil.fiche.titre': 'Ouvrir la fiche de {titre}',
  'notification.fermer': 'Fermer la notification',
  'accueil.vos.signets': 'Vos signets',
  'accueil.pages.marquees': '{n} page{s} marquée{s}',
  'accueil.rouvrir': 'Rouvrir',
  'accueil.retirer': 'Retirer',
  'accueil.vos.favoris': 'Vos favoris',
  'accueil.vider.historique': 'Vider l’historique',
  'accueil.favoris.vide':
    'Aucun favori. Ouvrez un webtoon puis cliquez sur l’icône ♥.',
  'accueil.favoris.filtre.vide': 'Aucun favori dans cette catégorie.',
  'accueil.filtre.toutes': 'Toutes',
  'accueil.filtre.sans': 'Sans catégorie ({n})',
  'accueil.rien.a.reprendre': 'Rien à reprendre. Lancez une lecture depuis un catalogue, elle apparaîtra ici.',
  'accueil.oublier.titre': 'Oublier {titre} dans l’historique',
  'accueil.ouverture': 'Ouverture…',
  'accueil.page.sur': 'page {p}/{n} · {quand}',
  'catalogue.bandeau.langues': 'Toutes les langues',
  'catalogue.compte.langues':
    '{n} sources réparties en {langs} langues. Choisissez une langue pour parcourir ses sources.',
  'catalogue.vide.langue': 'Toutes les sources de cette langue sont désactivées.',
  'telechargements.verification.ouverte':
    '{source} demande une vérification : une fenêtre va s’ouvrir, confirmez-y vous-même puis relancez le téléchargement.',
  'telechargements.verification.titre': 'Vérification — {source}',
  'telechargements.verification.note':
    'Confirmez dans la fenêtre du site. Une fois fait, revenez ici et relancez le téléchargement : le cookie sera transmis automatiquement.',
  'telechargements.verification.appliquer': 'C’est fait, appliquer',
  'telechargements.verification.fermer': 'Fermer',
  'telechargements.verification.aucun.cookie':
    'Aucun cookie : la vérification n’est sans doute pas terminée.',
  'telechargements.verification.cookies':
    '{n} cookie(s) transmis à {source}. Relancez le téléchargement.',
  'telechargements.verification.erreur': 'Vérification non appliquée : {detail}',
  'telechargements.lecture.VIDE':
    'Le fichier de « {chapitre} » ne contient aucune page lisible. Lance le téléchargement de nouveau, ou ouvre le .cbz dans un autre lecteur.',
  'telechargements.lecture.horsligne': 'Hors ligne — lu dans le fichier téléchargé',
  'telechargements.lecture.lancer': 'Lire',
  'lecteur.navigation': 'Changer de page',
  'lecteur.plein.ecran': 'Plein écran',
  'lecteur.plein.ecran.erreur': 'Plein écran indisponible : {raison}',
  'telechargements.chapitres.ouvrir': 'Choisir un chapitre',
  'telechargements.chapitres.aucun': 'Aucun chapitre téléchargé pour le moment.',
  'lecteur.horsligne': 'Hors ligne',
  'telechargements.aucun': 'Aucun webtoon téléchargé pour l’instant.',
  'telechargements.recap': 'Récapitulatif des téléchargements',
  'telechargements.stat.webtoons': 'webtoon{s}',
  'telechargements.stat.chapitres': 'chapitres',
  'telechargements.stat.disque': 'sur le disque',
  'telechargements.stat.a.completer': 'à compléter',
  'telechargements.vos.webtoons': 'Vos webtoons',
  'telechargements.chapitres.compte': '{n}/{total} chapitres',
  'telechargements.file': 'File de téléchargement',
  'telechargements.file.interrompue': 'File interrompue',
  'telechargements.file.interrompue.note':
    '{n} webtoon{s} attendaient encore des chapitres. Les chapitres déjà téléchargés seront ignorés.',
  'telechargements.file.reprendre': 'Reprendre la file',
  'telechargements.file.abandonner': 'Abandonner la file',

  // --- Téléchargements
    'erreur.page': 'Page introuvable.',
  'fiche.chapitres.vide': 'Aucun chapitre disponible sur cette source.',
  'source.chapitres.vide': 'Clique un webtoon pour voir ses chapitres.',
  'source.chapitres.recents': 'Chapitres récents',
  'source.vide': 'Aucun résultat sur cette source.',
  'telechargements.dossier': 'Dossier de téléchargement',
  'telechargements.dossier.note':
    'Les chapitres sont écrits dans ce dossier, un .cbz par chapitre. Changer de dossier ne déplace rien : les téléchargements déjà faits restent dans l’ancien, et la bibliothèque n’affichera plus que ceux du nouveau.',
  'telechargements.dossier.parcourir': 'Choisir un dossier…',
  'telechargements.dossier.defaut': 'Revenir au dossier par défaut',
  'telechargements.dossier.ligne': 'Dossier de téléchargement :',
  'telechargements.dossier.change':
    'Dossier changé. Les prochains téléchargements iront là.',
  'telechargements.dossier.retour': 'Retour au dossier {chemin}',
  'telechargements.dossier.erreur': 'Dossier de téléchargement : {raison}',
  'telechargements.pause': 'Mettre en pause',
  'telechargements.pause.note':
    'Le chapitre en cours est terminé, puis la file s’arrête. Reprendre la reprendra où elle s’est arrêtée.',
  'telechargements.pause.etat': 'en pause',
  'telechargements.effacer.file': 'Effacer la file',
  'telechargements.supprimer.titre': 'Supprimer {nom} ?',
  'telechargements.supprimer.note':
    '{n} chapitre(s), {poids} seront supprimés du dossier de téléchargements. Le webtoon reste téléchargeable ensuite.',
  'telechargements.supprimer.definitif': 'Supprimer définitivement',
  'telechargements.annuler': 'Annuler',

  // --- Lecteur
  'lecteur.signet': 'Signet',
  'lecteur.signet.actif': 'Signet ✓',

  // --- Erreurs
  'erreur.lecture': 'Lecture impossible : {raison}',
  'erreur.chapitres': 'Chapitres indisponibles : {raison}',
  'erreur.signet': 'Signet : {raison}',
  'erreur.categorie': 'Catégorie : {raison}',
  'erreur.favori': 'Favori : {raison}',
  'erreur.source': 'Activation de la source impossible : {raison}',
  'erreur.reactivation': 'Réactivation impossible : {raison}',
  'erreur.purge': 'Purge impossible : {raison}',
  'erreur.delai': "{source} n'a pas répondu à temps. Site mort ou trop lent.",
  'erreur.verification': '{source} demande une vérification anti-robot, pas encore gérée.',
  'erreur.compte.requis':
    '{source} demande un compte pour ouvrir ce chapitre. Cette source n’est pas utilisable sans.',
  'erreur.surcharge': '{source} est surchargé. Réessaie dans quelques minutes.',
  'erreur.domaine': 'Le domaine de {source} n’existe plus.',
  'erreur.404': '{source} renvoie 404 : la source est hors service.',
  'erreur.403': '{source} bloque les requêtes (403).',
  'erreur.serveur': '{source} a renvoyé une erreur serveur.',
  'erreur.categories.ordre': 'Ordre des catégories : {raison}',
  'erreur.suppression': 'Suppression impossible : {raison}',
  'erreur.telechargement': 'Téléchargement : {raison}',
} as const;

type Cle = keyof typeof FR;

/**
 * Traductions anglaises. Le type `Record<Cle, string>` fait échouer
 * `pnpm typecheck` dès qu'une clé française n'a pas de traduction : c'est la
 * seule garantie qu'on ne peut pas contourner sans le voir.
 */
const EN: Record<Cle, string> = {
  'nav.accueil': 'Home',
  'nav.catalogues': 'Catalogues',
  'nav.telechargements': 'Downloads',
  'nav.parametres': 'Settings',
  'app.nom': 'Tatami',

  'tri.source': 'Site order',
  'tri.az': 'A → Z',
  'tri.za': 'Z → A',
  'reglages.telechargement': 'Download',
  'reglages.catalogues': 'Catalogues shown',
  'reglages.rating.suggestive': 'Suggestive — default, excludes adult content',
  'reglages.rating.safe': 'Safe — several sources then return zero results',
  'reglages.rating.all': 'All — no filter',
  'reglages.contenu': 'Content',
  'reglages.contenu.affiche': 'Content shown',
  'reglages.contenu.note':
    'One rating at a time: sources do not accept several. Sources that expose no rating are not filtered.',

  'reglages.categories': 'Favourite categories',
  'reglages.categories.note':
    'A favourite belongs to a single category. Deleting a category does not delete its favourites: they go back to “no category”.',
  'reglages.categories.vide': 'No category yet.',
  'reglages.categories.nouvelle': 'New category…',
  'reglages.categories.ajouter': 'Add',
  'reglages.categories.supprimer': 'Delete',
  'reglages.categories.monter': 'Move {nom} up',
  'reglages.categories.descendre': 'Move {nom} down',
  'reglages.categories.nom': 'Name of category {nom}',
  'reglages.categories.couleur': 'Colour of {nom}',

  'reglages.reseau': 'Network',
  'reglages.reseau.delai': 'Timeout',
  'reglages.reseau.secondes': '{n} seconds',
  'reglages.reseau.note':
    'Some sources scan their whole catalogue before answering (EXHENTA: 36 s). Past the timeout the source is marked as down rather than made to wait.',

  // --- Source status screen (task 5.4) ---
  'etat.titre': 'Source status',
  'etat.tout.va': 'No source has failed. Recorded failures are cleared as soon as a visit succeeds.',
  'etat.resume1': '1 source failed during a visit.',
  'etat.resume': '{n} sources failed during visits.',
  'etat.sources1': '1 source',
  'etat.sources': '{n} sources',
  'etat.source': 'Source',
  'etat.code': 'Error',
  'etat.dernier': 'Reported',
  'etat.reessayer': 'Retry all',

  // --- Translated age of an event (was hardcoded French in `sinceFr`) ---
  'temps.aujourdhui': 'today',
  'temps.instance': 'just now',
  'temps.min': '{n} min ago',
  'temps.heure': '{n} h ago',
  'temps.jour': '{n} d ago',
  'temps.mois1': '1 month ago',
  'temps.mois': '{n} months ago',
  'temps.an': '1 year ago',
  'temps.ans': '{n} years ago',

  'reglages.apparence': 'Appearance',
  'reglages.apparence.sombre': 'Dark',
  'reglages.apparence.clair': 'Light',
  'reglages.apparence.note':
    'The reader stays dark in both themes: pages are mostly white, and a light background would make their edges disappear.',
  'reglages.langue': 'Language',
  'reglages.langue.fr': 'Français',
  'reglages.langue.en': 'English',
  'reglages.langue.note':
    'Changes the language of the whole interface. Titles stay in their original language.',

  'catalogue.sources.disponibles': '{n} sources available',
  'catalogue.sources.desactivees': '{n} disabled',
  'catalogue.rechercher': 'Search a catalogue…',
  'catalogue.langue.toutes': 'All',
  'catalogue.desactivees.afficher': 'Show the {n} disabled source{s}',
  'catalogue.reactiver.tout': 'Enable all',
  'catalogue.hors.service': 'down',
  'catalogue.desactivees.note':
    'Disabled sources stay recorded, they are only hidden.',
  'catalogue.vide.titre': 'No source to show',
  'catalogue.vide.texte': 'No source matches “{nom}”.',
  'catalogue.ouvrir': 'Open source {nom}',
  'catalogue.desactiver': 'Disable source {nom}',
  'catalogue.reactiver': 'Enable source {nom}',


  'fiche.ajouter.favori': '♡ Add to favourites',
  'fiche.dans.favoris': '♥ In your favourites',
  'fiche.sans.categorie': 'No category',

  'accueil.rien.a.reprendre':
    'Nothing to resume. Start reading from a catalogue and it will show up here.',
  'accueil.oublier.titre': 'Forget {titre} in the history',
  'accueil.ouverture': 'Opening…',
  'accueil.page.sur': 'page {p}/{n} · {quand}',
  'catalogue.bandeau.langues': 'All languages',
  'catalogue.compte.langues':
    '{n} sources across {langs} languages. Pick a language to browse its sources.',
  'catalogue.vide.langue': 'Every source in this language is disabled.',
  'telechargements.verification.ouverte':
    '{source} asks for a verification: a window will open, confirm there yourself then retry the download.',
  'telechargements.verification.titre': 'Verification — {source}',
  'telechargements.verification.note':
    'Confirm in the site window. Once done, come back and retry the download: the cookie is sent automatically.',
  'telechargements.verification.appliquer': 'Done, apply',
  'telechargements.verification.fermer': 'Close',
  'telechargements.verification.aucun.cookie':
    'No cookie: the verification is probably not finished.',
  'telechargements.verification.cookies':
    '{n} cookie(s) sent to {source}. Retry the download.',
  'telechargements.verification.erreur': 'Verification not applied: {detail}',
  'telechargements.lecture.VIDE':
    'The file for “{chapitre}” contains no readable page. Download it again, or open the .cbz in another reader.',
  'telechargements.lecture.horsligne': 'Offline — read from the downloaded file',
  'telechargements.lecture.lancer': 'Read',
  'lecteur.navigation': 'Change page',
  'lecteur.plein.ecran': 'Fullscreen',
  'lecteur.plein.ecran.erreur': 'Fullscreen unavailable: {raison}',
  'telechargements.chapitres.ouvrir': 'Choose a chapter',
  'telechargements.chapitres.aucun': 'No chapter downloaded yet.',
  'lecteur.horsligne': 'Offline',
  'telechargements.aucun': 'No webtoon downloaded yet.',
  'telechargements.recap': 'Downloads summary',
  'telechargements.stat.webtoons': 'webtoon{s}',
  'telechargements.stat.chapitres': 'chapters',
  'telechargements.stat.disque': 'on disk',
  'telechargements.stat.a.completer': 'to finish',
  'telechargements.vos.webtoons': 'Your webtoons',
  'telechargements.chapitres.compte': '{n}/{total} chapters',
  'telechargements.file': 'Download queue',
  'telechargements.file.interrompue': 'Interrupted queue',
  'telechargements.file.interrompue.note':
    '{n} webtoon{s} were still waiting for chapters. Already downloaded chapters will be skipped.',
  'telechargements.file.reprendre': 'Resume the queue',
  'telechargements.file.abandonner': 'Abandon the queue',
  'accueil.continuer': 'Continue reading',
  'accueil.incognito': 'Incognito',
  'accueil.incognito.note':
    'Hides titles their source rates as adult. They stay reachable through search.',
  'accueil.lu.recemment': '{n} recently read',
  'accueil.favori': '{n} favourite{s}',
  'accueil.reprendre': 'Resume',
  'accueil.oublier': 'Forget',
  'accueil.fiche.titre': 'Open the details for {titre}',
  'notification.fermer': 'Close the notification',
  'accueil.vos.signets': 'Your bookmarks',
  'accueil.pages.marquees': '{n} page{s} marked',
  'accueil.rouvrir': 'Open',
  'accueil.retirer': 'Remove',
  'accueil.vos.favoris': 'Your favourites',
  'accueil.vider.historique': 'Clear history',
  'accueil.favoris.vide': 'No favourite yet. Open a webtoon then click the ♥ icon.',
  'accueil.favoris.filtre.vide': 'No favourite in this category.',
  'accueil.filtre.toutes': 'All',
  'accueil.filtre.sans': 'No category ({n})',

    'erreur.page': 'Page not found.',
  'fiche.chapitres.vide': 'No chapter available on this source.',
  'source.chapitres.vide': 'Click a webtoon to see its chapters.',
  'source.chapitres.recents': 'Recent chapters',
  'source.vide': 'No result on this source.',
  'telechargements.dossier': 'Downloads folder',
  'telechargements.dossier.note':
    'Chapters are written to this folder, one .cbz per chapter. Changing it moves nothing: existing downloads stay where they are, and the library will only show the ones in the new folder.',
  'telechargements.dossier.parcourir': 'Choose a folder…',
  'telechargements.dossier.defaut': 'Back to the default folder',
  'telechargements.dossier.ligne': 'Downloads folder:',
  'telechargements.dossier.change': 'Folder changed. New downloads will go there.',
  'telechargements.dossier.retour': 'Back to {chemin}',
  'telechargements.dossier.erreur': 'Downloads folder: {raison}',
  'telechargements.pause': 'Pause',
  'telechargements.pause.note':
    'The current chapter finishes, then the queue stops. Resuming continues where it left off.',
  'telechargements.pause.etat': 'paused',
  'telechargements.effacer.file': 'Clear the queue',
  'telechargements.supprimer.titre': 'Delete {nom}?',
  'telechargements.supprimer.note':
    '{n} chapter(s), {poids} will be deleted from the downloads folder. The webtoon stays downloadable.',
  'telechargements.supprimer.definitif': 'Delete permanently',
  'telechargements.annuler': 'Cancel',

  'lecteur.signet': 'Bookmark',
  'lecteur.signet.actif': 'Bookmark ✓',

  'erreur.lecture': 'Cannot read: {raison}',
  'erreur.chapitres': 'Chapters unavailable: {raison}',
  'erreur.signet': 'Bookmark: {raison}',
  'erreur.categorie': 'Category: {raison}',
  'erreur.favori': 'Favourite: {raison}',
  'erreur.source': 'Cannot enable source: {raison}',
  'erreur.reactivation': 'Cannot re-enable: {raison}',
  'erreur.purge': 'Cannot clear: {raison}',
  'erreur.delai': '{source} did not answer in time. Site down or very slow.',
  'erreur.verification': '{source} asks for an anti-bot check, not handled yet.',
  'erreur.compte.requis':
    '{source} asks for an account to open this chapter. This source is unusable without one.',
  'erreur.surcharge': '{source} is overloaded. Try again in a few minutes.',
  'erreur.domaine': 'The domain of {source} no longer exists.',
  'erreur.404': '{source} returns 404: the source is out of service.',
  'erreur.403': '{source} blocks requests (403).',
  'erreur.serveur': '{source} returned a server error.',
  'erreur.categories.ordre': 'Category order: {raison}',
  'erreur.suppression': 'Cannot delete: {raison}',
  'erreur.telechargement': 'Download: {raison}',
};

const DICTIONNAIRES: Record<Lang, Record<Cle, string>> = { fr: FR, en: EN };

// La langue courante EST l'état de `localStorage` : pas de second magasin à
// tenir à jour, donc aucune variable globale qui pourrait s'en désynchroniser.
const abonnes = new Set<() => void>();

/** La langue est une préférence d'affichage : `localStorage` suffit. */
function lireLangue(): Lang {
  if (typeof window === 'undefined') return 'fr';
  return window.localStorage.getItem(LANG_KEY) === 'en' ? 'en' : 'fr';
}

function abonner(cb: () => void): () => void {
  abonnes.add(cb);
  return () => abonnes.delete(cb);
}

/**
 * Aligne `document.documentElement.lang` sur la langue réellement stockée.
 *
 * L'attribut n'était posé que dans `setLang`, donc **jamais au démarrage** :
 * mesuré le 2026-10-05, `localStorage` portait `en` et l'attribut valait `fr`.
 * Un lecteur d'écran annonçait donc le français pendant que l'écran affichait
 * l'anglais. Même correction de fond que pour le thème : l'attribut est posé par
 * le script bloquant de `layout.tsx`, **avant** l'hydratation de React. Le poser
 * au chargement du module — ce qu'il faisait — arrive trop tôt pour React, qui
 * signalait « some attributes of the server rendered HTML didn't match ».
 * Ici on ne le fait plus qu'au changement explicite de langue.
 */
function alignerAttribut(): void {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = lireLangue();
}

/** Change la langue et prévient tous les composants. */
export function setLang(lang: Lang): void {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(LANG_KEY, lang);
  }
  // Utile pour les lecteurs d'écran et le rendu des chiffres.
  alignerAttribut();
  abonnes.forEach((cb) => cb());
}

export interface I18n {
  lang: Lang;
  setLang: (lang: Lang) => void;
  /** Traduit une clé. Les `{placeholders}` sont remplacés par `vars`. */
  t: (key: Cle, vars?: Record<string, string | number>) => string;
}

/**
 * Accès à la traduction. La langue est lue via `useSyncExternalStore` : le
 * rerendu est déclenché par l'abonnement, pas par un effet, donc un changement
 * de langue est visible immédiatement partout.
 */
export function useI18n(): I18n {
  const lang = useSyncExternalStore(abonner, lireLangue, () => 'fr' as Lang);
  const t = (key: Cle, vars?: Record<string, string | number>): string => {
    // Repli sur le français si la clé manque : convertir l'interface écran par
    // écran ne doit jamais afficher une clé brute à l'utilisateur.
    let texte = DICTIONNAIRES[lang][key] ?? FR[key] ?? key;
    if (vars) {
      for (const [nom, valeur] of Object.entries(vars)) {
        texte = texte.replaceAll(`{${nom}}`, String(valeur));
      }
    }
    return texte;
  };
  return { lang, setLang, t };
}

export type { Cle as MessageKey };
