'use client';

// i18n minimal, sans dépendance.
//
// Deux garanties, et c'est tout le but de ce fichier :
//  1. **Exhaustivité vérifiée par le compilateur.** `EN` est typé
//     `Record<CleFr, string>` : si une clé française n'a pas de traduction
//     anglaise, `pnpm typecheck` échoue. Impossible d'oublier une clé en
//     prod, contrairement à un dictionnaire `JSON` que rien ne vérifie.
//  2. **Conversion possible par écrans.** `t()` retombe sur le français quand
//     une clé manque, donc convertir l'interface écran par écran ne casse
//     jamais l'application : le reste reste en français.
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

  // --- Réglages : contenu
  'reglages.contenu': 'Contenu',
  'reglages.contenu.affiche': 'Contenu affiché',
  'reglages.contenu.note':
    'Une seule classification à la fois : les sources n’en acceptent pas plusieurs. Les sources qui n’exposent pas de classification ne sont pas filtrées.',
  'reglages.rating.suggestive': 'Suggestive — défaut, exclut le contenu adulte',
  'reglages.rating.safe': 'Safe — plusieurs sources renvoient alors zéro résultat',
  'reglages.rating.all': 'Toutes — aucun filtre',

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
  'reglages.categories.couleur.n': 'Couleur {n}',
  'reglages.categories.attente': 'Créez vos catégories dans Réglages pour ranger vos favoris.',

  // --- Réglages : réseau
  'reglages.reseau': 'Réseau',
  'reglages.reseau.delai': 'Délai avant abandon',
  'reglages.reseau.secondes': '{n} secondes',
  'reglages.reseau.note':
    'Certaines sources scannent tout leur catalogue avant de répondre (EXHENTA : 36 s). Au-delà du délai, la source est signalée hors service plutôt que de faire attendre.',

  // --- Réglages : langue
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
  'catalogue.desactiver': 'Désactiver la source {nom}',
  'catalogue.reactiver': 'Réactiver la source {nom}',
  'catalogue.ouvrir': 'Ouvrir la source {nom}',
  'catalogue.voir.plus': 'Voir plus ({n} restantes)',

  // --- Liste des titres
  'source.rechercher': 'Rechercher un webtoon…',
  'source.rechercher.bouton': 'Rechercher',
  'source.choisir.un': 'Cliquez sur un webtoon pour voir ses chapitres.',
  'source.webtoons': '{n} webtoon{s}',
  'source.chargement': 'chargement…',
  'tri.libelle': 'Tri des titres',
  'tri.source': 'Ordre du site',
  'tri.az': 'A → Z',
  'tri.za': 'Z → A',
  'source.genres': '{n} genres au total',
  'source.effacer': 'Effacer ({n})',
  'source.chapitres.recents': 'Chapitres récents',
  'source.selectionner': 'Sélectionner',

  // --- Fiche
  'fiche.ajouter.favori': '♡ Ajouter aux favoris',
  'fiche.dans.favoris': '♥ Dans vos favoris',
  'fiche.categorie': 'Catégorie',
  'fiche.sans.categorie': 'Sans catégorie',
  'fiche.selectionner.chapitres': 'Sélectionner des chapitres',

  // --- Accueil
  'accueil.continuer': 'Continuer la lecture',
  'accueil.incognito': 'Incognito',
  'accueil.incognito.note':
    "Masque de l'accueil les titres classés adultes par leur source. Ils restent accessibles en recherche.",
  'accueil.lu.recemment': '{n} lu{s} récemment',
  'accueil.favori': '{n} favori{s}',
  'accueil.reprendre': 'Reprendre',
  'accueil.oublier': 'Oublier',
  'accueil.vos.signets': 'Vos signets',
  'accueil.pages.marquees': '{n} page{s} marquée{s}',
  'accueil.page': 'page {p}/{n}',
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
  'accueil.deuxiemes.titres': 'Titres dupliqués sur {source}',
  'accueil.trier.doublons': 'Doublons',
  'accueil.favori.retirer': 'Retirer {titre} des favoris',
  'accueil.favori.ajouter': 'Ajouter aux favoris',
  'catalogue.bandeau.langues': 'Toutes les langues',
  'catalogue.compte.langues':
    '{n} sources réparties en {langs} langues. Choisissez une langue pour parcourir ses sources.',
  'catalogue.vide.langue': 'Toutes les sources de cette langue sont désactivées.',
  'selection.compteur': '{n}/{total} sélectionnés',
  'telechargements.section': 'Téléchargement',
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
  'telechargements.arreter.apres': 'Arrêter après le chapitre en cours',
  'telechargements.lancer': 'Lancer la file',
  'telechargements.supprimer.webtoon': 'Supprimer {titre} ?',
  'telechargements.chapitre.erreur': 'Échec sur {titre}',
  'telechargements.emplacement': 'Dossier de téléchargement',
  'selection.chapitres': 'Sélectionner des chapitres',
  'selection.n.debut': 'N derniers',
  'selection.n.debut.legende': 'Marquer les {n} derniers chapitres',
  'selection.n.premiers': 'N premiers',
  'selection.n.premiers.legende': 'Marquer les {n} premiers chapitres',
  'selection.tout': 'Tout',
  'selection.aucun': 'Aucun',
  'selection.tout.legende': 'Tout sélectionner',
  'selection.aucun.legende': 'Tout désélectionner',
  'selection.lancer': 'Télécharger la sélection',
  'selection.vide': 'Aucun chapitre sélectionné.',
  'chapitres.filtre': 'Filtrer les chapitres',
  'chapitres.resultats': '{n} chapitre{s}',
  'chapitres.selectionnes': '{n} sélectionné{s}',
  'chapitres.pastille': '{n} chapitres récents',
  'chapitres.recent': 'Ch. {num}',
  'chapitre.sauter': 'Changer de chapitre',
  'chapitre.recent.pastille': 'Chapitres récents',
  'chapitre.selection.legende': 'Sélectionner ce chapitre',
  'selectionner.tout': 'Tout sélectionner',
  'chapitres.trier': 'Trier les chapitres',
  'chapitres.effacer.filtre': 'Effacer',
  'fiche.infos': 'Informations',
  'fiche.genres': 'Genres',
  'chapitre.selectionner.picker': 'Sélectionner ce chapitre',
  'accueil.selectionner.tout': 'Tout sélectionner',
  'accueil.deselectionner.tout': 'Tout désélectionner',
  'accueil.selection.partielle': '{n}/{total} sélectionnés',

  // --- Téléchargements
  'telechargements.vide': 'Aucun téléchargement.',
  'telechargements.chapitres': '{n} chapitre{s}',
  'telechargements.poids': '{poids}',
  'telechargements.en.cours': 'Téléchargement en cours…',
  'telechargement.echec': 'Échec du téléchargement',
  'telechargements.supprimer': 'Supprimer {titre} ?',
  'telechargements.supprimer.note':
    '{n} chapitre(s), {poids} seront supprimés du dossier de téléchargements. Le webtoon reste téléchargeable ensuite.',
  'telechargements.supprimer.definitif': 'Supprimer définitivement',
  'telechargements.annuler': 'Annuler',

  // --- Lecteur
  'lecteur.retour': '← Chapitres',
  'lecteur.signet': 'Signet',
  'lecteur.signet.actif': 'Signet ✓',
  'lecteur.signet.note': 'Marquer cette page pour la retrouver plus tard',
  'lecteur.mode': 'Mode de lecture',
  'lecteur.mode.page': 'Pagé',
  'lecteur.mode.webtoon': 'Webtoon',
  'lecteur.pages.ecran': 'Pages par écran',
  'lecteur.simple': 'Simple',
  'lecteur.double': 'Double',
  'lecteur.zoom': 'Zoom',
  'lecteur.sauter': 'Aller à la page',
  'lecteur.page.precedente': 'Page précédente',
  'lecteur.page.suivante': 'Page suivante',
  'lecteur.chargement': 'Chargement du chapitre…',

  // --- Erreurs
  'erreur.lecture': 'Lecture impossible : {raison}',
  'erreur.chapitres': 'Chapitres indisponibles : {raison}',
  'erreur.pages': 'Ce chapitre n’a aucune page lisible.',
  'erreur.chapitre.absent': 'Ce chapitre n’est plus disponible dans cette source.',
  'erreur.webtoon.absent': 'Ce webtoon n’a plus aucun chapitre lisible.',
  'erreur.signet': 'Signet : {raison}',
  'erreur.categorie': 'Catégorie : {raison}',
  'erreur.favori': 'Favori : {raison}',
  'erreur.source': 'Activation de la source impossible : {raison}',
  'erreur.reactivation': 'Réactivation impossible : {raison}',
  'erreur.purge': 'Purge impossible : {raison}',
  'erreur.categories.doublon': 'La catégorie « {nom} » existe déjà.',
  'erreur.categories.absente': 'Cette catégorie n’existe plus.',
  'erreur.categories.ordre': 'Ordre des catégories : {raison}',
  'erreur.suppression': 'Suppression impossible : {raison}',
  'erreur.verification': 'Fenêtre de vérification impossible : {raison}',
  'erreur.verification.non.appliquee': 'Vérification non appliquée : {detail}',
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

  'reglages.contenu': 'Content',
  'reglages.contenu.affiche': 'Content shown',
  'reglages.contenu.note':
    'One rating at a time: sources do not accept several. Sources that expose no rating are not filtered.',
  'reglages.rating.suggestive': 'Suggestive — default, excludes adult content',
  'reglages.rating.safe': 'Safe — several sources then return no results',
  'reglages.rating.all': 'All — no filter',

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
  'reglages.categories.couleur.n': 'Colour {n}',
  'reglages.categories.attente': 'Create your categories in Settings to sort your favourites.',

  'reglages.reseau': 'Network',
  'reglages.reseau.delai': 'Timeout',
  'reglages.reseau.secondes': '{n} seconds',
  'reglages.reseau.note':
    'Some sources scan their whole catalogue before answering (EXHENTA: 36 s). Past the timeout the source is marked as down rather than made to wait.',

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
  'catalogue.desactiver': 'Disable source {nom}',
  'catalogue.reactiver': 'Enable source {nom}',
  'catalogue.ouvrir': 'Open source {nom}',
  'catalogue.voir.plus': 'Show more ({n} remaining)',

  'source.rechercher': 'Search a webtoon…',
  'source.rechercher.bouton': 'Search',
  'source.choisir.un': 'Click a webtoon to see its chapters.',
  'source.webtoons': '{n} webtoon{s}',
  'source.chargement': 'loading…',
  'tri.libelle': 'Title order',
  'tri.source': 'Site order',
  'tri.az': 'A → Z',
  'tri.za': 'Z → A',
  'source.genres': '{n} genres in total',
  'source.effacer': 'Clear ({n})',
  'source.chapitres.recents': 'Recent chapters',
  'source.selectionner': 'Select',

  'fiche.ajouter.favori': '♡ Add to favourites',
  'fiche.dans.favoris': '♥ In your favourites',
  'fiche.categorie': 'Category',
  'fiche.sans.categorie': 'No category',
  'fiche.selectionner.chapitres': 'Select chapters',

  'accueil.rien.a.reprendre':
    'Nothing to resume. Start reading from a catalogue and it will show up here.',
  'accueil.oublier.titre': 'Forget {titre} in the history',
  'accueil.ouverture': 'Opening…',
  'accueil.page.sur': 'page {p}/{n} · {quand}',
  'accueil.deuxiemes.titres': 'Duplicate titles on {source}',
  'accueil.trier.doublons': 'Duplicates',
  'accueil.favori.retirer': 'Remove {titre} from favourites',
  'accueil.favori.ajouter': 'Add to favourites',
  'catalogue.bandeau.langues': 'All languages',
  'catalogue.compte.langues':
    '{n} sources across {langs} languages. Pick a language to browse its sources.',
  'catalogue.vide.langue': 'Every source in this language is disabled.',
  'selection.compteur': '{n}/{total} selected',
  'telechargements.section': 'Download',
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
  'telechargements.arreter.apres': 'Stop after the current chapter',
  'telechargements.lancer': 'Start the queue',
  'telechargements.supprimer.webtoon': 'Delete {titre}?',
  'telechargements.chapitre.erreur': 'Failed on {titre}',
  'telechargements.emplacement': 'Download folder',
  'selection.chapitres': 'Select chapters',
  'selection.n.debut': 'Last N',
  'selection.n.debut.legende': 'Mark the last {n} chapters',
  'selection.n.premiers': 'First N',
  'selection.n.premiers.legende': 'Mark the first {n} chapters',
  'selection.tout': 'All',
  'selection.aucun': 'None',
  'selection.tout.legende': 'Select all',
  'selection.aucun.legende': 'Deselect all',
  'selection.lancer': 'Download selection',
  'selection.vide': 'No chapter selected.',
  'chapitres.filtre': 'Filter chapters',
  'chapitres.resultats': '{n} chapter{s}',
  'chapitres.selectionnes': '{n} selected',
  'chapitres.pastille': '{n} recent chapters',
  'chapitres.recent': 'Ch. {num}',
  'chapitre.sauter': 'Switch chapter',
  'chapitre.recent.pastille': 'Recent chapters',
  'chapitre.selection.legende': 'Select this chapter',
  'selectionner.tout': 'Select all',
  'chapitres.trier': 'Order chapters',
  'chapitres.effacer.filtre': 'Clear',
  'fiche.infos': 'Information',
  'fiche.genres': 'Genres',
  'chapitre.selectionner.picker': 'Select this chapter',
  'accueil.continuer': 'Continue reading',
  'accueil.incognito': 'Incognito',
  'accueil.incognito.note':
    'Hides titles their source rates as adult. They stay reachable through search.',
  'accueil.lu.recemment': '{n} recently read',
  'accueil.favori': '{n} favourite{s}',
  'accueil.reprendre': 'Resume',
  'accueil.oublier': 'Forget',
  'accueil.vos.signets': 'Your bookmarks',
  'accueil.pages.marquees': '{n} page{s} marked',
  'accueil.page': 'page {p}/{n}',
  'accueil.rouvrir': 'Open',
  'accueil.retirer': 'Remove',
  'accueil.vos.favoris': 'Your favourites',
  'accueil.vider.historique': 'Clear history',
  'accueil.favoris.vide': 'No favourite yet. Open a webtoon then click the ♥ icon.',
  'accueil.favoris.filtre.vide': 'No favourite in this category.',
  'accueil.filtre.toutes': 'All',
  'accueil.filtre.sans': 'No category ({n})',
  'accueil.selectionner.tout': 'Select all',
  'accueil.deselectionner.tout': 'Deselect all',
  'accueil.selection.partielle': '{n}/{total} selected',

  'telechargements.vide': 'No download yet.',
  'telechargements.chapitres': '{n} chapter{s}',
  'telechargements.poids': '{poids}',
  'telechargements.en.cours': 'Downloading…',
  'telechargement.echec': 'Download failed',
  'telechargements.supprimer': 'Delete {titre}?',
  'telechargements.supprimer.note':
    '{n} chapter(s), {poids} will be deleted from the downloads folder. The webtoon stays downloadable.',
  'telechargements.supprimer.definitif': 'Delete for good',
  'telechargements.annuler': 'Cancel',

  'lecteur.retour': '← Chapters',
  'lecteur.signet': 'Bookmark',
  'lecteur.signet.actif': 'Bookmark ✓',
  'lecteur.signet.note': 'Mark this page to find it later',
  'lecteur.mode': 'Reading mode',
  'lecteur.mode.page': 'Paged',
  'lecteur.mode.webtoon': 'Webtoon',
  'lecteur.pages.ecran': 'Pages per screen',
  'lecteur.simple': 'Single',
  'lecteur.double': 'Double',
  'lecteur.zoom': 'Zoom',
  'lecteur.sauter': 'Go to page',
  'lecteur.page.precedente': 'Previous page',
  'lecteur.page.suivante': 'Next page',
  'lecteur.chargement': 'Loading chapter…',

  'erreur.lecture': 'Cannot read: {raison}',
  'erreur.chapitres': 'Chapters unavailable: {raison}',
  'erreur.pages': 'This chapter has no readable page.',
  'erreur.chapitre.absent': 'This chapter is no longer available in this source.',
  'erreur.webtoon.absent': 'This webtoon no longer has any readable chapter.',
  'erreur.signet': 'Bookmark: {raison}',
  'erreur.categorie': 'Category: {raison}',
  'erreur.favori': 'Favourite: {raison}',
  'erreur.source': 'Cannot enable source: {raison}',
  'erreur.reactivation': 'Cannot re-enable: {raison}',
  'erreur.purge': 'Cannot clear: {raison}',
  'erreur.categories.doublon': 'Category “{nom}” already exists.',
  'erreur.categories.absente': 'This category no longer exists.',
  'erreur.categories.ordre': 'Category order: {raison}',
  'erreur.suppression': 'Cannot delete: {raison}',
  'erreur.verification': 'Cannot open the verification window: {raison}',
  'erreur.verification.non.appliquee': 'Verification not applied: {detail}',
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

/** Change la langue et prévient tous les composants. */
export function setLang(lang: Lang): void {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(LANG_KEY, lang);
    // Utile pour les lecteurs d'écran et le rendu des chiffres.
    document.documentElement.lang = lang;
  }
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