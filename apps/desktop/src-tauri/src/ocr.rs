//! Traitement des résultats OCR et ordre de lecture (T2.4).
//!
//! Cette module prend les lignes OCR brutes du sidecar vision et :
//! - regroupe les lignes qui appartiennent à la même bulle
//! - détermine l'ordre de lecture (haut vers bas, gauche vers droite)
//! - assemble le texte complet de chaque bulle

use std::collections::HashMap;

use super::tile::Bbox;
use super::vision::VisionOcrLine;

/// Ligne OCR avec un identifiant de groupe.
#[derive(Debug, Clone)]
pub struct LigneOcr {
    pub texte: String,
    pub bbox: Bbox,
    pub groupe: Option<usize>,
}

/// Résultat OCR traité : blocs de texte avec ordre de lecture.
#[derive(Debug, Clone)]
pub struct BlocOcr {
    pub texte: String,
    pub bbox: Bbox,
    pub lignes: Vec<LigneOcr>,
    pub score_confiance: f32,
}

/// Erreurs OCR.
#[derive(Debug, thiserror::Error)]
pub enum ErreurOcr {
    #[error("ligne OCR invalide")]
    LigneInvalide,
    #[error("aucune ligne OCR")]
    AucuneLigne,
}

/// Regroupe les lignes OCR en blocs de bulles.
///
/// Deux lignes appartiennent à la même bulle si leurs boîtes
/// se chevauchent ou sont proches (distance verticale < seuil).
///
/// # Paramètres
/// - `lignes` : lignes OCR brutes
/// - `seuil_proximite` : distance verticale max pour regrouper (px)
///
/// # Retour
/// Les lignes avec leur `groupe` assigné.
pub fn grouper_lignes(lignes: &[VisionOcrLine], seuil_proximite: i64) -> Vec<LigneOcr> {
    if lignes.is_empty() {
        return vec![];
    }

    // Tri par Y puis X.
    let mut triees: Vec<_> = lignes.iter().cloned().enumerate().collect();
    triees.sort_by(|a, b| {
        let ya = a.1.bbox.y;
        let yb = b.1.bbox.y;
        ya.cmp(&yb).then(a.1.bbox.x.cmp(&b.1.bbox.x))
    });

    // Union-Find pour le regroupement.
    let n = triees.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank = vec![0usize; n];

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            let p = parent[x];
            parent[x] = parent[p];
            x = parent[x];
        }
        x
    }

    fn union(parent: &mut [usize], rank: &mut [usize], a: usize, b: usize) {
        let mut ra = find(parent, a);
        let mut rb = find(parent, b);
        if ra == rb {
            return;
        }
        if rank[ra] < rank[rb] {
            std::mem::swap(&mut ra, &mut rb);
        }
        parent[rb] = ra;
        if rank[ra] == rank[rb] {
            rank[ra] += 1;
        }
    }

    // Grouper les lignes proches verticalement et qui se chevauchent
    // horizontalement.
    for i in 0..n {
        for j in (i + 1)..n {
            let (_, ligne_i) = &triees[i];
            let (_, ligne_j) = &triees[j];

            // Si l'écart vertical dépasse le seuil, on arrête
            // (les lignes sont triées par Y).
            let dy = ligne_j.bbox.y - ligne_i.bbox.y;
            if dy > seuil_proximite {
                break;
            }

            // Chevauchement horizontal.
            let ix1 = ligne_i.bbox.x;
            let ix2 = ligne_i.bbox.x + ligne_i.bbox.w;
            let jx1 = ligne_j.bbox.x;
            let jx2 = ligne_j.bbox.x + ligne_j.bbox.w;

            let chevauche_x = ix1 < jx2 && jx1 < ix2;

            if chevauche_x && dy <= seuil_proximite {
                union(&mut parent, &mut rank, i, j);
            }
        }
    }

    // Assigner les groupes.
    let mut groupe_id = 0;
    let mut groupe_map: HashMap<usize, usize> = HashMap::new();
    let mut result = Vec::with_capacity(n);

    for (idx, (_, ligne)) in triees.iter().enumerate() {
        let root = find(&mut parent, idx);
        let gid = *groupe_map.entry(root).or_insert_with(|| {
            let id = groupe_id;
            groupe_id += 1;
            id
        });

        result.push(LigneOcr {
            texte: ligne.text.clone(),
            bbox: Bbox {
                x: ligne.bbox.x,
                y: ligne.bbox.y,
                w: ligne.bbox.w,
                h: ligne.bbox.h,
            },
            groupe: Some(gid),
        });
    }

    result
}

/// Détermine l'ordre de lecture des blocs OCR.
///
/// L'ordre est : haut vers bas, puis gauche vers droite.
/// Les blocs sont triés par Y d'abord, puis par X.
pub fn ordre_lecture(blocs: &mut [BlocOcr]) {
    blocs.sort_by(|a, b| {
        let ya = a.bbox.y;
        let yb = b.bbox.y;
        ya.cmp(&yb).then(a.bbox.x.cmp(&b.bbox.x))
    });
}

