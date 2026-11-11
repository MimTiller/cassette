//! A server's stickers: big images (a whole message, not an inline token)
//! uploaded by members with Manage Expressions and usable by every member.
//! A message carries a sticker by its id; the server copies the sticker's
//! name and image path onto the message row at SEND time (see the
//! `sticker_*` columns in `0033_server_stickers.sql`), so the message still
//! renders after the sticker itself is deleted.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::permissions::{self, Permissions};

/// At most this many stickers per server.
pub const MAX_PER_SERVER: usize = 30;
/// Largest image, in bytes (stickers are animated, so twice an emoji's cap).
pub const MAX_BYTES: usize = 512 * 1024;
/// Longest description, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sticker {
    pub id: i64,
    pub server_id: i64,
    pub name: String,
    pub description: String,
    /// One unicode emoji or a `:custom:` name, used for search.
    pub related_emoji: String,
    /// A `/media/…` path.
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickerListInput {
    pub server_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStickerInput {
    pub server_id: i64,
    pub name: String,
    pub description: String,
    pub related_emoji: String,
    /// `data:image/png|gif|webp;base64,…`
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditStickerInput {
    pub sticker_id: i64,
    pub name: String,
    pub description: String,
    pub related_emoji: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteStickerInput {
    pub sticker_id: i64,
}

/// 2–30 characters, no `<`, `>` or `:` (those are the message token
/// delimiters), not blank-padded.
pub fn name_ok(name: &str) -> bool {
    let chars = name.chars().count();
    (2..=30).contains(&chars)
        && name == name.trim()
        && name.chars().all(|c| !c.is_control() && !"<>:".contains(c))
}

/// The search chip: one unicode emoji (a non-ASCII codepoint, or an ASCII
/// one paired with a variation selector), or a `:custom:` name.
pub fn related_ok(related: &str) -> bool {
    if related.len() >= 2 && related.starts_with(':') && related.ends_with(':') {
        return name_ok(&related[1..related.len() - 1]);
    }
    related.chars().count() == 1 && related >= "\u{80}"
}

/// Sticker descriptions are free text up to 100 characters.
pub fn description_ok(description: &str) -> bool {
    description.chars().count() <= MAX_DESCRIPTION_CHARS
        && description == description.trim()
        && description.chars().all(|c| !c.is_control())
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
    "sticker names are 2–30 characters (no colons or angle brackets)".into()
}

#[cfg(feature = "server")]
fn bad_related() -> String {
    "relate the sticker to one emoji or a :custom: name".into()
}

#[cfg(feature = "server")]
fn bad_description() -> String {
    "descriptions are up to 100 characters".into()
}

#[cfg(feature = "server")]
fn friendly(e: rusqlite::Error) -> String {
    if e.to_string().contains("UNIQUE") {
        "this server already has a sticker with that name".into()
    } else {
        format!("failed to save the sticker: {e}")
    }
}

#[cfg(feature = "server")]
pub fn list(db: &Db, server_id: i64) -> Vec<Sticker> {
    let Ok(c) = db.lock() else { return Vec::new() };
    let Ok(mut stmt) = c.prepare(
        "SELECT id, server_id, name, description, related_emoji, image_path FROM stickers WHERE server_id = ?1 ORDER BY name COLLATE NOCASE",
    ) else {
        return Vec::new();
    };
    stmt.query_map(rusqlite::params![server_id], |r| {
        Ok(Sticker { id: r.get(0)?, server_id: r.get(1)?, name: r.get(2)?, description: r.get(3)?, related_emoji: r.get(4)?, image: r.get(5)? })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

#[cfg(feature = "server")]
fn one(db: &Db, sticker_id: i64) -> Option<Sticker> {
    let c = db.lock().ok()?;
    c.query_row(
        "SELECT id, server_id, name, description, related_emoji, image_path FROM stickers WHERE id = ?1",
        rusqlite::params![sticker_id],
        |r| Ok(Sticker { id: r.get(0)?, server_id: r.get(1)?, name: r.get(2)?, description: r.get(3)?, related_emoji: r.get(4)?, image: r.get(5)? }),
    )
    .ok()
}

/// The rules for a sticker send: the sender must be a member of the
/// sticker's server, and a channel send must be in that same server.
#[cfg(feature = "server")]
pub fn can_send(db: &Db, user_id: i64, sticker_id: i64, channel_server_id: i64) -> Result<Sticker, String> {
    let s = one(db, sticker_id).ok_or("sticker not found")?;
    if channel_server_id != 0 && s.server_id != channel_server_id {
        return Err(format!("{} can only be sent in its own server", s.name));
    }
    permissions::member_in(db, s.server_id, user_id)?;
    Ok(s)
}

/// Validate and store an upload. `store` puts the bytes in the media store
/// (a parameter so tests needn't touch the real one).
#[cfg(feature = "server")]
pub fn upload(
    db: &Db,
    caller: i64,
    input: UploadStickerInput,
    now_ms: i64,
    store: impl Fn(&[u8], &str) -> Result<String, String>,
) -> Result<Sticker, String> {
    require(db, caller, input.server_id)?;
    if !name_ok(&input.name) {
        return Err(bad_name());
    }
    if !description_ok(&input.description) {
        return Err(bad_description());
    }
    if !related_ok(&input.related_emoji) {
        return Err(bad_related());
    }
    if list(db, input.server_id).len() >= MAX_PER_SERVER {
        return Err(format!("a server can have at most {MAX_PER_SERVER} stickers"));
    }
    let (mime, bytes) = crate::media::decode_data_image_url(&input.data_url).ok_or("not an image")?;
    if !matches!(mime.as_str(), "image/png" | "image/gif" | "image/webp") {
        return Err("stickers must be PNG, GIF or WebP".into());
    }
    if bytes.len() > MAX_BYTES {
        return Err("sticker images can be at most 512 KB".into()) ;
    }
    let image = store(&bytes, &mime)?;
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute(
        "INSERT INTO stickers (server_id, name, description, related_emoji, image_path, created_by, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![input.server_id, input.name, input.description, input.related_emoji, image, caller, now_ms],
    )
    .map_err(friendly)?;
    Ok(Sticker { id: c.last_insert_rowid(), server_id: input.server_id, name: input.name, description: input.description, related_emoji: input.related_emoji, image })
}

/// Edit one sticker's name, description and related emoji; the image stays.
#[cfg(feature = "server")]
pub fn edit(db: &Db, caller: i64, input: EditStickerInput) -> Result<Sticker, String> {
    let s = one(db, input.sticker_id).ok_or("sticker not found")?;
    require(db, caller, s.server_id)?;
    if !name_ok(&input.name) {
        return Err(bad_name());
    }
    if !description_ok(&input.description) {
        return Err(bad_description());
    }
    if !related_ok(&input.related_emoji) {
        return Err(bad_related());
    }
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute(
        "UPDATE stickers SET name = ?2, description = ?3, related_emoji = ?4 WHERE id = ?1",
        rusqlite::params![input.sticker_id, input.name, input.description, input.related_emoji],
    )
    .map_err(friendly)?;
    Ok(Sticker { name: input.name, description: input.description, related_emoji: input.related_emoji, ..s })
}

/// Delete a sticker; returns its server id.
#[cfg(feature = "server")]
pub fn delete(db: &Db, caller: i64, sticker_id: i64) -> Result<i64, String> {
    let s = one(db, sticker_id).ok_or("sticker not found")?;
    require(db, caller, s.server_id)?;
    let c = db.lock().map_err(|e| e.to_string())?;
    c.execute("DELETE FROM stickers WHERE id = ?1", rusqlite::params![sticker_id])
        .map_err(|e| format!("failed to delete the sticker: {e}"))?;
    Ok(s.server_id)
}

/// Tell the server's members to refetch its stickers.
#[cfg(feature = "server")]
fn announce(db: &Db, server_id: i64) {
    crate::realtime::hub().publish(
        &crate::realtime::server_audience(db, server_id),
        &crate::server_settings::WsServerMsg::StickersChanged { server_id, seq: crate::server_settings::next_frame_seq() },
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

/// A server's stickers. Any member may read them.
#[server]
pub async fn list_stickers(input: StickerListInput) -> Result<Vec<Sticker>, ServerError> {
    let db = db_from_state()?;
    permissions::member_in(&db, input.server_id, caller()?).map_err(ServerError::failed)?;
    Ok(list(&db, input.server_id))
}

/// Upload a sticker. Needs Manage Expressions.
#[server]
pub async fn upload_sticker(input: UploadStickerInput) -> Result<Sticker, ServerError> {
    let db = db_from_state()?;
    let now = crate::realtime::now_millis();
    let s = upload(&db, caller()?, input, now, |bytes, mime| crate::media::store(bytes, mime, MAX_BYTES))
        .map_err(ServerError::failed)?;
    announce(&db, s.server_id);
    Ok(s)
}

/// Edit a sticker's name, description and related emoji. Needs Manage Expressions.
#[server]
pub async fn edit_sticker(input: EditStickerInput) -> Result<Sticker, ServerError> {
    let db = db_from_state()?;
    let s = edit(&db, caller()?, input).map_err(ServerError::failed)?;
    announce(&db, s.server_id);
    Ok(s)
}

/// Delete a sticker. Needs Manage Expressions.
#[server]
pub async fn delete_sticker(input: DeleteStickerInput) -> Result<(), ServerError> {
    let db = db_from_state()?;
    let server_id = delete(&db, caller()?, input.sticker_id).map_err(ServerError::failed)?;
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

    fn up(db: &Db, caller: i64, name: &str) -> Result<Sticker, String> {
        upload(db, caller, UploadStickerInput { server_id: 1, name: name.into(), description: "a big cat".into(), related_emoji: "😭".into(), data_url: PNG.into() }, 1, |_, _| Ok("/media/x.png".to_string()))
    }

    #[test]
    fn names_and_chips_are_checked() {
        assert!(name_ok("big cat"));
        assert!(name_ok("okay"));
        assert!(!name_ok("a"));
        assert!(!name_ok(&"x".repeat(31)));
        assert!(!name_ok("spaced  "));
        assert!(!name_ok("<tag>"));
        assert!(related_ok("😭"));
        assert!(related_ok(":party_parrot:"));
        assert!(!related_ok("not an emoji chip"));
        assert!(!related_ok("::"));
        assert!(description_ok(""));
        assert!(description_ok("up to a hundred characters here"));
        assert!(!description_ok(&"x".repeat(101)));
    }

    #[test]
    fn manage_expressions_is_needed() {
        let (_dir, db) = setup();
        assert!(up(&db, 2, "nope").is_err(), "a plain member can't upload");
        let s = up(&db, 3, "yes").unwrap();
        assert!(edit(&db, 2, EditStickerInput { sticker_id: s.id, name: "renamed".into(), description: String::new(), related_emoji: "😭".into() }).is_err());
        assert!(delete(&db, 2, s.id).is_err());
        assert_eq!(edit(&db, 3, EditStickerInput { sticker_id: s.id, name: "renamed".into(), description: "new".into(), related_emoji: ":wave:".into() }).unwrap().name, "renamed");
        assert_eq!(delete(&db, 3, s.id).unwrap(), 1);
        assert_eq!(list(&db, 1).len(), 0);
    }

    #[test]
    fn uploads_are_validated() {
        let (_dir, db) = setup();
        assert!(up2(&db, "jpeg", "data:image/jpeg;base64,/9j/4AA=").is_err(), "no JPEG");
        assert!(up2(&db, "junk", "not a data url").is_err());
        assert!(up(&db, 1, "x").is_err(), "name too short");
        up(&db, 1, "Same").unwrap();
        assert!(up(&db, 1, "same").unwrap_err().contains("already"), "names are unique ignoring case");
        let big = format!("data:image/png;base64,{}", "A".repeat(MAX_BYTES * 4 / 3 + 8));
        assert!(up2(&db, "big", &big).is_err(), "over 512 KB");
        for i in 0..MAX_PER_SERVER - 1 {
            up(&db, 1, &format!("st{i}")).unwrap();
        }
        assert!(up(&db, 1, "one_too_many").unwrap_err().contains("at most"));
    }

    fn up2(db: &Db, name: &str, data_url: &str) -> Result<Sticker, String> {
        upload(db, 1, UploadStickerInput { server_id: 1, name: name.into(), description: String::new(), related_emoji: "😭".into(), data_url: data_url.into() }, 1, |_, _| Ok("/media/x.png".to_string()))
    }

    #[test]
    fn a_sticker_cannot_be_sent_in_another_server() {
        let (_dir, db) = setup();
        db.lock().unwrap().execute("INSERT INTO servers (id, name, owner_id, created_at) VALUES (2,'two',1,1)", ()).unwrap();
        db.lock().unwrap().execute("INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES (2,2,'member',1)", ()).unwrap();
        let s = up(&db, 1, "mine").unwrap();
        assert!(can_send(&db, 2, s.id, 2).is_err(), "server 2 members can't send a server-1 sticker");
        assert!(can_send(&db, 2, s.id, 0).is_ok(), "but a DM send needs only membership");
        assert!(can_send(&db, 2, s.id, 1).is_ok());
    }

    #[test]
    fn a_non_member_cannot_send_or_edit_a_sticker_by_id() {
        let (_dir, db) = setup();
        db.lock().unwrap().execute(
            "INSERT INTO users (id, username, email, created_at) VALUES (9,'ghost','g@x',1)".into(),
            (),
        ).unwrap();
        let s = up(&db, 1, "mine").unwrap();
        // Even a DM send (channel 0) needs membership in the sticker's server.
        assert!(can_send(&db, 9, s.id, 0).is_err(), "a non-member can't send it");
        assert!(can_send(&db, 9, s.id, 1).is_err(), "nor in its own server");
        // `one` finds the sticker by id (the endpoint layer must gate it), but
        // edit/delete go through `require`, which rejects the non-member.
        assert_eq!(edit(&db, 9, EditStickerInput { sticker_id: s.id, name: "hijack".into(), description: String::new(), related_emoji: "😭".into() }).unwrap_err(),
            "you're not a member of this server");
        assert_eq!(delete(&db, 9, s.id).unwrap_err(), "you're not a member of this server");
    }
}
