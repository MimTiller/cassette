//! Instance-wide settings: the host's admin area configures the call
//! provider and the chat backend without editing env files.
//!
//! ## Design
//!
//! The store is the `instance_settings` key/value table (migration
//! `0035_instance_settings.sql`); env vars stay the fallback and the
//! headless-deploy path: a saved key wins over its env counterpart
//! (`CALL_PROVIDER`, `IDEA_CHAT_RELAY_URL`, `IDEA_CHAT_RELAY_SECRET`,
//! `IDEA_CHAT_RELAY_CERT_HASH`, `CHAT_BACKEND`, `IDEA_CHAT_URL`,
//! `INSTANCE_NAME`), and the answer labels which source each effective
//! value came from.
//!
//! Instance admins — who may read or change the store — live in the
//! `instance_admins` table, bootstrapped at startup from the comma-separated
//! `INSTANCE_ADMINS` env var (see `bin/server.rs`). An instance with no
//! admin is set up by the setup flow, not by auto-promoting the first
//! registered user.
//!
//! The call-provider config rides to the voice layer through a shared
//! handle ([`current_call_config`]): joins read it, and a save swaps it with
//! [`refresh_call_config`], so a setting change applies live without a
//! server restart.
//!
//! [`instance_info`] is public: name, provider kind, chat backend kind, and
//! whether setup is complete — nothing secret — so a client can read it at
//! startup before signing in.
//!
//! Authorization: every read and write checks the acting user against
//! `instance_admins` server-side; refusals are plain sentences the UI
//! shows verbatim.

use serde::{Deserialize, Serialize};

use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::session::acting_user;
#[cfg(feature = "server")]
use crate::voice::{CallConfig, PROVIDER_NONE};

/// Which layer a setting's effective value came from: the value saved in
/// the instance store, or the environment it fell back to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SettingSource {
    Env,
    Db,
}

/// The answer to a settings read: the effective value of every instance
/// setting plus the source it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceSettingsResult {
    pub instance_name: String,
    pub call_provider: String,
    pub call_provider_source: SettingSource,
    pub chat_backend: String,
    pub chat_backend_source: SettingSource,
    pub idea_chat_url: String,
    pub idea_chat_url_source: SettingSource,
    /// The voice join's refusal for this config ("" when a provider would
    /// answer), so the admin page can say whether the call setup works.
    #[serde(default)]
    pub provider_status: String,
    /// Whether this instance has at least one instance admin.
    #[serde(default)]
    pub setup_complete: bool,
}

/// Save instance settings. An empty field leaves that setting unchanged —
/// which is how an empty secret field keeps the old secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetInstanceSettingsInput {
    /// `none` | `idea-chat`.
    pub call_provider: String,
    /// `rwnd` | `idea-chat` (stored; the idea-chat chat backend is wired by
    /// a later task).
    pub chat_backend: String,
    pub idea_chat_url: String,
}

/// The public instance card: what kind of instance this is, nothing secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub instance_name: String,
    /// The call provider answering joins now: `idea-chat` | `none`.
    pub call_provider: String,
    /// The chat backend in use: `rwnd` | `idea-chat`.
    pub chat_backend: String,
    /// Whether instance setup is done (an instance admin exists).
    pub setup_complete: bool,
    /// Whether the CALLING user is an instance admin (false when
    /// unauthenticated) — what gates the client's settings gear entry.
    #[serde(default)]
    pub is_instance_admin: bool,
}

#[cfg(feature = "server")]
const CALL_IDEA_CHAT: &'static str = "idea-chat";
#[cfg(feature = "server")]
const CHAT_RWND: &'static str = "rwnd";
#[cfg(feature = "server")]
const CHAT_IDEA_CHAT: &'static str = "idea-chat";
#[cfg(feature = "server")]
const NOT_ADMIN: &'static str = "only instance admins can manage instance settings";
#[cfg(feature = "server")]
const DEFAULT_NAME: &'static str = "RWND instance";

