//! Pilote de la chaîne de vision, **sans interface**.
//!
//! Il existe parce que la chaîne se prouve plus vite et plus sûrement sans
//! interface : on lui donne un fichier image, il rend les blocs qu'elle
//! trouve. Sans lui, la seule façon de savoir si `analyser_page` marche est
//! d'ouvrir l'application, de trouver un chapitre anglais, d'attendre — et un
//! échec ne dit pas *où*.
//!
//! ```
//! cargo run --example analyse_page -- <image> <pack> <sidecar> [runtime-dll]
//! ```
//!
//! Sortie : les blocs, un par bloc, avec le texte lu et sa boîte. C'est la
//! même sortie que la surcouche consommera, donc ce qui est lisible ici est ce
//! qui sera affiché.

use std::path::PathBuf;
use std::sync::Arc;

use tatami_desktop::vision_pipeline::analyser_octets;
use tatami_desktop::vision_superviseur::SuperviseurVision;

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: analyse_page <image> <pack> <sidecar> [runtime-dll]");
        eprintln!("  image       fichier image de la page (jpeg/png)");
        eprintln!("  pack        dossier du pack vision");
        eprintln!("  sidecar     tatami-vision.exe");
        eprintln!("  runtime-dll onnxruntime.dll (facultatif, sinon ORT_DYLIB_PATH)");
        std::process::exit(2);
    }
    let image = PathBuf::from(&args[1]);
    let pack = PathBuf::from(&args[2]);
    let sidecar = PathBuf::from(&args[3]);

    let runtime = args.get(4).map(PathBuf::from).or_else(|| {
        std::env::var("ORT_DYLIB_PATH")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
    });

    if !image.is_file() {
        return Err(format!("image absente : {}", image.display()));
    }
    if !sidecar.is_file() {
        return Err(format!("sidecar absent : {}", sidecar.display()));
    }
    println!("image   : {}", image.display());
    println!("pack    : {}", pack.display());
    println!("sidecar : {}", sidecar.display());
    println!(
        "runtime : {}",
        runtime.as_ref().map_or_else(
            || "<recherche par ort>".to_string(),
            |p| p.display().to_string()
        )
    );

    let octets = std::fs::read(&image).map_err(|e| format!("lecture image : {e}"))?;
    println!("poids   : {} octets", octets.len());

    let superviseur = Arc::new(SuperviseurVision::new(sidecar, runtime, Some(pack)));

    // Le chrono compte : la chaîne est **lente**, et savoir combien est aussi
    // utile que le résultat.
    let debut = std::time::Instant::now();
    let page = analyser_octets(
        &superviseur,
        &octets,
        "jeton-de-pilotage",
        "https://exemple.test/chapitre-1",
        "PILOTAGE",
    )
    .await
    .map_err(|e| e.to_string())?;
    let duree = debut.elapsed();

    println!();
    println!(
        "page {}x{} — {} bloc(s) en {:.1?} ({} ms d'analyse)",
        page.width,
        page.height,
        page.blocks.len(),
        duree,
        duree.as_millis()
    );
    println!();
    for bloc in &page.blocks {
        println!(
            "  [{}] {:?} ({},{}) {}x{} score {:.2} -> {:?}",
            bloc.id,
            bloc.type_bloc,
            bloc.bbox.x,
            bloc.bbox.y,
            bloc.bbox.w,
            bloc.bbox.h,
            bloc.confidence.unwrap_or(0.0),
            bloc.original_text.as_deref().unwrap_or("")
        );
    }
    println!();
    println!("champs requis par le schéma, remplis :");
    println!(
        "  status par bloc   : {:?}",
        page.blocks
            .iter()
            .map(|b| format!("{:?}", b.status))
            .collect::<Vec<_>>()
    );
    println!(
        "  page_id/chapter/source : {:?} / {:?} / {:?}",
        page.page_id, page.chapter_url, page.source
    );
    Ok(())
}