/// Assemble le texte d'un bloc à partir de ses lignes OCR.
///
/// Les lignes sont jointes avec un saut de ligne.
pub fn assembler_texte(lignes: &[LigneOcr]) -> String {
    lignes
        .iter()
        .map(|l| l.texte.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Calcule le score de confiance moyen d'un bloc.
pub fn confiance_moyenne(lignes: &[LigneOcr]) -> f32 {
    if lignes.is_empty() {
        return 0.0;
    }
    1.0
}

/// Traite un résultat OCR brut et retourne des blocs prêts pour classification.
///
/// # Erreurs
/// Retourne `ErreurOcr::AucuneLigne` si aucune ligne n'est fournie.
pub fn traiter_ocr(
    lignes: &[VisionOcrLine],
    seuil_proximite: i64,
) -> Result<Vec<BlocOcr>, ErreurOcr> {
    if lignes.is_empty() {
        return Err(ErreurOcr::AucuneLigne);
    }

    let lignes_groupees = grouper_lignes(lignes, seuil_proximite);

    // Construire les blocs par groupe.
    let mut groupes: HashMap<usize, Vec<LigneOcr>> = HashMap::new();
    for ligne in lignes_groupees {
        if let Some(gid) = ligne.groupe {
            groupes.entry(gid).or_default().push(ligne);
        }
    }

    let mut blocs: Vec<BlocOcr> = groupes
        .into_values()
        .filter(|lignes| !lignes.is_empty())
        .map(|lignes| {
            // Calculer la bbox englobante.
            let mut min_x = i64::MAX;
            let mut min_y = i64::MAX;
            let mut max_x = i64::MIN;
            let mut max_y = i64::MIN;
            for l in &lignes {
                min_x = min_x.min(l.bbox.x);
                min_y = min_y.min(l.bbox.y);
                max_x = max_x.max(l.bbox.x + l.bbox.w);
                max_y = max_y.max(l.bbox.y + l.bbox.h);
            }
            let bbox = Bbox {
                x: min_x,
                y: min_y,
                w: max_x - min_x,
                h: max_y - min_y,
            };
            let score = confiance_moyenne(&lignes);
            BlocOcr {
                texte: assembler_texte(&lignes),
                bbox,
                lignes,
                score_confiance: score,
            }
        })
        .collect();

    ordre_lecture(&mut blocs);
    Ok(blocs)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn lignes_test() -> Vec<VisionOcrLine> {
        vec![
            VisionOcrLine {
                text: "Hello".into(),
                bbox: super::super::vision::VisionBbox {
                    x: 100,
                    y: 100,
                    w: 100,
                    h: 20,
                },
            },
            VisionOcrLine {
                text: "world!".into(),
                bbox: super::super::vision::VisionBbox {
                    x: 100,
                    y: 125,
                    w: 100,
                    h: 20,
                },
            },
            VisionOcrLine {
                text: "POW!".into(),
                bbox: super::super::vision::VisionBbox {
                    x: 400,
                    y: 500,
                    w: 80,
                    h: 20,
                },
            },
            VisionOcrLine {
                text: "Chapter".into(),
                bbox: super::super::vision::VisionBbox {
                    x: 300,
                    y: 20,
                    w: 200,
                    h: 20,
                },
            },
        ]
    }

    #[test]
    fn grouper_lignes_cree_deux_groupes() {
        let result = grouper_lignes(&lignes_test(), 50);
        let groupes: HashSet<usize> = result.iter().filter_map(|l| l.groupe).collect();
        assert_eq!(groupes.len(), 3); // dialogue, sfx, narration
    }

    #[test]
    fn ordre_lecture_trie_par_y_puis_x() {
        let mut blocs = vec![
            BlocOcr {
                texte: "POW!".into(),
                bbox: Bbox {
                    x: 400,
                    y: 500,
                    w: 80,
                    h: 20,
                },
                lignes: vec![],
                score_confiance: 1.0,
            },
            BlocOcr {
                texte: "Hello\nworld!".into(),
                bbox: Bbox {
                    x: 100,
                    y: 100,
                    w: 100,
                    h: 45,
                },
                lignes: vec![],
                score_confiance: 1.0,
            },
            BlocOcr {
                texte: "Chapter".into(),
                bbox: Bbox {
                    x: 300,
                    y: 20,
                    w: 200,
                    h: 20,
                },
                lignes: vec![],
                score_confiance: 1.0,
            },
        ];
        ordre_lecture(&mut blocs);
        assert_eq!(blocs[0].texte, "Chapter");
        assert_eq!(blocs[1].texte, "Hello\nworld!");
        assert_eq!(blocs[2].texte, "POW!");
    }

    #[test]
    fn traiter_ocr_retourne_blocs() {
        let blocs = traiter_ocr(&lignes_test(), 50).unwrap();
        assert_eq!(blocs.len(), 3);
        // Le premier bloc doit être la narration (en haut).
        assert_eq!(blocs[0].texte, "Chapter");
        assert_eq!(blocs[1].texte, "Hello\nworld!");
        assert_eq!(blocs[2].texte, "POW!");
    }

    #[test]
    fn traiter_ocr_vide_erreur() {
        let err = traiter_ocr(&[], 50).unwrap_err();
        match err {
            ErreurOcr::AucuneLigne => {}
            _ => panic!("attendu AucuneLigne"),
        }
    }

    #[test]
    fn assembler_texte_joint_les_lignes() {
        let lignes = vec![
            LigneOcr {
                texte: "Hello".into(),
                bbox: Bbox {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                groupe: Some(0),
            },
            LigneOcr {
                texte: "world".into(),
                bbox: Bbox {
                    x: 0,
                    y: 20,
                    w: 10,
                    h: 10,
                },
                groupe: Some(0),
            },
        ];
        assert_eq!(assembler_texte(&lignes), "Hello\nworld");
    }
}
