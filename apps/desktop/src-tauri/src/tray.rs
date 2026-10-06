//! Tâche 3.4 — icône dans la zone de notification, et fermeture qui cache la
//! fenêtre au lieu de quitter.
//!
//! ## Pourquoi c'est **Rust** qui crée l'icône, et pas l'interface
//!
//! « Fermer met dans la zone de notification » et « l'icône vit dans la zone »
//! sont le même geste : si l'icône pouvait ne pas exister, fermer la fenêtre
//! rendrait l'application **injoignable**. Une icône créée par la webview
//! dépend donc du bon fonctionnement de la webview — exactement la dépendance
//! qu'on ne peut pas accepter ici. Rust la crée dans `setup()`, avant que la
//! première requête ne parte.
//!
//! ## Pourquoi les libellés viennent de l'interface
//!
//! Même raison que les motifs d'import (2.4) et le texte des notifications
//! (3.3) : du texte écrit dans Rust s'affiche tel quel dans les deux langues. Le
//! menu est donc créé avec des libellés français provisoires — visibles si la
//! webview n'a pas encore répondu — et l'interface les remplace dès qu'elle
//! connaît la langue, par la commande [`set_libelles`].
//!
//! ## Le drapeau [`QUITTER`]
//!
//! Fermer la fenêtre ne quitte pas, donc « Quitter » doit passer **outre** cette
//! interception. Un drapeau suffit, et il est indispensable : sans lui,
//! `exit(0)` depuis le menu serait traité comme une fermeture et
//! l'application ne partirait jamais.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

/// L'utilisateur a demandé à quitter : la prochaine fermeture est réelle.
static QUITTER: AtomicBool = AtomicBool::new(false);

/// Le tray a-t-il été créé **avec** une icône ?
///
/// Vrai seulement dans ce cas. `TrayIconBuilder::icon()` attend une `Image` et
/// ne fait **rien** si on ne lui en donne pas : le tray existe alors, sans rien
/// à cliquer. Masquer la fenêtre dans ce cas rendrait l'application
/// injoignable — c'est le seul scénario vraiment grave de cette tâche, donc il
/// est verrouillé par [`doit_cacher`] et non par une hope.
static TRAY_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Identifiants des entrées du menu. Ce sont des **contrats** avec
/// [`set_libelles`] : les libellés changent, les identifiants non.
const ID_OUVRIR: &str = "ouvrir";
const ID_QUITTER: &str = "quitter";

/// Les entrées du menu, gardées pour pouvoir les **renommer**.
///
/// `MenuItem` est retrouvable par identifiant, mais le chercher à chaque
/// changement de langue coûterait une traversée de l'arbre de menus ; les garder
/// rend la commande triviale.
pub struct MenuZone {
    ouvrir: MenuItem<tauri::Wry>,
    quitter: MenuItem<tauri::Wry>,
}

/// Marque l'application comme devant vraiment quitter.
///
/// Appelé par « Quitter » : le drapeau est posé **avant** `exit(0)`, sinon la
/// fermeture est réinterprétée comme une mise dans la zone et rien ne se passe.
pub fn marquer_quitter() {
    QUITTER.store(true, Ordering::SeqCst);
}

/// Vrai si l'application est en train de quitter vraiment.
pub fn en_cours_de_quitter() -> bool {
    QUITTER.load(Ordering::SeqCst)
}

/// Le tray est-il réellement cliquable ?
pub fn tray_visible() -> bool {
    TRAY_VISIBLE.load(Ordering::SeqCst)
}

/// Faut-il transformer une fermeture en « mise dans la zone » ?
///
/// La règle tient en une phrase : **on ne masque la fenêtre que si l'utilisateur
/// a un moyen de la ramener.** Trois issues, donc trois cas refusés :
/// l'utilisateur quitte, il n'y a pas de tray, ou le tray n'a pas d'icône.
/// Extraire la décision dans une fonction pure la rend testable — c'est
/// exactement la décision qu'on ne peut pas se permettre de laisser au milieu
/// d'un gestionnaire d'évènement.
pub fn doit_cacher(quitte: bool, tray_visible: bool) -> bool {
    !quitte && tray_visible
}