#[cfg(feature = "server")]
/// The effective value of one key: the saved DB value over the env value
/// over the default, plus which source answered. An empty env value falls
/// through to the default.
pub fn db_or_env(db: &Db, key: &'static str, env: &'static str, default: &'static str) -> (String, SettingSource) {
    if let Some(v) = db.instance_setting(key.to_string()) {
        return (v, SettingSource::Db);
    }
    match std::env::var(env) {
        Ok(v) if !v.is_empty() => (v, SettingSource::Env),
        _ => (default.to_string(), SettingSource::Env),
    }
}

#[cfg(feature = "server")]
/// Fresh empty lock for the shared call-config handle, off the static-
/// initializer path (which only allows const calls).
fn new_call_config_lock() -> std::sync::Mutex<CallConfig> {
    std::sync::Mutex::new(CallConfig::from_env())
}

#[cfg(feature = "server")]
/// The shared call-config handle the voice layer reads: initialized from
/// the environment at first use, and swapped by [`refresh_call_config`]
/// whenever a setting is saved, so a change applies live without a restart.
static CALL_CONFIG: std::sync::OnceLock<std::sync::Mutex<CallConfig>> =
    std::sync::OnceLock::new();

#[cfg(feature = "server")]
/// The voice join's config right now: the shared handle.
pub fn current_call_config() -> CallConfig {
    CALL_CONFIG
        .get_or_init(new_call_config_lock)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

#[cfg(feature = "server")]
/// The voice config as it should be now: the saved DB values over the env
/// values. A fresh config for the shared handle.
pub fn call_config_of(db: &Db) -> CallConfig {
    CallConfig {
        provider: db_or_env(&db, "call_provider", "CALL_PROVIDER", PROVIDER_NONE).0,
        relay_url: db_or_env(&db, "idea_chat_relay_url", "IDEA_CHAT_RELAY_URL", "").0,
        relay_cert_hash: db_or_env(&db, "idea_chat_relay_cert_hash", "IDEA_CHAT_RELAY_CERT_HASH", "").0,
        relay_secret: db_or_env(&db, "idea_chat_relay_secret", "IDEA_CHAT_RELAY_SECRET", "").0,
    }
}

#[cfg(feature = "server")]
/// Swap the shared handle to the effective config of `db`: what a save
/// calls, so the next voice join sees the change without a restart.
pub fn refresh_call_config(db: Db) {
    let next = call_config_of(&db);
    let mut held = CALL_CONFIG
        .get_or_init(new_call_config_lock)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    *held = next;
}

#[cfg(feature = "server")]
/// The instance-settings service: reads and writes the store, gated on the
/// acting user holding an `instance_admins` row.
pub struct InstanceSettings {
    db: Db,
}

#[cfg(feature = "server")]
impl InstanceSettings {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Whether the caller may manage instance settings.
    pub fn authorize(&self, caller_id: i64) -> Result<(), String> {
        if !self.db.instance_admin(caller_id) {
            return Err(NOT_ADMIN.into());
        }
        Ok(())
    }

    /// The effective settings for an authorized caller.
    pub fn get(&self, caller_id: i64) -> Result<InstanceSettingsResult, String> {
        self.authorize(caller_id)?;
        Ok(self.read())
    }

    /// The effective value of every setting plus its source (caller already
    /// authorized). The secret comes back as [`SecretInfo`] only.
    pub fn read(&self) -> InstanceSettingsResult {
        let (provider, provider_src) = db_or_env(&self.db, "call_provider", "CALL_PROVIDER", PROVIDER_NONE);
        let (chat, chat_src) = db_or_env(&self.db, "chat_backend", "CHAT_BACKEND", CHAT_RWND);
        let (idea_url, idea_src) = db_or_env(&self.db, "idea_chat_url", "IDEA_CHAT_URL", "");
        let (name, _name_src) = db_or_env(&self.db, "instance_name", "INSTANCE_NAME", DEFAULT_NAME);
        InstanceSettingsResult {
            instance_name: name,
            call_provider: provider,
            call_provider_source: provider_src,
            chat_backend: chat,
            chat_backend_source: chat_src,
            idea_chat_url: idea_url,
            idea_chat_url_source: idea_src,
            provider_status: self.call_config().refusal().unwrap_or_default(),
            setup_complete: self.db.instance_admin_count() > 0,
        }
    }

    /// The instance chat backend kind, for the public card.
    pub fn chat_backend(&self) -> String {
        db_or_env(&self.db, "chat_backend", "CHAT_BACKEND", CHAT_RWND).0
    }

    /// Save settings: an empty field leaves that setting unchanged. The
    /// call provider and chat backend are checked before anything is
    /// written; the reserved `idea-chat` call provider is refused on save.
    /// On success the shared voice config swaps, so the next join uses the
    /// new provider without a restart.
    pub fn save(&self, caller_id: i64, input: SetInstanceSettingsInput) -> Result<InstanceSettingsResult, String> {
        self.authorize(caller_id)?;
        match input.call_provider.trim() {
            "" => (),
            CALL_IDEA_CHAT | PROVIDER_NONE => (),
            other => return Err(format!("call provider must be 'idea-chat' or 'none': {other}").into()),
        }
        match input.chat_backend.trim() {
            "" => (),
            CHAT_RWND | CHAT_IDEA_CHAT => (),
            _ => return Err("chat backend must be 'rwnd' or 'idea-chat'".into()),
        }
        self.store(input);
        refresh_call_config(self.db.clone());
        Ok(self.read())
    }

    /// Persist each non-empty field of a save.
    pub fn store(&self, input: SetInstanceSettingsInput) {
        let keep = |key: &'static str, value: String| {
            if !value.trim().is_empty() {
                self.db
                    .set_instance_setting(key.to_string(), value.trim().to_string())
                    .unwrap_or_else(|e| panic!("instance store {key}: {e}"));
            }
        };
        keep("call_provider", input.call_provider);
        keep("chat_backend", input.chat_backend);
        keep("idea_chat_url", input.idea_chat_url);
    }

    /// The effective call config (for the provider status line).
    pub fn call_config(&self) -> CallConfig {
        call_config_of(&self.db)
    }
}

