//! Commandes Tauri de sauvegarde / restauration de la bibliothèque.
//!
//! Elles sont **fines** : elles ouvrent le dialogue natif, lisent ou écrivent
//! le fichier JSON, et délèguent toute la logique à `store::Store`
//! (`sauvegarde()` / `restaurer()`, testées sans Tauri).
//!
//! Le dialogue est ouvert **ici, pas par l'interface**, pour la même raison
//! que `importer_source` (cf. `custom_source.rs`) : aucun chemin ne traverse
//! l'IPC. La seule action possible est celle que l'utilisateur voit et
//! confirme dans le sélecteur natif.

use serde::Serialize;
use std::io::Read as _;
use std::sync::Arc;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::store::{ComptesRestauration, Sauvegarde, Store};

/// Taille maximale d'un fichier de sauvegarde accepté à l'import (octets).
/// Une bibliothèque de plusieurs milliers d'entrées tient en ~1 Mo : ce plafond
/// refuse un fichier aberrant **avant** de lire quoi que ce soit en mémoire.
const TAILLE_MAX: u64 = 25 * 1024 * 1024;

/// Ce qu'une sauvegarde ou une restauration a fait, tel que l'écran l'affiche.
/// Les comptes sont des **deltas** côté restauration (cf. `ComptesRestauration`)
/// et des décomptes du fichier côté export : la même structure sert aux deux
/// commandes, et le panneau n'a qu'un seul type à déclarer.
#[derive(Serialize)]
pub struct RapportSauvegarde {
    pub chemin: String,
    pub categories: u32,
    pub favoris: u32,
    pub historique: u32,
    pub signets: u32,
}

fn rapport(chemin: String, s: &Sauvegarde) -> RapportSauvegarde {
    RapportSauvegarde {
        chemin,
        categories: s.categories.len() as u32,
        favoris: s.favoris.len() as u32,
        historique: s.historique.len() as u32,
        signets: s.signets.len() as u32,
    }
}

fn rapport_comptes(chemin: String, c: ComptesRestauration) -> RapportSauvegarde {
    RapportSauvegarde {
        chemin,
        categories: c.categories,
        favoris: c.favoris,
        historique: c.historique,
        signets: c.signets,
    }
}

/// Date locale **UTC** au format `AAAA-MM-JJ`, dérivée de `now_ms()` — la même
/// horloge que le reste de la base (`store::now_ms`). L'algorithme est
/// `civil_from_days` (Howard Hinnant) : aucune dépendance de plus pour un nom
/// de fichier par défaut que l'utilisateur peut changer.
fn date_iso(ms: i64) -> String {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let annee = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let jour = doy - (153 * mp + 2) / 5 + 1;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    let annee = if mois <= 2 { annee + 1 } else { annee };
    format!("{annee:04}-{mois:02}-{jour:02}")
}

/// Écrit la bibliothèque dans un fichier JSON choisi par l'utilisateur.
///
/// Renvoie `None` si le dialogue est annulé — ce n'est ni une erreur ni un
/// succès à afficher, l'utilisateur a simplement changé d'avis.
#[tauri::command]
pub async fn exporter_sauvegarde(
    app: tauri::AppHandle,
    store: State<'_, Arc<Store>>,
) -> Result<Option<RapportSauvegarde>, String> {
    // La bibliothèque est lue AVANT le dialogue : le fichier proposé porte la
    // date du jour, et un échec de lecture n'ouvre pas un sélecteur pour rien.
    let sauvegarde = store.sauvegarde()?;
    let defaut = format!(
        "tatami-sauvegarde-{}.json",
        date_iso(sauvegarde.exported_at)
    );

    // `blocking_save_file` poste la boîte sur le fil principal et attend sur un
    // canal : jamais appelée depuis ce fil (même raison que `importer_source`).
    let choisi = app
        .dialog()
        .file()
        .set_title("Exporter la bibliothèque")
        .add_filter("Sauvegarde Tatami (JSON)", &["json"])
        .set_file_name(&defaut)
        .blocking_save_file();
    let Some(fichier) = choisi else {
        return Ok(None);
    };
    let chemin = fichier
        .into_path()
        .map_err(|e| format!("chemin choisi illisible : {e}"))?;

    let json =
        serde_json::to_string_pretty(&sauvegarde).map_err(|e| format!("sérialisation : {e}"))?;
    std::fs::write(&chemin, json).map_err(|e| format!("écriture du fichier : {e}"))?;

    let chemin_str = chemin.to_string_lossy().into_owned();
    Ok(Some(rapport(chemin_str, &sauvegarde)))
}

