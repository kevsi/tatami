//! Tests de contrat sur réponses **enregistrées** du moteur (tâche 5.3).
//!
//! Ce que ces tests prouvent, et rien d'autre :
//!
//! 1. Chaque réponse enregistrée est **valide** contre `contract/protocol.json`.
//! 2. Chaque réponse est **acceptée par le code Rust réel** — `into_result`,
//!    `is_transient`, `image::detect`, `PageCache` — pas par une réécriture de
//!    la logique dans le test.
//! 3. Chaque méthode de l'`enum` `method` du schéma a **au moins une** réponse
//!    enregistrée.
//!
//! Ce qu'ils ne font pas : toucher au réseau. Une réponse du moteur enregistrée
//! vaut mieux qu'une réponse imaginée, mais elle ne prouve pas que le moteur
//! répondra la même chose demain — les sites cassent, c'est connu et documenté
//! (AGENTS.md §7). Ce que ces tests détectent, c'est **notre** code qui change
//! de forme : un champ renommé dans `Server.kt`, une enveloppe qui perd `id`,
//! une constante de version qui diverge de la fixture `hello`.
//!
//! ## La porte qui compte : les contre-exemples
//!
//! Un test de contrat qui n'accepte que des exemples valides ne prouve rien : il
//! passe aussi bien avec un validateur qui retourne toujours `Ok`. Chaque règle
//! du validateur a donc son contre-exemple, plus bas. Si le validateur n'arrive
//! pas à **rejeter** une réponse invalide, la porte est inexistante — et le
//! test qui le vérifie doit échouer.
//!
//! Cf. `fixtures/README.md` pour l'origine des réponses et l'anonymisation
//! (AGENTS.md §2.7).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use tatami_desktop::commands::{ajouter_jetons_couverture, into_result};
use tatami_desktop::image;
use tatami_desktop::pagecache::PageCache;
use tatami_desktop::supervisor::{is_transient, PROTOCOL_MAJOR};

// ===========================================================================
// Chargement des fixtures
// ===========================================================================

fn dossier_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Le schéma du contrat. `src-tauri` est à 3 crans de la racine du dépôt
/// (`src-tauri` -> `desktop` -> `apps` -> racine).
fn schema() -> Value {
    let chemin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("contract")
        .join("protocol.json");
    let texte = std::fs::read_to_string(&chemin)
        .unwrap_or_else(|e| panic!("contrat introuvable : {} ({e})", chemin.display()));
    serde_json::from_str(&texte).expect("contract/protocol.json n'est pas du JSON valide")
}

struct Fixture {
    fichier: String,
    methode: String,
    enveloppe: Value,
}

fn charger() -> Vec<Fixture> {
    let dir = dossier_fixtures();
    assert!(
        dir.is_dir(),
        "dossier de fixtures absent : {}. Les réponses enregistrées sont la preuve ; \
         sans elles ce fichier ne teste rien.",
        dir.display()
    );
    let mut chemins: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("lecture du dossier de fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    // Tri explicite : l'ordre d'exécution ne doit pas dépendre du système de
    // fichiers, qui ne rend pas le même ordre sous Windows et sous Linux.
    chemins.sort();

    let mut fixtures = Vec::new();
    for chemin in chemins {
        let nom = chemin
            .file_name()
            .and_then(|n| n.to_str())
            .expect("nom de fixture")
            .to_string();
        let texte = std::fs::read_to_string(&chemin).expect("lecture de la fixture");
        let mut enveloppe: Value =
            serde_json::from_str(&texte).unwrap_or_else(|e| panic!("{nom} : JSON invalide ({e})"));

        // `_methode` et `_source` sont des annotations du dépôt, pas des champs
        // du protocole. Le schéma interdit les propriétés inconnues : sans ce
        // retrait, on validerait le fichier de notes, pas la réponse du moteur.
        let methode = {
            let objet = enveloppe.as_object_mut().expect("fixture = objet JSON");
            objet
                .remove("_methode")
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| panic!("{nom} : clé `_methode` absente"))
        };
        strip_annotations(&mut enveloppe);

        fixtures.push(Fixture {
            fichier: nom,
            methode,
            enveloppe,
        });
    }
    assert!(
        !fixtures.is_empty(),
        "aucune fixture trouvée : le test de contrat ne prouverait rien"
    );
    fixtures
}

/// Retire les annotations de dépôt (`_note`, `_tailleBase64Mesuree`,
/// `_octetsSubstitues`) : elles documentent l'enregistrement, elles ne font pas
/// partie du protocole.
fn strip_annotations(valeur: &mut Value) {
    match valeur {
        Value::Object(mappe) => {
            mappe.retain(|cle, _| !cle.starts_with('_'));
            for v in mappe.values_mut() {
                strip_annotations(v);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_annotations),
        _ => {}
    }
}

// ===========================================================================
// Validation : le sous-ensemble de JSON Schema dont le contrat a besoin
// ===========================================================================

// Réécrire ce validateur est un choix, pas un raccourci. L'alternative serait
// d'ajouter la dépendance `jsonschema`, ce qu'AGENTS.md §9 interdit sans ADR —
// et qui ferait entrer un arbre de dépendances dans le dépôt pour juger 16
// fichiers.
//
// Le sous-ensemble couvre **exactement** les mots-clés utilisés par
// `contract/protocol.json`. Un mot-clé inconnu du validateur n'est pas ignoré
// en silence : `le_validateur_couvre_tout_le_schema_du_contrat` échoue, et
// `valider_dans` panique sur un `type` non géré.

#[derive(Debug)]
struct Violation {
    chemin: String,
    message: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} : {}", self.chemin, self.message)
    }
}

/// Le schéma de `result` de chaque méthode, quand il existe.
///
/// **Trou trouvé le 2026-10-07** : `healthResult` était déclaré dans
/// `$defs` et **référencé par rien** — `response.result` est un simple
/// `{type: object}`. La validation passait donc la forme de ce résultat sans
/// jamais la juger. Le contre-exemple « score de détection au-dessus du
/// maximum » l'a rendu visible : il était accepté.
///
/// L'association méthode → schéma est donc écrite ici, et une porte vérifie
/// qu'elle couvre tout schéma de résultat du contrat — sinon un nouveau `$defs`
/// serait ajouté sans être appliqué, en silence.
///
/// Elle ne contient que `health` : les cinq entrées du sidecar vision
/// (`capabilities`, `detect`, `ocr`, `inpaint`, `loadPack`) ont été retirées
/// avec le sous-système de traduction, et leurs `$defs` avec elles.
/// `healthResult` était jusqu'au 2026-10-10 **exempté** de la porte, parce que
/// `health` rendait deux formes ; il n'en rend plus qu'une depuis le retrait de
/// la branche par source, donc l'exemption a disparu avec elle — le schéma est
/// désormais appliqué comme les autres.
const RESULTATS_PAR_METHODE: &[(&str, &str)] = &[
    ("health", "healthResult"),
    ("injecter", "injectResult"),
    ("latest", "mangaList"),
    ("search", "mangaList"),
    ("chapters", "chapterList"),
    ("pages", "pageList"),
    ("listSources", "sourceList"),
];

