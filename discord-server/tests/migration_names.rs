//! Migration files run once each in filename order and are recorded by name,
//! so a shipped file can never be renamed. These checks keep new ones tidy:
//! a four-digit prefix, a name, `.sql`, and a prefix no other file uses.

use std::collections::BTreeMap;

/// Prefixes that were shipped twice before this check existed. They stay
/// as they are (renaming would re-run them); no new pairs may join them.
const SHIPPED_DUPLICATES: [&str; 3] = ["0002", "0013", "0014"];

fn migration_files() -> Vec<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read migrations dir")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn migration_names_are_well_formed() {
    for name in migration_files() {
        let (prefix, rest) = name.split_at(4.min(name.len()));
        assert!(
            prefix.len() == 4 && prefix.chars().all(|c| c.is_ascii_digit()),
            "{name}: migrations start with a four-digit number"
        );
        assert!(
            rest.starts_with("_") && rest.len() > ".sql".len() + 1 && rest.ends_with(".sql"),
            "{name}: migrations are named NNNN_what.sql"
        );
    }
}

#[test]
fn migration_numbers_are_unique() {
    let mut by_prefix: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in migration_files() {
        by_prefix.entry(name[..4].to_string()).or_default().push(name);
    }
    for (prefix, names) in by_prefix {
        let allowed = if SHIPPED_DUPLICATES.contains(&prefix.as_str()) { 2 } else { 1 };
        assert!(
            names.len() <= allowed,
            "migration number {prefix} is used by {names:?}; give the new file the next unused number"
        );
    }
}