/// Un libellé est exploitable s'il reste quelque chose à afficher.
fn libelle_exploitable(texte: &str) -> bool {
    !texte.trim().is_empty()
}

/// Ramène la fenêtre devant l'utilisateur.
///
/// Les trois étapes sont nécessaires et dans cet ordre : une fenêtre cachée ou
/// minimisée ne réapparaît pas sur un seul `set_focus`.
pub fn montrer(app: &AppHandle) {
    let Some(fenetre) = fenetre_principale(app) else {
        tracing::warn!("fenetre introuvable : rien a montrer");
        return;
    };
    if let Err(e) = fenetre.unminimize() {
        tracing::warn!("restauration de la fenetre impossible : {e}");
    }
    if let Err(e) = fenetre.show() {
        tracing::warn!("affichage de la fenetre impossible : {e}");
    }
    let _ = fenetre.set_focus();
}

/// La fenêtre principale, ou la première venue.
///
/// `tauri.conf.json` ne fixe pas de `label`, donc Tauri en attribue un ; mais
/// s'appuyer sur ce nom casserait le jour où quelqu'un en fixe un. On tente
/// donc « main », puis on prend la première.
fn fenetre_principale(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())
}

/// Crée l'icône de zone et son menu. À appeler dans `setup()`.
///
/// Ne **échoue pas** si l'icône de fenêtre par défaut est absente : dans ce
/// cas on ne crée pas de tray, [`TRAY_VISIBLE`] reste faux, et la fermeture
/// reprend son comportement normal. Mieux vaut une application qui ferme
/// qu'une application introuvable.
pub fn construire(app: &tauri::App) -> tauri::Result<()> {
    let Some(icon) = app.default_window_icon().cloned() else {
        tracing::warn!(
            "aucune icone de fenetre par defaut : pas de zone de notification, \
             la fermeture quittera normalement"
        );
        return Ok(());
    };

    // Libellés provisoires en français. L'interface les remplace ; ils ne sont
    // visibles que le temps que la webview réponde, et garantissent qu'un clic
    // droit sur l'icône n'ouvre pas un menu vide.
    let ouvrir = MenuItem::with_id(app, ID_OUVRIR, "Ouvrir Tatami", true, None::<&str>)?;
    let separateur = PredefinedMenuItem::separator(app)?;
    let quitter = MenuItem::with_id(app, ID_QUITTER, "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&ouvrir, &separateur, &quitter])?;

    TrayIconBuilder::with_id("tatami")
        .icon(icon)
        .tooltip("Tatami")
        .menu(&menu)
        // Clic gauche = montrer la fenêtre, donc le menu ne doit pas s'ouvrir au
        // clic gauche : c'est le comportement attendu sur Windows, où le menu
        // de zone s'ouvre au clic droit.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            ID_OUVRIR => montrer(app),
            ID_QUITTER => {
                marquer_quitter();
                app.exit(0);
            }
            autre => tracing::warn!("entree de menu inconnue : {autre}"),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = event
            {
                // Le clic gauche **relâché** montre la fenêtre ; l'appuyé ne le
                // fait pas, sinon un simple clic ouvrirait deux fois.
                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    montrer(tray.app_handle());
                }
            }
        })
        .build(app)?;

    app.manage(MenuZone { ouvrir, quitter });
    TRAY_VISIBLE.store(true, Ordering::SeqCst);
    tracing::info!("icone de zone creee");
    Ok(())
}

