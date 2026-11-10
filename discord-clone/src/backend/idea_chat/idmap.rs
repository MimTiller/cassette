//! The Uuid <-> i64 table: the bridge between the idea-chat wire's UUIDv7
//! ids (`chat_model::Id`) and the i64s the UI and its persisted state speak.
//!
//! RWND's whole client — its screens, its `ShellState`, its persisted spots
//! like `last_viewed_message` — is keyed by i64s; idea-chat's wire is UUIDv7.
//! The wire study's id plan settles the mapping: first seen, first numbered —
//! the first id to appear on the wire gets 1, the next new one 2, and so on —
//! so a stable order comes from the snapshot's own ordering and never changes
//! once assigned.
//!
//! Ids assigned in one browser session must mean the same thing after a
//! reload — so the table persists in browser storage (the same `platform_storage`
//! store `crate::auth` uses) under a key named per signed-in account: one
//! table per account. The persisted form is the UUIDs in assigned order
//! (position + `FIRST_ID` is the i64), so a reload re-seeds exactly.
//!
//! The account is an explicit argument on every verb (not a "current account"
//! global): one table per signed-in account, reached with the handle the
//! session was signed in under.

use serde::{Deserialize, Serialize};
use serde_json::from_str as json_from_str;
use serde_json::to_string as json_to_string;

use std::collections::HashMap;

// The i64 ids the table hands out start here: 0 stays reserved (the client
// uses `user_id == 0` as "signed out", and `active_server == 0` etc. as
// "none selected").
pub const FIRST_ID: i64 = 1;

/// The storage key prefix: one table per signed-in account (its handle), so
/// two accounts' id tables never collide — an i64 means one UUID per account.
const KEY_PREFIX: &str = "idea_chat.idmap.";

/// The persisted form: the UUIDs in the order they were assigned. Position
/// plus `FIRST_ID` is the i64 a UUID first got.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persisted {
    pub uuids: Vec<String>,
}

/// The store this table persists through: the same `platform_storage`
/// namespace `crate::auth` keeps the session in, so one store serves both.
fn store() -> std::sync::Arc<dyn storage::Storage> {
    crate::auth::store()
}

fn key(account: String) -> String {
    KEY_PREFIX + account
}

/// One account's table: the assigned ids in order, and both directions.
#[derive(Clone)]
struct Table {
    order: Vec<String>,
    uuid_to_i: HashMap<String, i64>,
    i_to_uuid: HashMap<i64, String>,
}

impl Table {
    /// A table with no ids assigned.
    fn empty() -> Table {
        Table {
            order: Vec::new(),
            uuid_to_i: HashMap::new(),
            i_to_uuid: HashMap::new(),
        }
    }

    /// Assign `uuid` the next number, or answer the number it already holds.
    fn number(mut self: Table, uuid: String) -> i64 {
        if let Some(id) = self.uuid_to_i.get(&uuid) {
            return id;
        }
        let id = FIRST_ID + self.order.len() as i64;
        self.order.push(uuid);
        self.uuid_to_i.insert(uuid, id);
        self.i_to_uuid.insert(id, uuid);
        id
    }

    /// The persisted view of this table.
    fn persisted(mut self: Table) -> Persisted {
        Persisted { uuids: self.order.clone() }
    }
}

/// Write `table` out under `account`'s key: on web this is localStorage, so
/// ids survive the reload; native keeps the same shape on its own store.
fn persist(account: String, table: Table) {
    if account.is_empty() {
        return;
    }
    let payload = table.persisted();
    let encoded = json_to_string(payload).unwrap_or_default();
    let _ = store().set_now(&key(account), &encoded);
}

/// Load `account`'s table straight from storage (a reload's fresh process
/// re-seeds from the persisted list, so the i64s keep meaning the same UUIDs).
fn load(account: String) -> Table {
    let held = store().get_now(&key(account)).ok().flatten().unwrap_or_default();
    persisted_table(held)
}

/// A table rebuilt from a persisted list: position plus `FIRST_ID` is the
/// i64 a UUID first got. An absent or unreadable value starts a fresh table.
fn persisted_table(held: String) -> Table {
    let uuids = match json_from_str::<Persisted>(held) {
        Ok(parsed) => parsed.uuids,
        Err(_) => Vec::new(),
    };
    let table = Table.empty();
    for (index, uuid) in uuids.iter().enumerate() {
        table.order.push(uuid);
        let id = FIRST_ID + index as i64;
        table.uuid_to_i.insert(uuid, id);
        table.i_to_uuid.insert(id, uuid);
    }
    table
}

/// The i64 for `uuid` under `account`: first-seen-first-numbered, and a UUID
/// this table has never seen is assigned and persisted here.
pub(crate) fn see(account: String, uuid: String) -> i64 {
    let table = load(account);
    let id = table.number(uuid);
    persist(account, table);
    id
}

/// The UUID behind an i64 under `account`, or `None`. The reverse map the
/// event-application path needs.
pub(crate) fn uuid_of(account: String, id: i64) -> Option<String> {
    load(account).i_to_uuid.get(id).clone()
}

/// The i64 behind a wire UUID under `account`, if this table has seen it.
pub(crate) fn id_of(account: String, uuid: String) -> Option<i64> {
    load(account).uuid_to_i.get(&uuid).clone()
}

/// What `account`'s table currently holds: the UUIDs in assigned order.
pub(crate) fn order(account: String) -> Vec<String> {
    load(account).order.clone()
}