/// Le nom du `$defs` qui décrit le `result` d'une méthode, s'il existe.
fn schema_de_resultat(methode: &str) -> Option<&'static str> {
    RESULTATS_PAR_METHODE
        .iter()
        .find(|(m, _)| *m == methode)
        .map(|(_, d)| *d)
}

fn valider(valeur: &Value, schema: &Value) -> Vec<Violation> {
    let defs = schema.get("$defs").cloned().unwrap_or(Value::Null);
    let mut out = Vec::new();
    valider_dans(valeur, schema, "$", &defs, &mut out);
    out
}

/// Valide une enveloppe **et** la forme de son `result` pour la méthode
/// déclarée. Sans le second temps, la validation est structurelle seulement.
fn valider_enveloppe(methode: &str, enveloppe: &Value, schema: &Value) -> Vec<Violation> {
    let mut out = valider(enveloppe, schema);
    // `result` peut être un **tableau** : `latest`, `search`, `chapters`, `pages`
    // et `listSources` rendent tous une liste. Le filtre `is_object()` les
    // supprimait donc silencieusement — c'est-à-dire que `manga`, `chapter`,
    // `page` et `sourceInfo` n'étaient validés par aucune fixture, alors que
    // leurs `$defs` étaient présents dans le schéma et semblaient appliqués.
    let (Some(nom), Some(resultat)) = (
        schema_de_resultat(methode),
        enveloppe
            .get("result")
            .filter(|v| v.is_object() || v.is_array()),
    ) else {
        return out;
    };
    let defs = schema.get("$defs").cloned().unwrap_or(Value::Null);
    let sous_schema = defs
        .get(nom)
        .unwrap_or_else(|| panic!("$defs/{nom} absent du schéma"));
    valider_dans(resultat, sous_schema, "$.result", &defs, &mut out);
    out
}

fn valider_dans(
    valeur: &Value,
    schema: &Value,
    chemin: &str,
    defs: &Value,
    out: &mut Vec<Violation>,
) {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        let nom = r
            .strip_prefix("#/$defs/")
            .unwrap_or_else(|| panic!("$ref non géré par le test : {r}"));
        let cible = defs
            .get(nom)
            .unwrap_or_else(|| panic!("$defs/{nom} absent du schéma"));
        valider_dans(valeur, cible, chemin, defs, out);
        return;
    }

    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        let matchs: Vec<usize> = branches
            .iter()
            .enumerate()
            .filter(|(_, b)| valider_silencieux(valeur, b, defs))
            .map(|(i, _)| i)
            .collect();
        match matchs.len() {
            1 => return,
            0 => {
                out.push(Violation {
                    chemin: chemin.into(),
                    message: "ne correspond à aucune branche du oneOf".into(),
                });
                return;
            }
            n => panic!(
                "{chemin} correspond à {n} branches du oneOf : le schéma est ambigu, \
                 la validation ne prouve donc plus rien"
            ),
        }
    }

    let nom_reel = match valeur {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    // `type` accepte une chaîne **ou** un tableau de chaînes. Le second cas a été
    // ajouté le 2026-10-15 quand la validation a enfin atteint les données
    // réelles : `manga.coverUrl` vaut `null` quand la source ne déclare pas de
    // couverture, et dire `{"type":"string"}` était faux. Corriger le schéma
    // imposait alors de savoir lire un `type` en liste — sans quoi le mot-clé
    // aurait été **ignoré silencieusement**, c'est-à-dire que la correction
    // aurait désactivé la règle au lieu de l'assouplir.
    let attendus: Vec<&str> = match schema.get("type") {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(listes)) => listes.iter().filter_map(Value::as_str).collect::<Vec<_>>(),
        Some(autre) => panic!("`type` non géré par le test : {autre}"),
        None => Vec::new(),
    };
    if !attendus.is_empty() {
        let ok = attendus.iter().any(|attendu| match *attendu {
            "object" => valeur.is_object(),
            "array" => valeur.is_array(),
            "string" => valeur.is_string(),
            "boolean" => valeur.is_boolean(),
            "integer" => valeur.is_i64() || valeur.is_u64(),
            "number" => valeur.is_number(),
            "null" => valeur.is_null(),
            autre => panic!("`type` non géré par le test : {autre}"),
        });
        if !ok {
            out.push(Violation {
                chemin: chemin.into(),
                message: format!("type attendu `{attendus:?}`, reçu `{nom_reel}`"),
            });
            return; // descendre plus bas ne donnerait que des violations en cascade
        }
    }

    if let Some(choix) = schema.get("enum").and_then(Value::as_array) {
        if !choix.contains(valeur) {
            let attendus: Vec<&str> = choix.iter().filter_map(Value::as_str).collect();
            out.push(Violation {
                chemin: chemin.into(),
                message: format!(
                    "`{}` absent de l'enum {attendus:?}",
                    valeur.as_str().unwrap_or("<non-chaîne>")
                ),
            });
        }
    }

    if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
        if let Some(n) = valeur.as_f64() {
            if n < min {
                out.push(Violation {
                    chemin: chemin.into(),
                    message: format!("{n} < minimum {min}"),
                });
            }
        }
    }

    // `maximum` est le pendant de `minimum` : ajouté le 2026-10-07 avec
    // `detectResult` (score dans [0, 1], classe dans [0, 2]). La porte
    // `le_validateur_couvre_tout_le_schema_du_contrat` l'a nommé — et son avertissement
    // est le bon : un mot-clé non géré rend la validation silencieuse, donc
    // elle passe toujours.
    if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
        if let Some(n) = valeur.as_f64() {
            if n > max {
                out.push(Violation {
                    chemin: chemin.into(),
                    message: format!("{n} > maximum {max}"),
                });
            }
        }
    }

    if let (Some(items), Some(minimum)) = (
        valeur.as_array(),
        schema.get("minItems").and_then(Value::as_u64),
    ) {
        if items.len() < minimum as usize {
            out.push(Violation {
                chemin: chemin.into(),
                message: format!("{} entrée(s) < minItems {minimum}", items.len()),
            });
        }
    }

    if let (Some(obj), Some(props)) = (
        valeur.as_object(),
        schema.get("properties").and_then(Value::as_object),
    ) {
        for (cle, sous_schema) in props {
            if let Some(v) = obj.get(cle) {
                valider_dans(v, sous_schema, &format!("{chemin}.{cle}"), defs, out);
            }
        }
        // `additionalProperties: false` : c'est cette règle qui a attrapé
        // `latest` et `fetchCover` (implémentées, utilisées, absentes de l'enum
        // `method` — 2026-10-04). Elle est donc testée deux fois : ici par
        // les fixtures, et explicitement par
        // `le_validateur_refuse_les_reponses_invalides`.
        if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
            for cle in obj.keys() {
                if !props.contains_key(cle) {
                    out.push(Violation {
                        chemin: format!("{chemin}.{cle}"),
                        message: "propriété inconnue (additionalProperties: false)".into(),
                    });
                }
            }
        }
    }

    if let Some(items) = valeur.as_array() {
        if let Some(sous_schema) = schema.get("items") {
            for (i, v) in items.iter().enumerate() {
                valider_dans(v, sous_schema, &format!("{chemin}[{i}]"), defs, out);
            }
        }
    }

    if let Some(requis) = schema.get("required").and_then(Value::as_array) {
        if let Some(obj) = valeur.as_object() {
            for cle in requis.iter().filter_map(Value::as_str) {
                if !obj.contains_key(cle) {
                    out.push(Violation {
                        chemin: chemin.into(),
                        message: format!("champ obligatoire absent : `{cle}`"),
                    });
                }
            }
        }
    }
}

