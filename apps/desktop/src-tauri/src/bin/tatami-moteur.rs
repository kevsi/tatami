//! Tâche 5.1 — gestion du moteur **en ligne de commande**, pour la mise à jour
//! manuelle.
//!
//! ## Pourquoi une commande plutôt qu'un écran
//!
//! La mise à jour en mode manuel, c'est : fabriquer une archive, la signer,
//! l'installer. Les deux premières étapes sont **déjà** des commandes
//! (`scripts/moteur-sign.mjs`). Une troisième, pour la même opération, reste
//! cohérente — et elle n'ajoute aucune permission Tauri (`dialog`, `fs`), ce que
//! « permissions au strict minimum » (AGENTS.md §7) apprécie.
//!
//! ## Ce qui n'est PAS dupliqué
//!
//! Ce binaire ne réimplémente rien : il appelle
//! [`tatami_desktop::moteur`], exactement comme l'application. Une seconde
//! implémentation de la vérification de signature serait le pire des deux mondes
//! — deux endroits où une faute passe, et un « ça marche en ligne de commande »
//! qui ne prouve rien sur le chemin réel.
//!
//! ## Commandes
//!
//! ```text
//! tatami-moteur etat                 # le pointeur, et la version en service
//! tatami-moteur verifier <m.json>    # signature et empreinte, sans rien écrire
//! tatami-moteur installer <m.json>    # vérifie, extrait, installe, puis affiche
//! tatami-moteur activer <version>    # bascule le pointeur
//! tatami-moteur revenir              # retour à la version précédente
//! ```
//!
//! `installer` **n'active pas**. Ce sont deux gestes séparés volontairement :
//! on veut pouvoir installer, regarder ce qu'on a obtenu, et seulement ensuite
//! basculer.

use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "tatami-moteur — mise à jour manuelle du moteur (tâche 5.1)\n\
         \n\
         \x20 etat                      le pointeur et la version en service\n\
         \x20 verifier <m.json>         vérifie la signature et l'empreinte, n'écrit rien\n\
         \x20 installer <m.json>       vérifie et extrait dans versions/<version>/\n\
         \x20 activer <version>        bascule le pointeur\n\
         \x20 revenir                   revient à la version précédente\n\
         \n\
         L'archive est cherchée **à côté** du manifeste, sous le nom que le\n\
         manifeste déclare. Aucun téléchargement n'existe : c'est un choix\n\
         (ADR 0006, § 6)."
    );
    std::process::exit(2)
}

/// Le manifeste, et l'archive qu'il déclare, impliedues l'une de l'autre.
fn lire_et_verifier(chemin: &Path) -> tatami_desktop::moteur::Manifeste {
    let json = match std::fs::read_to_string(chemin) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("manifeste illisible : {} ({e})", chemin.display());
            std::process::exit(1);
        }
    };
    // `charger_signe` vérifie la signature contre la clé **compilée dans ce
    // binaire**. C'est le même code, et la même clé, que l'application.
    match tatami_desktop::moteur::charger_signe(&json) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("REFUSÉ : {e}");
            eprintln!("L'archive ne sera pas extraite.");
            std::process::exit(1);
        }
    }
}

fn chemin_archive(manifeste: &Path, m: &tatami_desktop::moteur::Manifeste) -> PathBuf {
    manifeste
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&m.archive)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(commande) = args.first().map(String::as_str) else {
        usage()
    };

    match commande {
        "etat" => match tatami_desktop::moteur::lire_actif() {
            Some(a) => {
                println!("version en service : {}", a.version);
                match &a.precedente {
                        Some(p) => println!("version précédente : {p}   (revenir : tatami-moteur revenir)"),
                        None => println!("version précédente : aucune — un retour arrière est impossible tant qu'aucune mise à jour n'a eu lieu"),
                    }
                match tatami_desktop::moteur::installe() {
                    Some((java, cp)) => {
                        println!("moteur              : {}", java.display());
                        println!("classpath           : {cp}");
                    }
                    None => println!(
                        "moteur              : INTROUVABLE. Le pointeur désigne une version \
                             absente ou sans runtime — l'application retombait sur le moteur \
                             embarqué."
                    ),
                }
            }
            None => println!(
                "aucun moteur installé : l'application utilise le moteur embarqué. \
                     C'est l'état normal d'une application qui n'a jamais été mise à jour."
            ),
        },

        "verifier" => {
            let Some(chemin) = args.get(1) else { usage() };
            let chemin = PathBuf::from(chemin);
            let m = lire_et_verifier(&chemin);
            let archive = chemin_archive(&chemin, &m);
            println!("signature : valide (clé embarquée dans ce binaire)");
            println!("version   : {}", m.version);
            println!("moteur    : {}", m.engine);
            println!("parsers   : {}", m.parsers);
            println!("archive   : {}", m.archive);
            match tatami_desktop::moteur::empreinte_fichier(&archive) {
                Ok(obtenue) if obtenue == m.sha256.to_ascii_lowercase() => {
                    println!("empreinte : conforme — l'archive est exactement celle qui est signée")
                }
                Ok(obtenue) => {
                    eprintln!("empreinte : INATTENDUE");
                    eprintln!("  annoncée : {}", m.sha256);
                    eprintln!("  obtenue  : {obtenue}");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("archive illisible : {} ({e})", archive.display());
                    std::process::exit(1);
                }
            }
        }

        "installer" => {
            let Some(chemin) = args.get(1) else { usage() };
            let chemin = PathBuf::from(chemin);
            let m = lire_et_verifier(&chemin);
            let archive = chemin_archive(&chemin, &m);
            println!("signature : valide");
            println!("archive   : {}", archive.display());
            match tatami_desktop::moteur::installer(&archive, &m) {
                Ok(dossier) => {
                    println!("installé  : {}", dossier.display());
                    println!();
                    println!("Rien n'est encore activé. Pour l'utiliser :");
                    println!("  tatami-moteur activer {}", m.version);
                    println!(
                        "\n(ou supprimez `version` du manifest et recommencez : rien n'a été écrit)"
                    );
                }
                Err(e) => {
                    eprintln!("INSTALLATION REFUSÉE : {e}");
                    std::process::exit(1);
                }
            }
        }

        "activer" => {
            let Some(version) = args.get(1) else { usage() };
            match tatami_desktop::moteur::activer(version) {
                Ok(a) => {
                    println!("pointeur basculé sur {}", a.version);
                    match a.precedente {
                        Some(p) => println!("version précédente conservée : {p}"),
                        None => println!(
                            "aucune version précédente : un retour arrière ne sera pas possible"
                        ),
                    }
                }
                Err(e) => {
                    eprintln!("ACTIVATION REFUSÉE : {e}");
                    std::process::exit(1);
                }
            }
        }

        "revenir" => match tatami_desktop::moteur::revenir_en_arriere() {
            Ok(true) => println!("retour à la version précédente effectué"),
            Ok(false) => println!("aucun moteur installé : rien à reprendre"),
            Err(e) => {
                eprintln!("RETOUR IMPOSSIBLE : {e}");
                std::process::exit(1);
            }
        },

        _ => usage(),
    }
}