#[cfg(feature = "server")]
/// The first-claim setup path: while the instance has no admin, one
/// authenticated account claims the instance (the setup flow). Once any
/// admin exists, only admins grant more; a normal user gets the refusal.
fn claim_admin_idempotent(db: &Db, caller_id: i64) -> Result<bool, String> {
    if db.instance_admin(caller_id) {
        return Ok(true);
    }
    if db.instance_admin_count() == 0 {
        db.grant_instance_admin(caller_id).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Err(NOT_ADMIN.into())
}

#[cfg(feature = "server")]
/// An admin granting another account instance-admin power (promoting a
/// co-host). Admin-only; `granted` false when the account was already one.
fn promote_admin(db: &Db, caller_id: i64, user_id: i64) -> Result<bool, String> {
    InstanceSettings::new(db.clone()).authorize(caller_id)?;
    if db.instance_admin(user_id) {
        return Ok(false);
    }
    db.grant_instance_admin(user_id).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(feature = "server")]
/// The env bootstrap extended to accounts registered after startup: when
/// `INSTANCE_ADMINS` names the username of a fresh signup, that account
/// becomes an instance admin — headless deploys keep working without the
/// UI setup flow.
pub fn grant_env_admin(db: &Db, username: String) {
    match std::env::var("INSTANCE_ADMINS") {
        Err(_) => (),
        Ok(list) =>
            if list.split(",").any(|name| name.trim() == username.trim()) {
                match db.user_by_username(&username) {
                    None => (),
                    Some((id, _)) => db.grant_instance_admin(id).unwrap_or_else(|e| panic!("env admin grant: {e}")),
                }
            },
    }
}

#[cfg(feature = "server")]
fn db_from_state() -> Result<Db, ServerError> {
    server::use_state::<Db>()
        .ok_or_else(|| ServerError::failed("database not installed"))
}

/// The public instance card: name, the call provider kind answering joins,
/// the chat backend kind, and whether setup is complete. Nothing secret —
/// no credentials in the answer — and no auth, so a client can read it at
/// startup before signing in.
#[server]
pub async fn instance_info() -> Result<InstanceInfo, ServerError> {
    let db = db_from_state()?;
    let admin = match acting_user() {
        Some(caller) => db.instance_admin(caller),
        None => false,
    };
    Ok(
        InstanceInfo {
            instance_name: db_or_env(&db, "instance_name", "INSTANCE_NAME", DEFAULT_NAME).0,
            call_provider: current_call_config().provider,
            chat_backend: InstanceSettings::new(db.clone()).chat_backend(),
            setup_complete: db.instance_admin_count() > 0,
            is_instance_admin: admin,
        },
    )
}

/// Read the instance settings: the effective value of every setting plus
/// its source. Admin-only; a saved secret comes back only as whether it is
/// set and the source it came from.
#[server]
pub async fn get_instance_settings(_input: ()) -> Result<InstanceSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let settings = InstanceSettings::new(db.clone())
        .get(caller)
        .map_err(ServerError::failed)?;
    Ok(settings)
}

