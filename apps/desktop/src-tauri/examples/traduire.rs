//! Prove it out: send one real bubble through the app's translator factory.
//!
//! This exercises the exact path the reader uses: `Traducteur::traduire` →
//! provider chain (OpenRouter → Gemini → local). Not a re-implementation.
//!
//! ```
//! set OPENROUTER_API_KEY=...
//! cargo run --example traduire -- "PROBABLY\nTHE SAME PLACE\n..."
//! ```

#[tokio::main]
async fn main() -> Result<(), String> {
    // Le texte réel lu par le pilote sur `2-webtoonsen/p072.jpg` : le même que
    // l'OCR envoie au traducteur dans le lecteur.
    let texte = std::env::args().nth(1).unwrap_or_else(|| {
        "PROBABLY\nTHE SAME PLACE\nYOU GO TO GET\nCLOTHES FOR\nDOGS".to_string()
    });

    println!("--- entrée ---");
    println!("{texte}");
    println!("--- traduction ---");

    let t = tatami_desktop::traduction::Traducteur::default();
    match t.traduire(&texte).await {
        Ok(fr) => {
            println!("{fr}");
            Ok(())
        }
        Err(e) => {
            eprintln!("échec : {e}");
            Err(e.to_string())
        }
    }
}