fn valider_silencieux(valeur: &Value, schema: &Value, defs: &Value) -> bool {
    let mut out = Vec::new();
    valider_dans(valeur, schema, "$", defs, &mut out);
    out.is_empty()
}

// ===========================================================================
// Porte 1 — le validateur couvre bien le fichier de contrat qu'on lui donne
// ===========================================================================

#[test]
fn le_validateur_couvre_tout_le_schema_du_contrat() {
    let s = schema();
    let defs = s.get("$defs").and_then(Value::as_object).expect("$defs");

    let mut vus: BTreeSet<&str> = BTreeSet::new();
    for v in defs.values() {
        collecter_mots_cles(v, &mut vus);
    }

    const GERES: &[&str] = &[
        "$ref",
        "additionalProperties",
        "description",
        "enum",
        "items",
        "maximum",
        "minItems",
        "minimum",
        "oneOf",
        "properties",
        "required",
        "title",
        "type",
    ];
    let non_geres: Vec<&str> = vus.iter().copied().filter(|m| !GERES.contains(m)).collect();

    assert!(
        non_geres.is_empty(),
        "contract/protocol.json utilise des mots-clés que le validateur du test ne gère \
         pas : {non_geres:?}. Les implémenter dans `valider_dans`, ou les retirer du \
         schéma. Ne surtout pas les ignorer : le test de contrat validerait alors des \
         réponses qu'il ne sait pas juger, et il passerait toujours."
    );
}

#[test]
fn les_domaines_source_sont_optionnels_mais_non_vides() {
    let s = schema();
    let defs = s.get("$defs").and_then(Value::as_object).expect("$defs");
    let source_info = defs.get("sourceInfo").expect("$defs/sourceInfo");

    assert!(valider(&serde_json::json!({"name": "MANGALIB"}), source_info).is_empty());
    assert!(valider(
        &serde_json::json!({"name": "MANGALIB", "domains": ["mangalib.org"]}),
        source_info,
    )
    .is_empty());
    let vide = valider(
        &serde_json::json!({"name": "MANGALIB", "domains": []}),
        source_info,
    );
    assert_eq!(vide.len(), 1, "une liste vide doit être refusée : {vide:?}");
    assert!(vide[0].message.contains("minItems 1"), "{vide:?}");
}