/// What the admin page typed into "Test connection": an empty field falls
/// back to the saved (DB-over-env) value, so the test can run against
/// values that were never saved. The relay's own URL and secret come from
/// the store/environment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestCallInput {
    pub call_provider: String,
}

/// The answer to "Test connection": never an error — a provider-side
/// failure comes back as `ok: false` with the exact message, so the admin
/// page can paint it red without the RPC itself having failed. Only the
/// admin gate is an error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallTestResult {
    pub ok: bool,
    /// Rooms the provider reported (meaningful only when `ok`).
    pub rooms: i64,
    pub message: String,
}

#[cfg(feature = "server")]
/// What "Test connection" resolves to: the typed provider over the saved
/// (DB-over-env) one, and the relay's saved (DB-over-env) values — so the
/// admin can check the pairing without saving it first. Admin-only.
fn resolve_call_input(db: &Db, caller_id: i64, input: TestCallInput) -> Result<CallConfig, String> {
    InstanceSettings::new(db.clone()).authorize(caller_id)?;
    let pick = |typed: String, key: &'static str, env: &'static str, default: &'static str| {
        if !typed.trim().is_empty() {
            return typed.trim().to_string();
        }
        db_or_env(db, key, env, default).0
    };
    Ok(
        CallConfig {
            provider: pick(input.call_provider, "call_provider", "CALL_PROVIDER", PROVIDER_NONE),
            relay_url: pick(String::new(), "idea_chat_relay_url", "IDEA_CHAT_RELAY_URL", ""),
            relay_cert_hash: pick(String::new(), "idea_chat_relay_cert_hash", "IDEA_CHAT_RELAY_CERT_HASH", ""),
            relay_secret: pick(String::new(), "idea_chat_relay_secret", "IDEA_CHAT_RELAY_SECRET", ""),
        }
    )
}

#[cfg(feature = "server")]
fn test_call_error(message: String) -> CallTestResult {
    CallTestResult { ok: false, rooms: 0, message: message.to_string() }
}

#[cfg(feature = "server")]
fn test_call_ok(rooms: i64) -> CallTestResult {
    CallTestResult { ok: true, rooms, message: "the provider answered".to_string() }
}

#[cfg(feature = "server")]
/// The answer to "Test connection": for the idea-chat relay there is no
/// Twirp-style endpoint to dial (the relay speaks QUIC/WebTransport), so the
/// test checks the pairing — the relay URL and secret the joins would use —
/// and reports what would answer. A provider-side failure is `ok: false`
/// plus the exact message — never an RPC error — so the admin page paints
/// one red line. Pure so the routing is unit-testable without HTTP.
fn call_test_answer(cfg: CallConfig) -> CallTestResult {
    match cfg.provider.trim() {
        CALL_IDEA_CHAT =>
            if cfg.relay_url.trim().is_empty() || cfg.relay_secret.trim().is_empty() {
                test_call_error("idea-chat is configured but its relay URL or secret is missing".to_string())
            } else if !cfg.relay_url.trim().starts_with("https://") {
                test_call_error(format!("the relay URL is not usable: {}", cfg.relay_url))
            } else {
                test_call_ok(1)
            },
        PROVIDER_NONE => test_call_error("no call provider is selected".to_string()),
        other => test_call_error(format!("unknown call provider: {}", other)),
    }
}