/// Fusionne un fichier de sauvegarde dans la bibliothèque courante.
///
/// La fusion est dans `Store::restaurer` et ne supprime **jamais** rien. Le
/// refus d'une version inconnue vient de là aussi ; le message utilisateur est
/// le même, avec le chemin en tête pour qu'on sache **quel** fichier est en
/// cause.
#[tauri::command]
pub async fn importer_sauvegarde(
    app: tauri::AppHandle,
    store: State<'_, Arc<Store>>,
) -> Result<Option<RapportSauvegarde>, String> {
    let choisi = app
        .dialog()
        .file()
        .set_title("Importer une sauvegarde")
        .add_filter("Sauvegarde Tatami (JSON)", &["json"])
        .blocking_pick_file();
    let Some(fichier) = choisi else {
        return Ok(None);
    };
    let chemin = fichier
        .into_path()
        .map_err(|e| format!("chemin choisi illisible : {e}"))?;

    // La taille se vérifie avant la lecture : un fichier de 12 Go refusé là
    // reste un refus immédiat, pas une allocation qui meurt au milieu.
    let meta = std::fs::metadata(&chemin).map_err(|e| format!("lecture du fichier : {e}"))?;
    if meta.len() > TAILLE_MAX {
        return Err(format!(
            "fichier trop volumineux : {} octets (maximum {TAILLE_MAX})",
            meta.len()
        ));
    }

    let mut contenu = String::new();
    std::fs::File::open(&chemin)
        .and_then(|mut f| f.read_to_string(&mut contenu))
        .map_err(|e| format!("lecture du fichier : {e}"))?;
    let sauvegarde: Sauvegarde =
        serde_json::from_str(&contenu).map_err(|e| format!("sauvegarde illisible : {e}"))?;

    let comptes = store.restaurer(&sauvegarde)?;
    let chemin_str = chemin.to_string_lossy().into_owned();
    Ok(Some(rapport_comptes(chemin_str, comptes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::VERSION_SAUVEGARDE;

    #[test]
    fn la_date_iso_reproduit_des_instants_connus() {
        // Époque : 1970-01-01. Le lendemain. Un instant en heure d'hiver et un
        // autre en heure d'été (le passage d'heure ne doit rien décaler).
        assert_eq!(date_iso(0), "1970-01-01");
        assert_eq!(date_iso(86_400_000), "1970-01-02");
        assert_eq!(date_iso(1_700_000_000_000), "2023-11-14");
        assert_eq!(date_iso(1_583_020_800_000), "2020-03-01");
        assert_eq!(date_iso(1_759_622_400_000), "2025-10-05");
    }

    #[test]
    fn le_rapport_dexport_compte_le_fichier() {
        let sv = Sauvegarde {
            version: VERSION_SAUVEGARDE,
            exported_at: 0,
            categories: vec![crate::store::SauvegardeCategorie {
                name: "C".into(),
                color: 1,
            }],
            favoris: vec![],
            historique: vec![],
            signets: vec![],
        };
        let r = rapport("C:\\x.json".into(), &sv);
        assert_eq!(r.chemin, "C:\\x.json");
        assert_eq!(r.categories, 1);
        assert_eq!(r.favoris, 0);
        assert_eq!(r.historique, 0);
        assert_eq!(r.signets, 0);
    }
}