/// Collecte les mots-clés **du schéma**, en descendant dans les sous-schémas.
///
/// Le piège : dans un objet `properties`, les clés sont des NOMS DE CHAMPS
/// (`url`, `title`, `authors`…), pas des mots-clés. Les collecter ferait
/// crier la porte avec une liste de 24 noms de champs — j'ai fait l'erreur en
/// écrivant ce test la première fois. On ne descend donc dans `properties`
/// que par ses valeurs.
fn collecter_mots_cles<'a>(valeur: &'a Value, out: &mut BTreeSet<&'a str>) {
    match valeur {
        Value::Object(mappe) => {
            for (cle, v) in mappe {
                if cle == "properties" {
                    if let Some(sous_schemas) = v.as_object() {
                        for sous_schema in sous_schemas.values() {
                            collecter_mots_cles(sous_schema, out);
                        }
                    }
                } else {
                    out.insert(cle.as_str());
                    collecter_mots_cles(v, out);
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|i| collecter_mots_cles(i, out)),
        _ => {}
    }
}

/// Chaque schéma de résultat du contrat est **appliqué** par une méthode.
///
/// Le trou du 2026-10-07 : `detectResult` et consorts étaient dans `$defs`
/// sans que rien ne les référence. Cette porte empêche qu'un `$defs` de résultat
/// soit ajouté puis oublié — il passerait inaperçu, et la validation donnerait
/// l'illusion de juger.
#[test]
fn chaque_schema_de_resultat_est_attribue_a_une_methode() {
    let s = schema();
    let defs = s.get("$defs").and_then(Value::as_object).expect("$defs");
    // Un `$defs` nommé `*Result` est, par convention du contrat, la forme d'un
    // `result`. Le nom est donc la convention ; la porte la vérifie.
    // Aucune exemption ici depuis le 2026-10-10 : `healthResult` était exclu
    // parce que `health` rendait deux formes et que le moteur comme le sidecar
    // vision en rendaient d'autres. Une seule forme reste, donc il est attribué
    // comme les autres.
    let resultats: Vec<&str> = defs
        .keys()
        .filter(|k| k.ends_with("Result"))
        .map(String::as_str)
        .collect();
    let attribues: BTreeSet<&str> = RESULTATS_PAR_METHODE.iter().map(|(_, d)| *d).collect();

    let orphans: Vec<&&str> = resultats
        .iter()
        .filter(|r| !attribues.contains(*r))
        .collect();
    assert!(
        orphans.is_empty(),
        "schémas de résultat jamais appliqués : {orphans:?}. Un `$defs` de résultat \
         non référencé passe la validation sans être jugée — c'est exactement le trou \
         du 2026-10-07 sur `detectResult`."
    );

    // Et l'inverse : une attribution qui pointe un `$defs` absent planterait
    // chaque validation, donc on le vérifie ici, une fois, avec un message clair.
    for (methode, nom) in RESULTATS_PAR_METHODE {
        assert!(
            defs.contains_key(*nom),
            "`{methode}` pointe vers `$defs/{nom}`, absent du schéma"
        );
    }
}

// ===========================================================================
// Porte 2 — chaque réponse enregistrée est valide contre le schéma
// ===========================================================================

#[test]
fn chaque_reponse_enregistree_valide_le_schema() {
    let s = schema();
    for f in charger() {
        let violations = valider_enveloppe(&f.methode, &f.enveloppe, &s);
        assert!(
            violations.is_empty(),
            "{} (méthode {}) n'est pas valide contre contract/protocol.json :\n  - {}",
            f.fichier,
            f.methode,
            violations
                .iter()
                .map(Violation::to_string)
                .collect::<Vec<_>>()
                .join("\n  - ")
        );
    }
}

#[test]
fn le_nombre_de_reponses_enregistrees_est_stable() {
    // Une fixture supprimée en accident ferait passer `chaque_methode_du_schema_a_une_reponse`
    // pour une méthode orpheline… non : elle le ferait échouer. Mais une fixture
    // perdue doit se voir tout de suite, pas dans six mois.
    let n = charger().len();
    // 15 réponses du moteur de sources. Elles ont été 22 jusqu'au
    // 2026-10-08 : les 6 du sidecar vision (`hello`, `loadPack`, `detect`,
    // `ocr`, `inpaint`, `capabilities`) sont parties avec le sous-système de
    // traduction, dont la trace reste dans l'historique git.
    // Puis 16 jusqu'au 2026-10-10 : `14-health-source.json` est partie avec la
    // forme par source de `health` (voir fixtures/README.md).
    // Puis 15 : cette fixture supprimée a libéré le numéro 14.
    // Puis 16 le 2026-10-15 : `14-injecter.json`, réponse **réellement capturée**
    // (sonde NDJSON sur `engine/dist`) le jour où l'on a constaté que `injecter`
    // était implémentée, appelée par `challenge.rs`, et absente de l'`enum` du
    // contrat — donc hors contrat par construction.
    assert_eq!(
        n, 16,
        "16 réponses enregistrées attendues (moteur de sources). Changer ce nombre est \
         normal (nouvelle méthode, nouveau cas mesuré) — mais il faut le faire exprès et le \
         noter dans fixtures/README.md."
    );
}

// ===========================================================================
// Porte 3 — contre-exemples : le validateur doit REFUSER
// ===========================================================================

/// Chaque ligne est un mensonge plausible du moteur. Le test échoue si le
/// validateur l'accepte : une porte qui n'a jamais rien trouvé n'est pas une
/// porte (leçon mesurée le 2026-10-04 sur `gradle test`, vert avec 0 fichier
/// de résultat).
#[test]
fn le_validateur_refuse_les_reponses_invalides() {
    let s = schema();
    let cas: Vec<(&str, Value)> = vec![
        (
            "enveloppe sans `id`",
            serde_json::json!({ "ok": true, "result": {} }),
        ),
        (
            "enveloppe sans `ok`",
            serde_json::json!({ "id": "x", "result": {} }),
        ),
        (
            "`ok` en chaîne au lieu du booléen",
            serde_json::json!({ "id": "x", "ok": "true", "result": {} }),
        ),
        (
            "`error` sans `message`",
            serde_json::json!({ "id": "x", "ok": false, "error": { "code": "TIMEOUT" } }),
        ),
        (
            "code d'erreur hors de l'enum",
            serde_json::json!({
                "id": "x", "ok": false,
                "error": { "code": "TEAPOT", "message": "m" }
            }),
        ),
        (
            "propriété inconnue à la racine",
            serde_json::json!({ "id": "x", "ok": true, "result": {}, "extra": 1 }),
        ),
        (
            "`result` qui n'est pas un objet",
            serde_json::json!({ "id": "x", "ok": true, "result": [] }),
        ),
        // Contre-exemple du mot-clé `minimum` : sans lui, le validateur
        // laisserait passer un délai nul — donc une porte qui aurait l'air verte
        // et ne jugerait rien. Il remplace le contre-exemple de `maximum` sur le
        // score de détection, retiré avec `detectResult`.
        (
            "`timeoutMs` nul, sous le minimum",
            serde_json::json!({ "id": "x", "method": "hello", "params": {}, "timeoutMs": 0 }),
        ),
        // Contre-exemples de la forme de `result` de `health`. La branche par
        // source (`source` + `reachable`) a été retirée le 2026-10-10 : elle
        // répondait `reachable: true` sans aucune requête. Sans ces cas, la
        // validation du `result` pourrait ne rien voir, et
        // `additionalProperties: false` serait une décoration.
        //
        // Le PREMIER est celui qui compte : c'est la réponse que le moteur
        // rendait avant le retrait, et que le nouveau schéma doit refuser.
        // S'il était encore accepté, le changement de contrat n'aurait rien
        // changé du tout.
        (
            "health : la forme par source, retirée le 2026-10-10",
            serde_json::json!({
                "id": "x", "ok": true,
                "result": { "source": "MANGADEX", "reachable": true }
            }),
        ),
        // Et le mélange des deux formes, refusé par les deux bornes.
        (
            "health : forme globale et forme par source mélangées",
            serde_json::json!({
                "id": "x", "ok": true,
                "result": { "ok": true, "source": "X", "reachable": true }
            }),
        ),
    ];

    for (nom, enveloppe) in cas {
        // Le dernier cas concerne la forme d'un `result` : il passe par
        // `valider_enveloppe`, sinon la méthode serait inconnue et le schéma de
        // résultat ne s'appliquerait pas — donc le contre-exemple ne prouverait
        // rien du tout.
        let methode = if nom.starts_with("health") {
            "health"
        } else {
            ""
        };
        let violations = valider_enveloppe(methode, &enveloppe, &s);
        assert!(
            !violations.is_empty(),
            "le validateur a ACCEPTÉ « {nom} ». Le test de contrat ne prouve donc rien : \
             il passe aussi bien avec un validateur qui accepte tout.\n  rejet attendu : {nom}"
        );
    }
}

/// **Écart trouvé en écrivant ce test**, et donc consigné plutôt que corrigé en
/// douce : `contract/protocol.json` exige `id` et `ok`, mais **n'exige pas
/// `error` quand `ok` vaut `false`**. Une enveloppe `{id, ok:false}` sans
/// `error` est donc valide, alors que `into_result` en déduit `INTERNAL` avec
/// un message **vide**.
///
/// Ce test ne corrige pas le schéma : AGENTS.md §5 impose une ADR pour toute
/// évolution du contrat, et `protocol.json` est partagé avec le moteur. Il
/// **fige** le comportement réel pour qu'il ne change pas par accident, et il
/// rend l'écart visible dans le rapport `cargo test` plutôt que dans un bug
/// d'utilisateur. La correction (un `if/then` sur le schéma, ou une
/// `$defs` par cas) est listée dans PLAN.md comme point ouvert de 5.3.
#[test]
fn le_schema_admet_une_erreur_sans_bloc_error_et_nous_le_savons() {
    let s = schema();
    let enveloppe = serde_json::json!({ "id": "x", "ok": false });

    assert!(
        valider(&enveloppe, &s).is_empty(),
        "le schéma EXIGE maintenant `error` quand `ok: false`. C'est la correction \
         attendue — alors mets à jour ce test et l'ADR, et ajoute le cas à \
         `le_validateur_refuse_les_reponses_invalides`."
    );

    // Le comportement de repli de `into_result`, lui, est vérifié : c'est lui
    // qui produit ce que l'utilisateur lit.
    let message = into_result(enveloppe).expect_err("ok=false doit échouer");
    assert_eq!(
        message, "INTERNAL: ",
        "sans bloc `error`, `into_result` doit retomber sur INTERNAL avec un message vide \
         (et non inventer un code). Message reçu : {message:?}"
    );
}

/// Le même travail, mais sur la **forme** de `result` — c'est là que vivent les
/// défauts réels : une liste qui devient un objet, un champ `url` qui disparaît
/// d'un manga, un `index` de page négatif.
#[test]
fn le_validateur_refuse_les_resultats_deformes() {
    let s = schema();
    let defs = s.get("$defs").cloned().unwrap_or(Value::Null);
    let resultat = s
        .get("$defs")
        .and_then(|d| d.get("manga"))
        .cloned()
        .expect("$defs.manga");

    let cas: Vec<(&str, Value)> = vec![
        (
            "manga sans `url`",
            serde_json::json!({ "title": "t", "source": "S" }),
        ),
        (
            "manga sans `source`",
            serde_json::json!({ "url": "u", "title": "t" }),
        ),
        (
            "`title` en nombre",
            serde_json::json!({ "url": "u", "title": 42, "source": "S" }),
        ),
        (
            "`authors` en chaîne au lieu d'un tableau",
            serde_json::json!({ "url": "u", "title": "t", "source": "S", "authors": "moi" }),
        ),
    ];
    for (nom, manga) in cas {
        assert!(
            !valider_silencieux(&manga, &resultat, &defs),
            "le validateur a ACCEPTÉ « {nom} » contre $defs.manga"
        );
    }

    let page = s
        .get("$defs")
        .and_then(|d| d.get("page"))
        .cloned()
        .expect("$defs.page");
    let cas_page: Vec<(&str, Value)> = vec![
        (
            "page sans `index`",
            serde_json::json!({ "url": "https://exemple.test/p" }),
        ),
        (
            "`index` négatif (minimum: 0)",
            serde_json::json!({ "index": -1, "url": "https://exemple.test/p" }),
        ),
        ("page sans `url`", serde_json::json!({ "index": 0 })),
    ];
    for (nom, p) in cas_page {
        assert!(
            !valider_silencieux(&p, &page, &defs),
            "le validateur a ACCEPTÉ « {nom} » contre $defs.page"
        );
    }
}

/// La porte qui aurait attrapé le défaut du 2026-10-04 : `latest` et
/// `fetchCover` étaient implémentées, utilisées par l'interface, et **absentes**
/// de l'`enum` `method`. Une méthode de l'enum sans réponse enregistrée est le
/// symptôme exact de ce genre d'écart, dans l'autre sens.
#[test]
fn chaque_methode_du_schema_a_une_reponse_enregistree() {
    let s = schema();
    let methodes: Vec<&str> = s
        .get("$defs")
        .and_then(|d| d.get("method"))
        .and_then(|m| m.get("enum"))
        .and_then(Value::as_array)
        .expect("enum method")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        methodes.len() >= 10,
        "l'enum `method` du contrat ne contient que {} méthodes : le schéma a été tronqué ?",
        methodes.len()
    );

    let fixtures = charger();
    let vues: BTreeSet<&str> = fixtures.iter().map(|f| f.methode.as_str()).collect();
    let sans_reponse: Vec<&&str> = methodes.iter().filter(|m| !vues.contains(*m)).collect();

    assert!(
        sans_reponse.is_empty(),
        "méthodes du contrat sans réponse enregistrée : {sans_reponse:?}. \
         Le schéma annonce une méthode que rien ne prouve : c'est exactement l'écart \
         `latest`/`fetchCover` du 2026-10-04, en miroir. Enregistrer une réponse, ou \
         sortir la méthode du contrat — mais pas laisser le contrat seul."
    );
}