#[cfg(feature = "server")]
/// The admin page's "Test connection": never saves anything — it resolves
/// the effective config (admin-gated) and asks what it answers with.
fn test_call_with(db: &Db, caller_id: i64, input: TestCallInput) -> Result<CallTestResult, String> {
    let cfg = resolve_call_input(db, caller_id, input)?;
    Ok(call_test_answer(cfg))
}

/// Test the call provider with the typed (or saved) values: resolves the
/// effective config and reports what the joins would get. Admin-only; never
/// saves.
#[server]
pub async fn test_call_provider(input: TestCallInput) -> Result<CallTestResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let outcome = match test_call_with(&db, caller, input) {
        Ok(result) => result,
        Err(reason) => return Err(ServerError::failed(reason)),
    };
    Ok(outcome)
}


/// Claim instance-admin power for the signed-in account: allowed only while
/// the instance has no admin at all (the setup flow). Once an admin exists,
/// only an admin can grant more, through `promote_instance_admin`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimAdminResult {
    pub is_instance_admin: bool,
}

#[server]
pub async fn claim_instance_admin(_input: ()) -> Result<ClaimAdminResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let claimed = match claim_admin_idempotent(&db, caller) {
        Ok(done) => done,
        Err(reason) => return Err(ServerError::failed(reason)),
    };
    Ok(ClaimAdminResult { is_instance_admin: claimed })
}

/// Give an account instance-admin power (promote a co-host). Admin-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromoteAdminInput {
    pub user_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromoteAdminResult {
    pub promoted: bool,
}

#[server]
pub async fn promote_instance_admin(input: PromoteAdminInput) -> Result<PromoteAdminResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let promoted = match promote_admin(&db, caller, input.user_id) {
        Ok(done) => done,
        Err(reason) => return Err(ServerError::failed(reason)),
    };
    Ok(PromoteAdminResult { promoted })
}

/// Save instance settings (empty fields keep the old values, including
/// secrets); the shared voice config swaps live so the next join uses the
/// new provider. Admin-only: a normal user gets the plain refusal.
#[server]
pub async fn set_instance_settings(input: SetInstanceSettingsInput) -> Result<InstanceSettingsResult, ServerError> {
    let db = db_from_state()?;
    let caller = acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    let settings = InstanceSettings::new(db.clone())
        .save(caller, input)
        .map_err(ServerError::failed)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_call_test_routes_by_provider() {
        let relay = CallConfig {
            provider: "idea-chat".to_string(),
            relay_url: "https://relay.x:8443/".to_string(),
            relay_cert_hash: "".to_string(),
            relay_secret: "s3cret".to_string(),
        };
        assert_eq!(call_test_answer(relay).ok, true, "a paired relay reports the joins will work");
        let missing = CallConfig {
            provider: "idea-chat".to_string(),
            relay_url: "".to_string(),
            relay_cert_hash: "".to_string(),
            relay_secret: "".to_string(),
        };
        assert_eq!(
            call_test_answer(missing).message,
            "idea-chat is configured but its relay URL or secret is missing".to_string(),
            "a half-set pairing says so",
        );
        assert_eq!(
            call_test_answer(CallConfig {
                provider: "gong".to_string(),
                relay_url: "https://x/".to_string(),
                relay_cert_hash: "".to_string(),
                relay_secret: "s".to_string(),
            })
                .message,
            "unknown call provider: gong".to_string(),
            "and so does anything else",
        );
    }
}

