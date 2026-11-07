//! The account's custom colour themes: palettes a person imported or made
//! for themselves, stored per account so they follow the person to another
//! device. A theme is a name, the built-in palette it started from (a
//! `base` of "dark" or "light"), and one hex colour per theme role in the
//! app's role order. The client fills missing roles from the base and
//! warns about contrast; this layer re-validates and caps size.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;

/// Custom themes one account may keep.
pub const MAX_PER_USER: usize = 8;
/// Colours per theme: one per app role, in the app's role order.
pub const COLOR_COUNT: usize = 30;
/// Largest stored colour list, in bytes.
pub const MAX_COLORS_JSON_BYTES: usize = 8 * 1024;

/// The built-in palettes a custom theme may claim to start from.
pub const BASES: [&str; 2] = ["dark", "light"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTheme {
    pub name: String,
    /// "dark" or "light": the built-in palette the theme started from.
    pub base: String,
    /// One colour per role, in the app's role order.
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveCustomThemeInput {
    pub name: String,
    pub base: String,
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameCustomThemeInput {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteCustomThemeInput {
    pub name: String,
}

#[cfg(feature = "server")]
fn bad_name() -> String {
    "a theme name is 2-32 letters, digits, underscores, dashes or spaces".into()
}

/// A 2-32 character theme name: letters, digits, `_`, `-` or spaces.
pub fn name_ok(name: &str) -> bool {
    (2..33).contains(&name.chars().count())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b' ')
}

/// A `#rrggbb` hex colour.
pub fn color_ok(value: &str) -> bool {
    value.len() == 7
        && value.starts_with("#")
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Everything a request's palette must satisfy; the message is user-facing.
pub fn palette_ok(colors: &[String]) -> Result<(), String> {
    let count = colors.len();
    if count != COLOR_COUNT {
        return Err(format!("a theme needs {COLOR_COUNT} colours, got {count}"))
    }
    if colors.into_iter().all(|c| color_ok(c)) {
        Ok(())
    } else {
        Err("every colour must be a #rrggbb hex".into())
    }
}

#[cfg(feature = "server")]
pub fn list(db: &Db, caller: i64) -> Vec<CustomTheme> {
    let Ok(c) = db.lock() else { return Vec::new() };
    let Ok(mut stmt) = c.prepare(
        "SELECT name, base, colors_json FROM custom_themes WHERE user_id = ?1 ORDER BY name",
    ) else {
        return Vec::new();
    };
    let themes = match stmt.query_map(rusqlite::params![caller], |r| {
        Ok(CustomTheme {
            name: r.get::<_, String>(0)?,
            base: r.get::<_, String>(1)?,
            colors: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or(Vec::new()),
        })
    }) {
        Ok(rows) => rows.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    };
    themes
}

#[cfg(feature = "server")]
fn held(db: &Db, caller: i64) -> usize {
    list(db, caller).into_iter().fold(0, |n, _| n + 1)
}

#[cfg(feature = "server")]
fn themed(db: &Db, caller: i64, name: &str) -> bool {
    list(db, caller).into_iter().any(|t| t.name == name)
}

/// Validate and store (or replace) one theme.
#[cfg(feature = "server")]
pub fn save(db: &Db, caller: i64, input: SaveCustomThemeInput) -> Result<CustomTheme, String> {
    if !name_ok(&input.name) {
        return Err(bad_name())
    }
    if !BASES.into_iter().any(|b| b == &input.base) {
        return Err("a custom theme must say which built-in palette it started from: dark or light".into())
    }
    let colors = input.colors;
    palette_ok(&colors)?;
    let json = serde_json::to_string(&colors).map_err(|e| e.to_string())?;
    if json.len() > MAX_COLORS_JSON_BYTES {
        return Err("the palette is too large".into())
    }
    if !themed(db, caller, &input.name) && held(db, caller) >= MAX_PER_USER {
        return Err("you already have too many custom themes".into())
    }
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute(
        "INSERT OR REPLACE INTO custom_themes (user_id, name, base, colors_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![caller, input.name, input.base, json, crate::realtime::now_millis()],
    )
    .map_err(|e| format!("failed to save the theme: {e}"))?;
    Ok(CustomTheme { name: input.name, base: input.base, colors: colors })
}

/// Give one of the caller's themes a new name, unless another already has it.
#[cfg(feature = "server")]
pub fn rename(db: &Db, caller: i64, input: RenameCustomThemeInput) -> Result<CustomTheme, String> {
    if !name_ok(&input.to) {
        return Err(bad_name())
    }
    let existing = match list(db, caller).into_iter().find(|t| t.name == input.from) {
        None => return Err("no such theme".into()),
        Some(t) => t,
    };
    if themed(db, caller, &input.to) {
        return Err("a theme with that name already exists".into())
    }
    let renamed = CustomTheme { name: input.to, base: existing.base, colors: existing.colors };
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute(
        "UPDATE custom_themes SET name = ?3 WHERE user_id = ?1 AND name = ?2",
        rusqlite::params![caller, input.from, renamed.name],
    )
    .map_err(|e| format!("failed to rename the theme: {e}"))?;
    Ok(renamed)
}

/// Drop one of the caller's themes.
#[cfg(feature = "server")]
pub fn remove(db: &Db, caller: i64, name: &str) -> Result<(), String> {
    if !themed(db, caller, name) {
        return Err("no such theme".into())
    }
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute(
        "DELETE FROM custom_themes WHERE user_id = ?1 AND name = ?2",
        rusqlite::params![caller, name],
    )
    .map_err(|e| format!("failed to delete the theme: {e}"))?;
    Ok(())
}

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))
}

#[cfg(feature = "server")]
fn caller() -> Result<i64, ServerError> {
    crate::session::acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))
}