/// L'inverse : une réponse enregistrée pour une méthode que le contrat ignore.
/// Elle ne viendrait jamais du moteur tel que le contrat le décrit.
#[test]
fn chaque_reponse_enregistree_decrit_une_methode_du_schema() {
    let s = schema();
    let methodes: Vec<&str> = s
        .get("$defs")
        .and_then(|d| d.get("method"))
        .and_then(|m| m.get("enum"))
        .and_then(Value::as_array)
        .expect("enum method")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for f in charger() {
        assert!(
            methodes.contains(&f.methode.as_str()),
            "{} décrit la méthode `{}`, absente de l'enum `method` du contrat. Le moteur \
             produit quelque chose que le contrat interdit : corriger le schéma (avec ADR) \
             ou la fixture.",
            f.fichier,
            f.methode
        );
    }
}

// ===========================================================================
// Porte 4 — rejouer les réponses contre le CODE RUST
// ===========================================================================

/// `into_result` est le seul point où une enveloppe devient un résultat ou une
/// erreur. Une régression ici ne casse rien à la compilation : elle vide un
/// écran.
#[test]
fn chaque_reponse_enregistree_passe_dans_into_result() {
    for f in charger() {
        let attendu_ok = f.enveloppe.get("ok").and_then(Value::as_bool) == Some(true);
        let obtenu = into_result(f.enveloppe.clone());

        match (attendu_ok, obtenu) {
            (true, Ok(resultat)) => {
                assert!(
                    resultat.is_object(),
                    "{} : ok=true doit produire un `result` objet, reçu {}",
                    f.fichier,
                    type_name(&resultat)
                );
            }
            (false, Err(message)) => {
                // Format documenté dans supervisor.rs : « CODE: message ».
                // C'est ce format que `is_transient` analyse, donc le reste
                // du code en dépend.
                assert!(
                    message.contains(':'),
                    "{} : l'erreur doit être « CODE: message », reçu {message:?}. Toute \
                     classification d'erreur en aval lit ce format.",
                    f.fichier
                );
                let code = message.split(':').next().unwrap_or_default().trim();
                let schema_contrat = schema();
                let codes: Vec<&str> = schema_contrat
                    .get("$defs")
                    .and_then(|d| d.get("errorCode"))
                    .and_then(|c| c.get("enum"))
                    .and_then(Value::as_array)
                    .expect("enum errorCode")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                assert!(
                    codes.contains(&code),
                    "{} : code d'erreur `{code}` absent de l'enum du contrat {codes:?}",
                    f.fichier
                );
            }
            (true, Err(e)) => panic!("{} : ok=true mais into_result a échoué ({e})", f.fichier),
            (false, Ok(r)) => panic!(
                "{} : ok=false mais into_result a rendu un résultat ({})",
                f.fichier,
                type_name(&r)
            ),
        }
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Les deux erreurs enregistrées doivent être **définitives** : rejouer une
/// source morte ou une 404 en boucle transformerait un échec net enполное
/// silence côté utilisateur, avec deux tentatives qui plus rien.
#[test]
fn les_erreurs_enregistrees_ne_sont_pas_rejouees() {
    let erreurs: Vec<Fixture> = charger()
        .into_iter()
        .filter(|f| f.enveloppe.get("ok") == Some(&Value::Bool(false)))
        .collect();
    assert!(
        erreurs.len() >= 2,
        "il faut au moins deux réponses d'erreur enregistrées (mesurées le 2026-10-05 : \
         source inconnue -> PARSE_ERROR, URL vide -> INTERNAL 404)"
    );

    for f in erreurs {
        let message = into_result(f.enveloppe.clone()).expect_err("ok=false doit échouer");
        assert!(
            !is_transient(&Err(message.clone())),
            "{} : `is_transient` rejouerait « {message} ». Une source morte serait alors \
             rejouée 3 fois avant d'abandonner, et une 404 à chaque fois.",
            f.fichier
        );
    }
}

/// L'inverse de la précédente, sur le **message** seulement : c'est bien le
/// texte que `is_transient` lit, pas l'enveloppe.
#[test]
fn is_transient_reconnait_un_panneau_reel() {
    // Non mesuré sur un site : c'est le behavior attendu de la liste de
    // marqueurs (supervisor.rs), vérifié pour que la porte ci-dessus ait un
    // témoin des deux côtés.
    assert!(is_transient(&Err(
        "INTERNAL: java.net.UnknownHostException: mangadex.org".into()
    )));
    assert!(is_transient(&Err("RATE_LIMITED: trop de requêtes".into())));
    assert!(is_transient(&Err("timeout 60s sur latest".into())));
    assert!(!is_transient(&Err("PARSE_ERROR: selecteur cassé".into())));
    assert!(!is_transient(&Err(
        "CHALLENGE_REQUIRED: https://exemple.test".into()
    )));
    assert!(!is_transient(&Ok(Value::Null)));
}

/// Un rejeu refusé ne doit **pas** être rejoué : la vérification a déjà eu
/// lieu et le site refuse quand même, donc une nouvelle tentative coûterait le
/// réseau pour reproduire le même 403.
///
/// **Pourquoi le second message contient un marqueur transitoire.** Le message
/// réel (`CHALLENGE_UNREPLAYABLE: challenge at … (non rejouable)`) ne contient
/// *aucun* marqueur transitoire : `is_transient` le classerait non-transitoire
/// **par défaut**, garde retirée ou non — le test passerait donc sans prouver
/// rien. Mesuré le 2026-10-11 : c'est exactement ce qu'a fait la première
/// version de ce test, verte garde enlevée.
///
/// Le second message porte **les deux**. Sans la garde, le `503 Service
/// Unavailable` le ferait rejouer ; avec elle, le code gagne. C'est la seule
/// forme qui éprouve réellement la garde.
#[test]
fn un_rejeu_refuse_nest_pas_transitoire() {
    // Le message que le moteur émet en pratique.
    assert!(!is_transient(&Err(
        "CHALLENGE_UNREPLAYABLE: challenge at https://exemple.test (non rejouable)".into()
    )));
    // Le témoin : même code + un marqueur transitoire. **La garde doit gagner**,
    // sinon la source serait rejouée jusqu'à l'épuisement des tentatives.
    assert!(!is_transient(&Err(
        "CHALLENGE_UNREPLAYABLE: 503 Service Unavailable".into()
    )));
}

/// Le `hello` enregistré doit passer la porte de version que le superviseur
/// applique au démarrage. C'est la seule vérification qui relie une constante
/// de notre code à une réponse du moteur.
#[test]
fn le_hello_enregistre_est_accepte_par_la_porte_de_version() {
    let hello = charger()
        .into_iter()
        .find(|f| f.fichier.starts_with("01-hello"))
        .expect("fixture hello absente");
    let resultat = into_result(hello.enveloppe).expect("hello doit réussir");
    let version = resultat
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("hello sans protocolVersion : {}", resultat));
    let majeur = version.split('.').next().unwrap_or_default();

    assert_eq!(
        majeur, PROTOCOL_MAJOR,
        "la fixture `hello` annonce protocolVersion `{version}` (majeur `{majeur}`) alors que \
         le superviseur n'accepte que le majeur `{PROTOCOL_MAJOR}`. L'application refuserait \
         de démarrer. Soit le moteur a changé de version majeure — ce qui exige une ADR \
         (AGENTS.md §5) — soit cette fixture est périmée."
    );

    // Le `hello` porte aussi l'agent utilisateur réinjecté dans la fenêtre de
    // résolution : une clearance n'est émise que pour un agent donné.
    let ua = resultat
        .get("defaultUserAgent")
        .and_then(Value::as_str)
        .expect("hello sans defaultUserAgent");
    assert!(
        !ua.is_empty(),
        "defaultUserAgent vide : la fenêtre de résolution parlerait avec un agent différent \
         de celui du moteur, et la clearance serait refusée"
    );
}

// ===========================================================================
// Porte 5 — les images : le contentType annoncé ne doit jamais décider
// ===========================================================================

/// Régression du 2026-10-04 : une source répondait `200 OK` avec la page HTML de
/// son interstitiel de vérification d'âge, et le `contentType` annoncé
/// (`image/jpeg`) a fait écrire 37 chapitres sous le nom `001.jpg`. Rust déduit
/// le type **des octets** (`image::detect`), jamais de l'en-tête.
///
/// La fixture `fetchCover` porte volontairement un `contentType: image/jpeg`
/// et des octets PNG : c'est le désaccord exact qu'il faut savoir pardonner.
/// `fetchImage` annonce `image/png` **et** contient du PNG — la concordance y
/// est normale, et c'est le désaccord de `fetchCover` qui porte le test.
#[test]
fn le_type_dechduit_vient_des_octets_pas_du_contenttype() {
    // (fixture, type des octets, contentType annoncé, contentType réel des octets)
    for (nom, kind, annonce, reel) in [
        // Concordance : l'en-tête dit la vérité, rien à pardonner.
        (
            "08-fetchImage",
            image::ImageKind::Png,
            "image/png",
            "image/png",
        ),
        // L'en-tête MENT (le cas mesuré) : les octets disent PNG.
        (
            "09-fetchCover",
            image::ImageKind::Png,
            "image/jpeg",
            "image/png",
        ),
    ] {
        let f = charger()
            .into_iter()
            .find(|f| f.fichier.starts_with(nom))
            .unwrap_or_else(|| panic!("fixture {nom} absente"));
        let resultat = into_result(f.enveloppe).expect("réponse d'image valide");
        let data = resultat
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{nom} : `data` absent"));
        let content_type = resultat
            .get("contentType")
            .and_then(Value::as_str)
            .expect("contentType absent");
        assert_eq!(
            content_type, annonce,
            "{nom} : le `contentType` enregistré a changé. C'est une réponse du moteur : \
             vérifier qu'on n'a pas modifié la fixture."
        );

        let octets = decharger_base64(data, &f.fichier);
        let detecte = image::detect(&octets).unwrap_or_else(|| {
            panic!(
                "{nom} : les octets ne sont pas une image reconnaissable. Le test de contrat \
                    ne teste alors plus rien sur ce chemin."
            )
        });

        assert_eq!(
            detecte, kind,
            "{nom} : l'image synthétique substituée n'est plus un PNG"
        );
        assert_eq!(
            detecte.content_type(),
            reel,
            "{nom} : `image::detect` ne lit plus la bonne signature"
        );

        // **L'extension écrite suit les octets, jamais l'en-tête.** C'est
        // l'assertion centrale : elle passe pour les deux fixtures, y compris
        // quand l'en-tête ment.
        assert_eq!(
            detecte.extension(),
            "png",
            "{nom} : l'extension écrite doit suivre les octets, pas le `contentType` annoncé"
        );
    }

    // Le désaccord doit rester dans les fixtures, sinon ce test ne couvre plus
    // rien : c'est la régression du 2026-10-04 (37 `.cbz` illisibles).
    let desaccords: Vec<&str> = ["08-fetchImage", "09-fetchCover"]
        .iter()
        .filter(|nom| {
            charger()
                .into_iter()
                .find(|f| f.fichier.starts_with(**nom))
                .and_then(|f| into_result(f.enveloppe).ok())
                .and_then(|r| {
                    r.get("contentType")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .is_some_and(|ct| ct != "image/png")
        })
        .copied()
        .collect();
    assert!(
        !desaccords.is_empty(),
        "plus aucune fixture dont le `contentType` ment. Le défaut mesuré le 2026-10-04 \
         (un site sert du HTML en annonçant une image) n'est plus couvert : rétablir au \
         moins `09-fetchCover` avec un en-tête `image/jpeg` et des octets PNG."
    );
}

/// L'interstitiel mesuré : du HTML avec `200 OK`. Ni le type, ni la signature ne
/// doivent passer, sinon on réécrit le défaut.
#[test]
fn un_interstitiel_html_nest_pas_une_image() {
    let html = b"<!DOCTYPE html>\n<html lang=\"fr\"><body class=\"ori-age-verif\"></body></html>";
    assert_eq!(
        image::detect(html),
        None,
        "une page HTML passe pour une image : le défaut des 37 `.cbz` illisibles revient"
    );
    // Le chemin de production transforme l'échec en message, pas en fichier écrit.
    let message = image::not_an_image_message("La page 3", Some("image/jpeg"));
    assert!(
        message.contains("vérification"),
        "le message doit nommer la cause la plus fréquente (vérification d'âge / anti-robot), \
         pas un code HTTP :.message = {message}"
    );
}

fn decharger_base64(data: &str, fichier: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .unwrap_or_else(|e| panic!("{fichier} : `data` n'est pas du base64 standard ({e})"))
}

// ===========================================================================
// Porte 6 — la forme des listes : ce que l'interface lit vraiment
// ===========================================================================

/// `latest` et `search` rendent `{ list: manga[] }`. Si la clé change, l'écran
/// affiche « aucun résultat » — sans erreur, sans console. C'est un défaut
/// silencieux, donc il faut une porte.
#[test]
fn latest_et_search_rendent_une_liste_de_mangas_lisible() {
    for nom in ["04-latest", "15-search"] {
        let f = charger()
            .into_iter()
            .find(|f| f.fichier.starts_with(nom))
            .unwrap_or_else(|| panic!("fixture {nom} absente"));
        let resultat = into_result(f.enveloppe).expect("réponse valide");
        let liste = resultat
            .get("list")
            .and_then(Value::as_array)
            .unwrap_or_else(|| {
                panic!(
                    "{nom} : pas de `result.list`. L'interface lit cette clé (page.tsx) : \
                     une clé renommée vide l'écran en silence."
                )
            });
        assert!(
            !liste.is_empty(),
            "{nom} : `list` est vide. Toutes les assertions ci-dessous passeraient sur un \
             tableau vide — exactement le piège mesuré le 2026-10-04 (trois assertions \
             « A→Z = Z→A » passing sur deux listes vides)."
        );

        for (i, manga) in liste.iter().enumerate() {
            for champ in ["url", "title", "source"] {
                let v = manga.get(champ).and_then(Value::as_str).unwrap_or_default();
                assert!(
                    !v.is_empty(),
                    "{nom} : manga[{i}] sans `{champ}` exploitable. `url` vide = aucune \
                     couverture enregistrable et aucune reprise possible ; `source` vide = \
                     un manga rangé sous aucune source."
                );
            }
            // `coverUrl` peut légitimement valoir null (mesuré sur la 3ᵉ entrée
            // de `latest`) : l'interface doit savoir afficher sans vignette.
            if let Some(c) = manga.get("coverUrl") {
                assert!(
                    c.is_string() || c.is_null(),
                    "{nom} : manga[{i}].coverUrl doit être une chaîne ou null, reçu {}",
                    type_name(c)
                );
            }
            // `rating` est un double du site : -1 = « non noté », 0 = noté 0.
            // Ces deux valeurs se distinguent, donc le type doit être un nombre.
            assert!(
                manga.get("rating").is_some_and(Value::is_number),
                "{nom} : manga[{i}].rating absent ou non numérique : l'interface l'affiche \
                 tel quel"
            );
        }
    }
}

/// `details` rend `{ manga }`, pas `{ list }`. Les deux formes coexistent dans
/// le contrat, et les confondre afficherait une fiche vide.
#[test]
fn details_rend_un_manga_unique() {
    let f = charger()
        .into_iter()
        .find(|f| f.fichier.starts_with("05-details"))
        .expect("fixture details absente");
    let resultat = into_result(f.enveloppe).expect("réponse valide");
    assert!(
        resultat.get("list").is_none(),
        "details rend un manga unique : la présence de `list` ferait croire à une liste"
    );
    let manga = resultat.get("manga").unwrap_or_else(|| {
        panic!("details : pas de `result.manga` — la fiche serait vide, sans erreur")
    });
    assert_eq!(
        manga.get("url").and_then(Value::as_str),
        Some("00000000-0000-4000-8000-00000000a001"),
        "l'URL de `details` doit être celle que `latest` a rendue : c'est la clé qui relie \
         la liste à la fiche, puis aux chapitres"
    );
}

/// `chapters` : les trois formes de `chapterToJson` (titre ou `null`), et le
/// nombre flottant mesuré à l'écran (« Ch. 38.09999847… »). Le contrat dit
/// `number: number` — pas `integer` — et c'est délibéré : certains sites
/// renvoient `3.5`. Le test verrouille cette intention au lieu de la contester.
#[test]
fn chapters_ont_un_url_et_un_nombre_utilisable() {
    let f = charger()
        .into_iter()
        .find(|f| f.fichier.starts_with("06-chapters"))
        .expect("fixture chapters absente");
    let resultat = into_result(f.enveloppe).expect("réponse valide");
    let chapitres = resultat
        .get("chapters")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("chapters : pas de `result.chapters`"));
    assert!(
        chapitres.len() >= 4,
        "la fixture doit garder au moins 4 chapitres pour couvrir les formes mesurées"
    );

    for (i, c) in chapitres.iter().enumerate() {
        let url = c.get("url").and_then(Value::as_str).unwrap_or_default();
        assert!(
            !url.is_empty(),
            "chapitres[{i}] : `url` vide. C'est la clé que `pages` consomme : un chapitre sans \
             URL est un chapitre qu'on ne peut pas lire, et rien ne le signale."
        );
        let nombre = c
            .get("number")
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("chapitres[{i}] : `number` absent ou non numérique"));
        assert!(
            nombre.is_finite() && nombre >= 0.0,
            "chapitres[{i}] : `number` = {nombre} n'est pas exploitable"
        );
        // `number` est un **identifiant**, pas une donnée à afficher tel quel :
        // la source fait une division flottante, et 38.09999847… est arrivé
        // jusqu'à l'écran (« Ch. 38.099998474121094 »). Le contrat dit donc
        // `number`, pas `integer`, et ce test verrouille cette intention.
        // L'arrondi se fait à l'affichage (AGENTS.md §14), pas dans le contrat :
        // on vérifie ici que le flottant **survit**, pas qu'il disparaît.
    }

    // Le cas mesuré doit rester dans la fixture, sinon la régression qu'il
    // documente disparaîtrait en même temps que la fixture. Une assertion
    // « multiple de 0.1 » échouerait ici : c'est exactement la valeur à garder.
    let flottants: Vec<f64> = chapitres
        .iter()
        .filter_map(|c| c.get("number").and_then(Value::as_f64))
        .filter(|n| n.fract().abs() > f64::EPSILON)
        .collect();
    assert!(
        !flottants.is_empty(),
        "plus aucun chapitre à nombre flottant dans la fixture : le cas mesuré \
         (« Ch. 38.09999847… ») a disparu du test. Le remettre."
    );
    assert!(
        flottants.iter().any(|n| *n > 38.0 && *n < 38.1),
        "le flottant mesuré (38.09…) a disparu ; flottants présents : {flottants:?}"
    );
}

/// `pages` : `index` croissant depuis 0, et **surtout** une URL non vide à
/// chaque page. L'URL vide est le défaut mesuré le 2026-10-04 sur HENTAIORIGINES
/// : 45 pages annoncées, 45 URLs vides, et le téléchargeur écrivait la page
/// d'accueil 45 fois dans un `.cbz` valide.
#[test]
fn pages_ont_un_index_croissant_et_des_urls_non_vides() {
    let f = charger()
        .into_iter()
        .find(|f| f.fichier.starts_with("07-pages"))
        .expect("fixture pages absente");
    let resultat = into_result(f.enveloppe).expect("réponse valide");
    let pages = resultat
        .get("pages")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("pages : pas de `result.pages`"));
    assert!(!pages.is_empty(), "pages : liste vide");

    for (attendu, p) in pages.iter().enumerate() {
        assert_eq!(
            p.get("index").and_then(Value::as_i64),
            Some(attendu as i64),
            "pages[{attendu}] : `index` doit être la position, sinon le lecteur saute une page"
        );
        let url = p.get("url").and_then(Value::as_str).unwrap_or_default();
        assert!(
            !url.is_empty(),
            "pages[{attendu}] : `url` VIDE. C'est le défaut mesuré le 2026-10-04 : le \
             téléchargeur part chercher l'adresse \"\" et écrit la page d'accueil du site, \
             45 fois, dans un `.cbz` parfaitement valide."
        );
        // `previewUrl` vaut null sur les sources mesurées : il ne faut pas le
        // supposer présent.
        assert!(
            matches!(
                p.get("previewUrl"),
                Some(Value::String(_)) | Some(Value::Null)
            ),
            "pages[{attendu}] : `previewUrl` doit être une chaîne ou null"
        );
    }
}