#[cfg(all(test, feature = "server"))]
mod db_tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    /// alice (instance admin), bob (a normal user) in the store's own DB.
    fn setup() -> (TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::migrate(
            &dir.path().join("t.db"),
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"),
        ).unwrap();
        (dir, db)
    }

    #[test]
    fn the_store_gates_on_the_admin_roster() {
        let (_d, db) = setup();
        db.lock().unwrap().execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'alice', 'a@x', 1), (2, 'bob', 'b@x', 1);",
        ).unwrap();
        db.grant_instance_admin(1).unwrap();
        let svc = InstanceSettings::new(db.clone());
        assert_eq!(svc.authorize(2).unwrap_err(), NOT_ADMIN, "a normal user reads the plain refusal");
        svc.authorize(1).unwrap();
        assert_eq!(svc.save(2, SetInstanceSettingsInput {
            call_provider: "idea-chat".to_string(), chat_backend: "bogus".to_string(), idea_chat_url: "".to_string(),
        }).unwrap_err(), NOT_ADMIN, "and cannot write either");
        let r = svc.save(1, SetInstanceSettingsInput {
            call_provider: "idea-chat".to_string(), chat_backend: "rwnd".to_string(), idea_chat_url: "https://chat.x".to_string(),
        }).unwrap();
        assert_eq!((r.call_provider, r.call_provider_source), ("idea-chat".to_string(), SettingSource::Db), "the save lands in the store");
        assert_eq!(db.instance_setting("call_provider".to_string()), Some("idea-chat".to_string()), "readback");
        let again = svc.save(1, SetInstanceSettingsInput {
            call_provider: "".to_string(), chat_backend: "".to_string(), idea_chat_url: "".to_string(),
        }).unwrap();
        assert_eq!(again.call_provider, "idea-chat", "an empty field keeps the old value");
        assert_eq!(again.chat_backend, "rwnd", "and every other field");
    }

    #[test]
    fn the_instance_admin_gate_covers_test_claim_and_promote() {
        let (_d, db) = setup();
        db.lock().unwrap().execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'alice', 'a@x', 1), (2, 'bob', 'b@x', 1), (3, 'carol', 'c@x', 1);",
        ).unwrap();
        db.grant_instance_admin(1).unwrap();
        // alice is an admin: testing works, and she can promote carol.
        let cfg = resolve_call_input(&db, 1, TestCallInput { call_provider: "none".to_string() }).unwrap();
        let answer = call_test_answer(cfg);
        assert_eq!(answer.message, "no call provider is selected".to_string(), "the admin reaches the test with a clear answer");
        assert_eq!(answer.ok, false, "no provider means no live test ran");
        assert_eq!(promote_admin(&db, 1, 3).unwrap(), true, "the admin promotes a co-host");
        assert_eq!(db.instance_admin(3), true, "carol is an admin now");
        // bob is not: every instance op refuses him with the same sentence.
        assert_eq!(
            match resolve_call_input(
                &db,
                2,
                TestCallInput { call_provider: "".to_string() },
            ) {
                Err(reason) => reason,
                Ok(_) => panic!("a normal user resolved a test config"),
            },
            NOT_ADMIN,
            "a normal user cannot even test",
        );
        assert_eq!(promote_admin(&db, 2, 3).unwrap_err(), NOT_ADMIN, "or promote");
        assert_eq!(claim_admin_idempotent(&db, 2).unwrap_err(), NOT_ADMIN, "or claim once an admin exists");
        // alice re-claiming is a no-op success (she is already one).
        assert_eq!(claim_admin_idempotent(&db, 1).unwrap(), true, "re-claiming keeps the flag");
    }

    #[test]
    fn the_first_admin_claims_once_then_claims_are_refused() {
        let (_d, db) = setup();
        db.lock().unwrap().execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES
                (1, 'alice', 'a@x', 1), (2, 'bob', 'b@x', 1);",
        ).unwrap();
        assert_eq!(db.instance_admin_count(), 0, "the store starts adminless");
        assert_eq!(claim_admin_idempotent(&db, 2).unwrap(), true, "the first account claims the instance");
        assert_eq!(claim_admin_idempotent(&db, 1).unwrap_err(), NOT_ADMIN, "the second gets the plain refusal");
        assert_eq!(db.instance_admin(2), true, "only the claimer holds the power");
    }

    #[test]
    fn an_unknown_call_provider_is_refused_on_save() {
        let (_d, db) = setup();
        db.lock().unwrap().execute_batch(
            "INSERT INTO users (id, username, email, created_at) VALUES (1, 'alice', 'a@x', 1);",
        ).unwrap();
        db.grant_instance_admin(1).unwrap();
        let svc = InstanceSettings::new(db.clone());
        let refused = svc.save(1, SetInstanceSettingsInput {
            call_provider: "live-stream".to_string(), chat_backend: "rwnd".to_string(), idea_chat_url: "".to_string(),
        }).unwrap_err();
        assert_eq!(refused, "call provider must be 'idea-chat' or 'none': live-stream", "a provider outside the picker refuses with its sentence");
    }
}
