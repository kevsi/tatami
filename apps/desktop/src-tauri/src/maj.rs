//! Mise à jour de l'application (ADR 0020).
//!
//! **Pourquoi ce module vit dans le cœur.** L'interface ne touche ni au réseau
//! ni à l'installation : elle appelle `etat_maj`, `verifier_maj` et
//! `installer_maj`, et écoute `mise-a-jour://progression`. C'est le même contrat
//! que le reste du projet (commandes et événements Tauri), et cela évite
//! d'accorder à la webview les permissions du plugin.
//!
//! **Aucun objet `Update` n'est conservé entre deux commandes.** Il porte le
//! contexte complet du plugin (client HTTP, hooks) et le garder en état géré
//! pour économiser un GET de quelques kilo-octets ajouterait une pièce mobile
//! et un manifeste périmé. `installer_maj` refait donc le contrôle, puis
//! installe la version qu'il vient de lire.
//!
//! **Pas d'`app.restart()`.** Sous Windows, `download_and_install` ferme
//! l'application lui-même après avoir lancé l'installeur, et
//! `restart_after_install` vaut `true` par défaut : le plugin relance l'app via
//! NSIS. `tauri-plugin-process` reste donc inutile.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

use crate::store::now_ms;

/// Progression du téléchargement de l'installeur.
///
/// Nom distinct de `maj://nouveaux-chapitres` (planificateur) : `maj` y désigne
/// les **chapitres** nouveaux, pas une version de l'application.
pub const EVT_PROGRESSION: &str = "mise-a-jour://progression";

/// Version annoncée par le manifeste, telle que l'interface l'affiche.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VersionDisponible {
    pub version: String,
    pub version_courante: String,
    /// Notes de version du manifeste (texte brut ou Markdown, affiché tel quel).
    pub notes: Option<String>,
    /// Date de publication en millisecondes depuis l'époque, comme le reste du
    /// cœur : le formatage appartient à l'interface.
    pub date_ms: Option<i64>,
}

/// Instantané de l'état, rendu à chaque commande.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct EtatMaj {
    pub version_courante: String,
    pub disponible: Option<VersionDisponible>,
    pub derniere_verification_ms: Option<i64>,
    pub installation_en_cours: bool,
}

/// Codes d'erreur stables, traduits côté interface. Le message technique part
/// au journal : l'écran n'a pas à montrer une URL ni un chemin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CodeErreurMaj {
    Reseau,
    Signature,
    Espace,
    Droits,
    Inconnu,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ErreurMaj {
    pub code: CodeErreurMaj,
}

/// Charge utile de `EVT_PROGRESSION`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progression {
    pub recu: u64,
    /// `None` quand le serveur n'annonce pas de `Content-Length`.
    pub total: Option<u64>,
}

#[derive(Debug, Default)]
pub struct EtatPartage {
    interne: Mutex<Interne>,
    /// Drapeau d'installation : deux clics ne doivent pas lancer deux
    /// installations. Atomique, jamais un verrou tenu au travers d'un `await`.
    installation: AtomicBool,
}

#[derive(Debug, Default)]
struct Interne {
    disponible: Option<VersionDisponible>,
    derniere_verification_ms: Option<i64>,
}

impl EtatPartage {
    fn instantane(&self, version_courante: String) -> EtatMaj {
        let interne = self.interne.lock().unwrap_or_else(|e| e.into_inner());
        EtatMaj {
            version_courante,
            disponible: interne.disponible.clone(),
            derniere_verification_ms: interne.derniere_verification_ms,
            installation_en_cours: self.installation.load(Ordering::SeqCst),
        }
    }
}