// ===========================================================================
// Porte 7 — les jetons : la webview ne doit jamais voir une URL de site
// ===========================================================================

#[test]
fn les_couvertures_recues_recoivent_un_jeton_local() {
    let f = charger()
        .into_iter()
        .find(|f| f.fichier.starts_with("04-latest"))
        .expect("fixture latest absente");
    let mut resultat = into_result(f.enveloppe).expect("réponse valide");

    let cache = PageCache::new(dossier_fixtures().join("..").join("cache-de-test"));
    ajouter_jetons_couverture(&cache, "MANGADEX", &mut resultat);

    let liste = resultat
        .get("list")
        .and_then(Value::as_array)
        .expect("list");
    let avec_cover: Vec<&Value> = liste
        .iter()
        .filter(|m| m.get("coverUrl").is_some_and(|c| !c.is_null()))
        .collect();
    let sans_cover: Vec<&Value> = liste
        .iter()
        .filter(|m| m.get("coverUrl").map(Value::is_null).unwrap_or(true))
        .collect();
    assert!(
        !avec_cover.is_empty() && !sans_cover.is_empty(),
        "la fixture doit contenir les deux cas : une couverture présente (jeton attendu) et \
         `coverUrl: null` (aucun jeton). Rejouée à vide, l'assertion passerait sans rien prouver."
    );

    for manga in avec_cover {
        let jeton = manga
            .get("coverToken")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                panic!(
                    "manga `{}` : pas de `coverToken`. Sans lui l'interface fait une balise \
                 image sur l'URL du site, et la vignette est vide — en silence, sans erreur \
                 console (constat du 2026-10-04).",
                    manga.get("title").and_then(Value::as_str).unwrap_or("?")
                )
            });
        assert!(
            jeton.starts_with('c'),
            "jeton de couverture attendu (préfixe `c`), reçu `{jeton}` : un jeton de page \
             collisionnerait avec le cache image"
        );
        assert!(
            cache.resolve(jeton).is_some(),
            "jeton `{jeton}` présent dans la réponse mais absent du cache : la requête \
             `tatami://cover/<jeton>` rendrait 404"
        );
    }

    for manga in sans_cover {
        assert!(
            manga.get("coverToken").is_none(),
            "manga sans couverture : un `coverToken` a été fabriqué. Il pointerait sur rien."
        );
    }
}

/// Même vérification pour `pages` : chaque page reçoit un jeton, et le jeton est
/// un préfixe `p` — jamais le même qu'une couverture.
#[test]
fn les_pages_recues_recoivent_un_jeton_local() {
    let f = charger()
        .into_iter()
        .find(|f| f.fichier.starts_with("07-pages"))
        .expect("fixture pages absente");
    let resultat = into_result(f.enveloppe).expect("réponse valide");
    let cache = PageCache::new(dossier_fixtures().join("..").join("cache-de-test"));

    let pages = resultat
        .get("pages")
        .and_then(Value::as_array)
        .expect("pages");
    for (i, p) in pages.iter().enumerate() {
        let url = p.get("url").and_then(Value::as_str).expect("url");
        let jeton = cache.register("MANGADEX", url);
        assert!(
            jeton.starts_with('p'),
            "pages[{i}] : jeton de page attendu (préfixe `p`), reçu `{jeton}`"
        );
        // Ré-enregistrer la même page rend le même jeton : c'est ce qui évite
        // de sature le cache en relisant un chapitre.
        assert_eq!(
            cache.register("MANGADEX", url),
            jeton,
            "pages[{i}] : la même URL doit rendre le même jeton (le cache est plafonné)"
        );
    }
}
