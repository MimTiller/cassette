//! A server's custom emoji: small images with a name, uploaded by members
//! with Manage Expressions and usable by every member. Messages and
//! reactions carry them as `<:name:id>` (see the client's `custom_emoji`),
//! so renaming one updates everything that uses it.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::permissions::{self, Permissions};

/// At most this many emoji per server.
pub const MAX_PER_SERVER: usize = 50;
/// Largest image, in bytes.
pub const MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEmoji {
    pub id: i64,
    pub server_id: i64,
    pub name: String,
    /// A `/media/…` path.
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEmojiInput {
    pub server_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadEmojiInput {
    pub server_id: i64,
    pub name: String,
    /// `data:image/png|gif|webp;base64,…`
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameEmojiInput {
    pub emoji_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteEmojiInput {
    pub emoji_id: i64,
}

/// 2–32 characters of letters, digits and `_`.
pub fn name_ok(name: &str) -> bool {
    (2..=32).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(feature = "server")]
fn require(db: &Db, caller: i64, server_id: i64) -> Result<(), String> {
    let (_, member) = permissions::member_in(db, server_id, caller)?;
    if !member.server.contains(Permissions::MANAGE_EXPRESSIONS) {
        return Err(permissions::missing(Permissions::MANAGE_EXPRESSIONS));
    }
    Ok(())
}

#[cfg(feature = "server")]
fn bad_name() -> String {
    "emoji names are 2–32 letters, digits or underscores".into()
}

#[cfg(feature = "server")]
fn friendly(e: rusqlite::Error) -> String {
    if e.to_string().contains("UNIQUE") {
        "this server already has an emoji with that name".into()
    } else {
        format!("failed to save the emoji: {e}")
    }
}

#[cfg(feature = "server")]
pub fn list(db: &Db, server_id: i64) -> Vec<ServerEmoji> {
    let Ok(c) = db.lock() else { return Vec::new() };
    let Ok(mut stmt) = c.prepare(
        "SELECT id, server_id, name, image_path FROM server_emoji WHERE server_id = ?1 ORDER BY name COLLATE NOCASE",
    ) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![server_id], |r| {
        Ok(ServerEmoji { id: r.get(0)?, server_id: r.get(1)?, name: r.get(2)?, image: r.get(3)? })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

#[cfg(feature = "server")]
fn one(db: &Db, emoji_id: i64) -> Option<ServerEmoji> {
    let c = db.lock().ok()?;
    c.query_row(
        "SELECT id, server_id, name, image_path FROM server_emoji WHERE id = ?1",
        rusqlite::params![emoji_id],
        |r| Ok(ServerEmoji { id: r.get(0)?, server_id: r.get(1)?, name: r.get(2)?, image: r.get(3)? }),
    )
    .ok()
}

/// Validate and store an upload. `store` puts the bytes in the media store
/// (a parameter so tests needn't touch the real one).
#[cfg(feature = "server")]
pub fn upload(
    db: &Db,
    caller: i64,
    input: UploadEmojiInput,
    now_ms: i64,
    store: impl Fn(&[u8], &str) -> Result<String, String>,
) -> Result<ServerEmoji, String> {
    require(db, caller, input.server_id)?;
    if !name_ok(&input.name) {
        return Err(bad_name());
    }
    if list(db, input.server_id).len() >= MAX_PER_SERVER {
        return Err(format!("a server can have at most {MAX_PER_SERVER} emoji"));
    }
    let (mime, bytes) = crate::media::decode_data_image_url(&input.data_url).ok_or("not an image")?;
    if !matches!(mime.as_str(), "image/png" | "image/gif" | "image/webp") {
        return Err("emoji must be PNG, GIF or WebP".into());
    }
    if bytes.len() > MAX_BYTES {
        return Err("emoji images can be at most 256 KB".into());
    }
    let image = store(&bytes, &mime)?;
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute(
        "INSERT INTO server_emoji (server_id, name, image_path, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![input.server_id, input.name, image, caller, now_ms],
    )
    .map_err(friendly)?;
    Ok(ServerEmoji { id: c.last_insert_rowid(), server_id: input.server_id, name: input.name, image })
}

#[cfg(feature = "server")]
pub fn rename(db: &Db, caller: i64, emoji_id: i64, name: &str) -> Result<ServerEmoji, String> {
    let e = one(db, emoji_id).ok_or("emoji not found")?;
    require(db, caller, e.server_id)?;
    if !name_ok(name) {
        return Err(bad_name());
    }
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute("UPDATE server_emoji SET name = ?2 WHERE id = ?1", rusqlite::params![emoji_id, name])
        .map_err(friendly)?;
    Ok(ServerEmoji { name: name.to_string(), ..e })
}

/// Delete an emoji; returns its server id.
#[cfg(feature = "server")]
pub fn delete(db: &Db, caller: i64, emoji_id: i64) -> Result<i64, String> {
    let e = one(db, emoji_id).ok_or("emoji not found")?;
    require(db, caller, e.server_id)?;
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute("DELETE FROM server_emoji WHERE id = ?1", rusqlite::params![emoji_id])
        .map_err(|e| format!("failed to delete the emoji: {e}"))?;
    Ok(e.server_id)
}

/// Tell the server's members to refetch its emoji.
#[cfg(feature = "server")]
fn announce(db: &Db, server_id: i64) {
    crate::realtime::hub().publish(
        &crate::realtime::server_audience(db, server_id),
        &crate::server_settings::WsServerMsg::EmojiChanged { server_id },
    );
}

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))
}

#[cfg(feature = "server")]
fn caller() -> Result<i64, ServerError> {
    crate::session::acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))
}

/// A server's emoji. Any member may read them.
#[server]
pub async fn server_emoji(input: ServerEmojiInput) -> Result<Vec<ServerEmoji>, ServerError> {
    let db = db_from_state()?;
    permissions::member_in(&db, input.server_id, caller()?).map_err(ServerError::failed)?;
    Ok(list(&db, input.server_id))
}

/// Upload an emoji. Needs Manage Expressions.
#[server]
pub async fn upload_emoji(input: UploadEmojiInput) -> Result<ServerEmoji, ServerError> {
    let db = db_from_state()?;
    let now = crate::realtime::now_millis();
    let e = upload(&db, caller()?, input, now, |bytes, mime| crate::media::store(bytes, mime, MAX_BYTES))
        .map_err(ServerError::failed)?;
    announce(&db, e.server_id);
    Ok(e)
}

/// Rename an emoji. Needs Manage Expressions.
#[server]
pub async fn rename_emoji(input: RenameEmojiInput) -> Result<ServerEmoji, ServerError> {
    let db = db_from_state()?;
    let e = rename(&db, caller()?, input.emoji_id, &input.name).map_err(ServerError::failed)?;
    announce(&db, e.server_id);
    Ok(e)
}

/// Delete an emoji. Needs Manage Expressions.
#[server]
pub async fn delete_emoji(input: DeleteEmojiInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let server_id = delete(&db, caller()?, input.emoji_id).map_err(ServerError::failed)?;
    announce(&db, server_id);
    Ok(())
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use std::path::Path;

    const MIGRATIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
    const PNG: &str = "data:image/png;base64,iVBORw0KGgo=";

    /// alice (1) owns server 1; bob (2) is a member; carol (3) holds a role
    /// with Manage Expressions.
    fn setup() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::migrate(&dir.path().join("t.db"), Path::new(MIGRATIONS)).unwrap();
        db.lock()
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO users (id, username, email, created_at) VALUES (1,'alice','a@x',1),(2,'bob','b@x',1),(3,'carol','c@x',1);
                 INSERT INTO servers (id, name, owner_id, created_at) VALUES (1,'one',1,1);
                 INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (1,1,'owner',1),(1,2,'member',1),(1,3,'member',1);
                 INSERT INTO server_roles (id, server_id, name, position, permissions, created_at) VALUES (7,1,'Emoji',1,{},1);
                 INSERT INTO member_roles (user_id, role_id) VALUES (3,7);",
                Permissions::MANAGE_EXPRESSIONS.0
            ))
            .unwrap();
        (dir, db)
    }

    fn up(db: &Db, caller: i64, name: &str, data_url: &str) -> Result<ServerEmoji, String> {
        let input = UploadEmojiInput { server_id: 1, name: name.into(), data_url: data_url.into() };
        upload(db, caller, input, 1, |_, mime| Ok(format!("/media/x.{}", mime.trim_start_matches("image/"))))
    }

    #[test]
    fn names_are_checked() {
        assert!(name_ok("ok"));
        assert!(name_ok("party_parrot_2"));
        assert!(!name_ok("a"));
        assert!(!name_ok(&"x".repeat(33)));
        assert!(!name_ok("no spaces"));
        assert!(!name_ok("dash-es"));
        assert!(!name_ok("émoji"));
    }

    #[test]
    fn manage_expressions_is_needed() {
        let (_dir, db) = setup();
        assert!(up(&db, 2, "nope", PNG).is_err(), "a plain member can't upload");
        let e = up(&db, 3, "yes", PNG).unwrap();
        assert!(up(&db, 1, "owner_can", PNG).is_ok());
        assert!(rename(&db, 2, e.id, "renamed").is_err());
        assert!(delete(&db, 2, e.id).is_err());
        assert_eq!(rename(&db, 3, e.id, "renamed").unwrap().name, "renamed");
        assert_eq!(delete(&db, 3, e.id).unwrap(), 1);
        assert_eq!(list(&db, 1).len(), 1);
    }

    #[test]
    fn uploads_are_validated() {
        let (_dir, db) = setup();
        assert!(up(&db, 1, "jpeg", "data:image/jpeg;base64,/9j/4AA=").is_err(), "no JPEG");
        assert!(up(&db, 1, "junk", "not a data url").is_err());
        assert!(up(&db, 1, "x", PNG).is_err(), "name too short");
        up(&db, 1, "Same", PNG).unwrap();
        assert!(up(&db, 1, "same", PNG).unwrap_err().contains("already"), "names are unique ignoring case");
        let big = format!("data:image/png;base64,{}", "A".repeat(MAX_BYTES * 4 / 3 + 8));
        assert!(up(&db, 1, "big", &big).is_err(), "over 256 KB");
        for i in 0..MAX_PER_SERVER - 1 {
            up(&db, 1, &format!("e{i}"), PNG).unwrap();
        }
        assert!(up(&db, 1, "one_too_many", PNG).unwrap_err().contains("at most"));
    }
}
