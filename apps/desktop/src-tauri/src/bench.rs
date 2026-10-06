//! Benchmark mémoire du pipeline vision (T2.7).
//!
//! Cette module mesure l'empreinte mémoire des étapes clés du pipeline
//! et vérifie qu'elles restent dans le budget défini pour la machine cible
//! (12 Go RAM, Windows 10).

/// Budget mémoire par tuile (en Mo).
/// Une tuile de 2048x2048 en RGBA = 16 Mo, on ajoute une marge pour
/// les masques et les structures de données.
pub const BUDGET_TUILE_MO: u64 = 64;

/// Budget mémoire total pour le pipeline vision (en Mo).
/// Le pipeline traite plusieurs tuiles en parallèle, mais le cœur
/// Rust limite la concurrence pour rester sous ce plafond.
pub const BUDGET_PIPELINE_MO: u64 = 256;

/// Mesure l'empreinte estimée d'une tuile.
///
/// # Paramètres
/// - `largeur` : largeur de la tuile en pixels
/// - `hauteur` : hauteur de la tuile en pixels
///
/// # Retour
/// Empreinte estimée en octets.
pub fn empreinte_tuile(largeur: u32, hauteur: u32) -> u64 {
    // Image RGBA : 4 octets par pixel.
    let pixels = (largeur as u64) * (hauteur as u64) * 4;
    // Masque binaire : 1 octet par pixel.
    let masque = (largeur as u64) * (hauteur as u64);
    // Structures de données (bbox, etc.) : estimation conservative.
    let overhead = 1024 * 1024; // 1 Mo

    pixels + masque + overhead
}

/// Vérifie que l'empreinte d'une tuile est dans le budget.
pub fn tuile_dans_budget(largeur: u32, hauteur: u32) -> bool {
    let empreinte = empreinte_tuile(largeur, hauteur);
    (empreinte / (1024 * 1024)) <= BUDGET_TUILE_MO
}

/// Calcule le nombre maximal de tuiles simultanées pour rester dans le budget.
pub fn tuiles_max_simultanees() -> u32 {
    let budget_octets = BUDGET_PIPELINE_MO * 1024 * 1024;
    let empreinte_2048 = empreinte_tuile(2048, 2048);

    if empreinte_2048 == 0 {
        return 1;
    }

    let max = budget_octets / empreinte_2048;
    max.max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuile_2048_dans_budget() {
        assert!(tuile_dans_budget(2048, 2048));
    }

    #[test]
    fn tuile_4096_depasse_budget() {
        // 4096x4096 RGBA = 64 Mo + masque 16 Mo + overhead = ~81 Mo > 64 Mo.
        assert!(!tuile_dans_budget(4096, 4096));
    }

    #[test]
    fn tuiles_max_raisonnables() {
        let max = tuiles_max_simultanees();
        // Avec un budget de 256 Mo et des tuiles de 2048x2048 (~21 Mo),
        // on devrait pouvoir faire au moins 4 tuiles en parallèle.
        assert!(max >= 4, "max={}, attendu >= 4", max);
    }
}