/// Intercepte la fermeture : cache la fenêtre au lieu de quitter.
///
/// `WindowEvent::CloseRequested` est un **événement de fenêtre**, pas de fenêtre
/// webview — c'est pourquoi cette partie ne peut pas vivre dans l'interface,
/// contrairement à l'icône elle-même.
pub fn sur_evenement_fenetre(fenetre: &tauri::Window, evenement: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = evenement else {
        return;
    };
    // **Seule la fenêtre principale se cache.** Mesuré le 2026-10-06 : ce
    // gestionnaire s'appliquait à *toute* fenêtre, donc la fenêtre de
    // vérification Cloudflare était **cachée** au lieu d'être fermée — et
    // `cancel_challenge` renvoyait `Ok(())` en croyant l'avoir fermée. Le
    // symptôme : deux `cancel_challenge` successifs laissaient la fenêtre
    // ouverte, le cookie encore lisible, alors que la commande annonçait avoir
    // fermé. Une fenêtre de challenge n'a pas à revenir dans la zone : elle
    // n'a pas de tray, donc « la mettre dans la zone » veut dire « la cacher
    // définitivement ».
    if !est_fenetre_principale(fenetre) {
        return;
    }
    if !doit_cacher(en_cours_de_quitter(), tray_visible()) {
        // Sortie volontaire, ou pas de tray pour ramener la fenêtre : on laisse
        // la fermeture se faire.
        return;
    }
    api.prevent_close();
    if let Err(e) = fenetre.hide() {
        tracing::warn!("cache de la fenetre impossible : {e}");
    }
}

/// La fenêtre principale est la seule qui se met dans la zone de notification.
///
/// Une fenêtre de vérification a un label `challenge-<source>` : elle n'est ni
/// la fenêtre principale, ni une fenêtre qu'on doit pouvoir ramener — donc la
/// Decision est prise sur le **label**, seule donnée disponible ici, et la
/// décision reste testable sans Tauri.
fn est_fenetre_principale(fenetre: &tauri::Window) -> bool {
    est_fenetre_principale_par_label(fenetre.label())
}

/// Version testable : le gestionnaire d'évènement reçoit une fenêtre, donc on
/// isole la décision sur son label.
pub fn est_fenetre_principale_par_label(label: &str) -> bool {
    label == "main"
}

