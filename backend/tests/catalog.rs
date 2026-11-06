use serde_json::Value;
use stac_geoparquet_validator::catalog;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const ID_PREFIXES: [&str; 6] = [
    "parquet.",
    "geoparquet.",
    "bp.",
    "stac.",
    "stacbp.",
    "sample.",
];

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(name: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(manifest_dir().join(name)).unwrap()).unwrap()
}

fn is_id(literal: &str) -> bool {
    ID_PREFIXES.iter().any(|prefix| {
        literal.strip_prefix(prefix).is_some_and(|rest| {
            !rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        })
    })
}

fn ids_in_source(dir: &Path, ids: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            ids_in_source(&path, ids);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let source = fs::read_to_string(&path).unwrap();
            for (i, literal) in source.split('"').enumerate() {
                if i % 2 == 1 && is_id(literal) {
                    ids.insert(literal.to_string());
                }
            }
        }
    }
}

#[test]
fn checks_json_matches_its_schema() {
    let validator = jsonschema::validator_for(&read_json("checks.schema.json")).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&read_json("checks.json"))
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn catalog_parses() {
    assert!(!catalog::catalog().checks.is_empty());
}

#[test]
fn ids_are_unique_and_never_reused() {
    let catalog = catalog::catalog();
    let mut all: Vec<&str> = catalog.checks.iter().map(|c| c.id.as_str()).collect();
    all.extend(catalog.retired.iter().map(|r| r.id.as_str()));
    let unique: BTreeSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len());
}

#[test]
fn every_section_has_a_source() {
    let catalog = catalog::catalog();
    for check in &catalog.checks {
        if let Some(section) = check.section {
            assert!(catalog.sections.iter().any(|s| s.section == section));
        }
    }
}

#[test]
fn catalog_lists_every_id_in_the_code() {
    let mut in_source = BTreeSet::new();
    ids_in_source(&manifest_dir().join("src"), &mut in_source);
    let documented: BTreeSet<String> = catalog::catalog()
        .checks
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert_eq!(in_source, documented);
}

#[test]
fn markdown_lists_every_check_and_source() {
    let catalog = catalog::catalog();
    let markdown = catalog.to_markdown();
    for check in &catalog.checks {
        assert!(markdown.contains(&format!("`{}`", check.id)));
        for reference in &check.references {
            assert!(markdown.contains(reference.as_str()));
        }
    }
    for section in &catalog.sections {
        assert!(markdown.contains(&section.source.href));
    }
}

#[test]
fn html_lists_every_check_and_source() {
    let catalog = catalog::catalog();
    let html = catalog.to_html();
    for check in &catalog.checks {
        assert!(html.contains(&format!("<code>{}</code>", check.id)));
        for reference in &check.references {
            assert!(html.contains(&format!("href=\"{reference}\"")));
        }
    }
    for section in &catalog.sections {
        assert!(html.contains(&format!("href=\"{}\"", section.source.href)));
    }
}

#[test]
fn html_escapes_descriptions() {
    let html = catalog::catalog().to_html();
    assert!(!html.contains("<code>`"));
    assert!(html.contains("<code>geo</code>"));
}