/// The caller's custom themes.
#[server]
pub async fn list_custom_themes() -> Result<Vec<CustomTheme>, ServerError> {
    Ok(list(&db_from_state()?, caller()?))
}

/// Save (or replace) one custom theme.
#[server]
pub async fn save_custom_theme(input: SaveCustomThemeInput) -> Result<CustomTheme, ServerError> {
    save(&db_from_state()?, caller()?, input).map_err(ServerError::failed)
}

/// Rename one custom theme.
#[server]
pub async fn rename_custom_theme(input: RenameCustomThemeInput) -> Result<CustomTheme, ServerError> {
    rename(&db_from_state()?, caller()?, input).map_err(ServerError::failed)
}

/// Delete one custom theme.
#[server]
pub async fn delete_custom_theme(input: DeleteCustomThemeInput) -> Result<(), ServerError> {
    remove(&db_from_state()?, caller()?, &input.name).map_err(ServerError::failed)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;

    const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");

    /// alice (1) and bob (2): two accounts with their own themes.
    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::migrate(&dir.path().join("t.db"), Path::new(MIGRATIONS)).unwrap();
        db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1),(2,'bob','b@x',1);",
            )
            .unwrap();
        (dir, db)
    }

    /// A full palette of valid hex colours, all different per position.
    fn palette() -> Vec<String> {
        Vec::from_iter((1..COLOR_COUNT + 1).map(|i| format!("#{:06x}", i)))
    }

    fn count(v: Vec<CustomTheme>) -> usize {
        v.into_iter().fold(0, |n, _| n + 1)
    }

    fn theme(name: &str, base: &str, colors: Vec<String>) -> SaveCustomThemeInput {
        SaveCustomThemeInput { name: name.into(), base: base.into(), colors: colors }
    }

    #[test]
    fn save_validates_and_round_trips() {
        let (_dir, db) = setup();
        let saved = save(&db, 1, theme("My Theme", "dark", palette())).unwrap();
        assert_eq!(saved.colors.len(), COLOR_COUNT);
        let rows = list(&db, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "My Theme");
        assert_eq!(rows[0].base, "dark");
        assert_eq!(list(&db, 2).len(), 0);
        save(&db, 1, theme("My Theme", "light", palette())).unwrap();
        assert_eq!(list(&db, 1).len(), 1);
    }

    #[test]
    fn bad_inputs_are_refused() {
        let (_dir, db) = setup();
        assert!(save(&db, 1, theme("x", "dark", palette())).is_err());
        assert!(save(&db, 1, theme("bad*name", "dark", palette())).is_err());
        assert!(save(&db, 1, theme("ok", "neon", palette())).is_err());
        let short = palette().into_iter().take(COLOR_COUNT - 1).collect();
        assert!(save(&db, 1, theme("ok", "dark", short)).is_err());
        let bad = palette().into_iter().map(|c| if c == "#000001" { "red".into() } else { c }).collect();
        assert!(save(&db, 1, theme("ok", "dark", bad)).is_err());
        for i in 0..MAX_PER_USER {
            save(&db, 1, theme(&format!("theme {i}"), "dark", palette())).unwrap();
        }
        assert!(save(&db, 1, theme("one more", "dark", palette())).is_err());
        save(&db, 1, theme("theme 0", "light", palette())).unwrap();
        assert_eq!(count(list(&db, 1)), MAX_PER_USER);
    }

    #[test]
    fn rename_and_delete() {
        let (_dir, db) = setup();
        save(&db, 1, theme("First", "dark", palette())).unwrap();
        save(&db, 1, theme("Second", "light", palette())).unwrap();
        let moved = rename(&db, 1, RenameCustomThemeInput { from: "First".into(), to: "Renamed".into() }).unwrap();
        assert_eq!(moved.base, "dark");
        assert!(rename(&db, 1, RenameCustomThemeInput { from: "Renamed".into(), to: "Second".into() }).is_err());
        assert!(rename(&db, 1, RenameCustomThemeInput { from: "Gone".into(), to: "Else".into() }).is_err());
        assert!(rename(&db, 1, RenameCustomThemeInput { from: "Second".into(), to: "bad*name".into() }).is_err());
        remove(&db, 1, "Renamed").unwrap();
        assert_eq!(count(list(&db, 1)), 1);
        assert!(remove(&db, 1, "Renamed").is_err());
        db.lock().unwrap().execute("DELETE FROM users WHERE id = ?1", rusqlite::params![1]).unwrap();
        assert_eq!(count(list(&db, 1)), 0);
    }

    #[test]
    fn colour_shape_is_checked() {
        assert!(color_ok("#aBcDeF"));
        assert!(!color_ok("#abc"));
        assert!(!color_ok("aabbcc"));
        assert!(!color_ok("#gggggg"));
        assert!(!color_ok(""));
    }
}