/// Remplace les libellés du menu. Appelé par l'interface au démarrage et à
/// chaque changement de langue.
///
/// **Partielle** : une entrée omise garde son texte. Un libellé **vide** est
/// ignoré — une entrée de menu sans texte est illisible, donc pire que de
/// garder le texte précédent.
///
/// Renvoie les textes **relus depuis le menu**, pas ceux reçus : c'est la seule
/// preuve que le système a bien pris le texte. Le renvoyer donne aussi un moyen
/// de vérifier sans ajouter `core:menu:allow-get-text` aux capabilities, ce que
/// la règle « strict minimum » interdit d'ajouter pour les besoins d'une mesure.
#[tauri::command]
pub fn set_libelles(
    app: AppHandle,
    ouvrir: Option<String>,
    quitter: Option<String>,
) -> Result<(String, String), String> {
    // Sans icône de zone, il n'y a rien à renommer. Ce n'est pas une erreur :
    // l'appel peut arriver avant que `setup()` n'ait fini.
    let Some(menu) = app.try_state::<MenuZone>() else {
        return Err("icone de zone absente : aucun menu a renommer".to_string());
    };
    if let Some(texte) = ouvrir.as_deref().filter(|t| libelle_exploitable(t)) {
        menu.ouvrir.set_text(texte).map_err(|e| e.to_string())?;
    }
    if let Some(texte) = quitter.as_deref().filter(|t| libelle_exploitable(t)) {
        menu.quitter.set_text(texte).map_err(|e| e.to_string())?;
    }
    let lu = (
        menu.ouvrir.text().map_err(|e| e.to_string())?,
        menu.quitter.text().map_err(|e| e.to_string())?,
    );
    tracing::info!("libelles du menu de zone : {:?} / {:?}", lu.0, lu.1);
    Ok(lu)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le drapeau doit avoir **un seul** état dans ce test, et cet état doit
    /// être vrai à la fin : c'est la condition pour que « Quitter » sorte
    /// vraiment de l'interception de fermeture.
    ///
    /// Un seul test et non deux : le drapeau est un `static` global, donc deux
    /// tests qui le manipulent en parallèle se marcheraient dessus.
    #[test]
    fn le_drapeau_de_quitte_arme_la_sortie_reelle() {
        QUITTER.store(false, Ordering::SeqCst);
        assert!(!en_cours_de_quitter(), "au depart, on ne quite pas");
        marquer_quitter();
        assert!(
            en_cours_de_quitter(),
            "apres le menu, on doit vraiment quitter"
        );
    }

    /// Un libellé vide ne doit pas **écraser** un libellé correct : une entrée
    /// de menu sans texte est illisible, donc pire que l'ancien texte.
    #[test]
    fn un_libelle_vide_est_ignore() {
        assert!(!libelle_exploitable(""));
        assert!(!libelle_exploitable("   "));
        assert!(!libelle_exploitable("\t\n"));
        assert!(libelle_exploitable("Ouvrir Tatami"));
    }

    /// Un libellé blanc autour d'un texte est **exploitable** : l'interface peut
    /// envoyer des espaces par accident, et les rejeter laisserait un menu
    /// dans une langue au hasard plutôt que dans la bonne.
    #[test]
    fn un_libelle_entoure_de_espaces_est_accepte() {
        assert!(
            libelle_exploitable("  Quitter  "),
            "espaces autour ne doivent pas faire perdre le libelle"
        );
    }

    /// Le scénario grave de cette tâche : **masquer la fenêtre alors que rien ne
    /// peut la ramener.** L'application devient introuvable, et l'utilisateur
    /// doit passer par le gestionnaire de tâches.
    ///
    /// Chaque cas est un test distinct et nomme, pour qu un echec dise *lequel*
    /// des deux garde-fous a cédé — un seul test qui boucle dirait seulement
    /// « ça échoue ».
    #[test]
    fn on_ne_masque_la_fenetre_que_si_le_tray_est_cliquable() {
        // Le cas nominal : tout est là, on masque.
        assert!(
            doit_cacher(false, true),
            "fermer doit mettre dans la zone quand le tray est la"
        );
        // Sortie volontaire : « Quitter » ne doit surtout pas être annulé.
        assert!(
            !doit_cacher(true, true),
            "une sortie volontaire ne peut pas etre transformee en mise dans la zone"
        );
        // Aucun tray : pas d'icone, donc masquer serait une impasse.
        assert!(
            !doit_cacher(false, false),
            "sans icone, masquer la fenetre rendrait l'application injoignable"
        );
        // Les deux : sortie volontaire ET pas de tray.
        assert!(!doit_cacher(true, false));
    }

    /// Seule la fenêtre **principale** se met dans la zone.
    ///
    /// Mesuré le 2026-10-06 : le gestionnaire s'appliquait à toutes les
    /// fenêtres, donc la fenêtre de vérification Cloudflare était **cachée**
    /// au lieu d'être fermée, et `cancel_challenge` renvoyait `Ok(())` en
    /// croyant l'avoir fermée. Deux annulations successives laissaient la
    /// fenêtre ouverte, cookie encore lisible.
    ///
    /// Le cas nominal est écrit **en premier** : un test qui n'aurait qu'un cas
    /// d'échec passerait aussi avec la fonction qui renvoie `false` partout.
    #[test]
    fn seule_la_fenetre_principale_se_met_dans_la_zone() {
        assert!(
            est_fenetre_principale_par_label("main"),
            "la fenetre principale doit se mettre dans la zone"
        );
        for label in ["challenge-KAGANE", "challenge-MANGADEX"] {
            assert!(
                !est_fenetre_principale_par_label(label),
                "une fenetre de verification doit se FERMER : {label} n'a pas de tray, \
                 donc la cacher la rendrait inaccessible definitivement"
            );
        }
    }
}