fn version_courante(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Traduit une erreur du plugin en code stable.
///
/// `Error` est `#[non_exhaustive]` : la branche finale est obligatoire, et elle
/// est volontairement `Inconnu` plutôt que devinée.
pub fn classer(erreur: &tauri_plugin_updater::Error) -> CodeErreurMaj {
    use tauri_plugin_updater::Error as E;
    match erreur {
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => CodeErreurMaj::Signature,
        E::ReleaseNotFound
        | E::Network(_)
        | E::Reqwest(_)
        | E::TargetNotFound(_)
        | E::TargetsNotFound(_)
        | E::Serialization(_)
        | E::Semver(_)
        | E::UrlParse(_) => CodeErreurMaj::Reseau,
        E::Io(io) => {
            // 112 = ERROR_DISK_FULL (Windows) : la seule façon de distinguer un
            // disque plein d'une autre erreur d'écriture, `ErrorKind` n'ayant
            // pas de variante stable pour cela.
            if io.raw_os_error() == Some(112) {
                CodeErreurMaj::Espace
            } else if io.kind() == std::io::ErrorKind::PermissionDenied {
                CodeErreurMaj::Droits
            } else {
                CodeErreurMaj::Inconnu
            }
        }
        _ => CodeErreurMaj::Inconnu,
    }
}

fn erreur_et_journal(etape: &str, erreur: &tauri_plugin_updater::Error) -> ErreurMaj {
    tracing::warn!("mise a jour ({etape}) : {erreur}");
    ErreurMaj {
        code: classer(erreur),
    }
}

/// Construit la version affichable depuis une réponse du plugin.
///
/// `pub` pour être mesurable : c'est la seule conversion manifeste → écran, et
/// une inversion de champs (annoncée / courante) ne se verrait qu'à l'écran.
pub fn version_disponible(mise_a_jour: &tauri_plugin_updater::Update) -> VersionDisponible {
    VersionDisponible {
        version: mise_a_jour.version.clone(),
        version_courante: mise_a_jour.current_version.clone(),
        notes: mise_a_jour.body.clone(),
        date_ms: mise_a_jour
            .date
            .map(|date| date.unix_timestamp().saturating_mul(1000)),
    }
}

#[tauri::command]
pub fn etat_maj(app: AppHandle, etat: State<'_, EtatPartage>) -> EtatMaj {
    etat.instantane(version_courante(&app))
}

#[tauri::command]
pub async fn verifier_maj(
    app: AppHandle,
    etat: State<'_, EtatPartage>,
) -> Result<EtatMaj, ErreurMaj> {
    let updater = app
        .updater()
        .map_err(|e| erreur_et_journal("verifier", &e))?;
    let trouve = updater
        .check()
        .await
        .map_err(|e| erreur_et_journal("verifier", &e))?;

    tracing::info!(
        "mise a jour : verification -> {}",
        trouve
            .as_ref()
            .map_or("a jour".to_string(), |u| u.version.clone())
    );

    {
        let mut interne = etat.interne.lock().unwrap_or_else(|e| e.into_inner());
        interne.disponible = trouve.as_ref().map(version_disponible);
        interne.derniere_verification_ms = Some(now_ms());
    }
    Ok(etat.instantane(version_courante(&app)))
}

#[tauri::command]
pub async fn installer_maj(
    app: AppHandle,
    etat: State<'_, EtatPartage>,
) -> Result<EtatMaj, ErreurMaj> {
    if etat
        .installation
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        tracing::warn!("mise a jour : installation deja en cours, second appel ignore");
        return Err(ErreurMaj {
            code: CodeErreurMaj::Inconnu,
        });
    }

    let resultat = installer(&app, &etat).await;
    if resultat.is_err() {
        // Sur Windows, un succès ne revient jamais : le plugin ferme le
        // processus. Le drapeau n'est donc relâché que sur erreur.
        etat.installation.store(false, Ordering::SeqCst);
    }
    resultat.map(|()| etat.instantane(version_courante(&app)))
}

async fn installer(app: &AppHandle, etat: &EtatPartage) -> Result<(), ErreurMaj> {
    let updater = app
        .updater_builder()
        .on_before_exit(|| {
            // Il n'y a pas de file de téléchargements côté cœur : l'interface
            // pilote les chapitres un par un, et c'est elle qui refuse de lancer
            // une installation pendant un téléchargement (ADR 0020).
            tracing::warn!("mise a jour : fermeture de l'application pour installer");
        })
        .build()
        .map_err(|e| erreur_et_journal("installer", &e))?;

    let Some(mise_a_jour) = updater
        .check()
        .await
        .map_err(|e| erreur_et_journal("installer", &e))?
    else {
        // Le manifeste ne propose plus rien (publié puis retiré, ou version
        // égale) : l'écran ne doit pas continuer d'annoncer une mise à jour
        // installable. Sans cette remise à zéro, `etat_maj` resservirait le
        // disponible mémorisé lors du contrôle précédent.
        tracing::info!("mise a jour : plus rien a installer");
        let mut interne = etat.interne.lock().unwrap_or_else(|e| e.into_inner());
        interne.disponible = None;
        interne.derniere_verification_ms = Some(now_ms());
        return Ok(());
    };

    tracing::info!("mise a jour : installation de {}", mise_a_jour.version);
    let pour_progression = app.clone();
    mise_a_jour
        .download_and_install(
            move |recu, total| {
                let _ = pour_progression.emit(
                    EVT_PROGRESSION,
                    Progression {
                        recu: recu as u64,
                        total,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|e| erreur_et_journal("installer", &e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_updater::Error as E;

    #[test]
    fn un_disque_plein_est_classe_espace() {
        assert_eq!(
            classer(&E::Io(std::io::Error::from_raw_os_error(112))),
            CodeErreurMaj::Espace
        );
    }

    #[test]
    fn un_refus_de_droits_est_classe_droits() {
        assert_eq!(
            classer(&E::Io(std::io::Error::from(
                std::io::ErrorKind::PermissionDenied
            ))),
            CodeErreurMaj::Droits
        );
    }

    #[test]
    fn une_erreur_d_utf8_de_signature_est_classee_signature() {
        assert_eq!(
            classer(&E::SignatureUtf8("signature illisible".to_string())),
            CodeErreurMaj::Signature
        );
    }

    #[test]
    fn une_configuration_sans_point_de_terminaison_n_est_pas_une_panne_reseau() {
        // `EmptyEndpoints` veut dire « ce binaire n'a pas d'URL de manifeste »
        // (build ordinaire : `plugins.updater.endpoints` est vide) : accuser le
        // réseau enverrait l'utilisateur vérifier sa connexion pour rien.
        assert_eq!(classer(&E::EmptyEndpoints), CodeErreurMaj::Inconnu);
    }

    #[test]
    fn une_erreur_non_couverte_ne_devine_pas_un_code() {
        assert_eq!(classer(&E::UnsupportedArch), CodeErreurMaj::Inconnu);
    }

    #[test]
    fn un_etat_vide_ne_pretend_pas_connaitre_de_version() {
        let etat = EtatPartage::default();
        let instantane = etat.instantane("0.1.0".to_string());
        assert_eq!(instantane.version_courante, "0.1.0");
        assert!(instantane.disponible.is_none());
        assert!(instantane.derniere_verification_ms.is_none());
        assert!(!instantane.installation_en_cours);
    }
}
