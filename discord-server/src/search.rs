//! Message search with Discord's filters.
//!
//! A query is free text plus any of `from:<user>`, `mentions:<user>`,
//! `in:<channel>`, `has:link|image|file`, `pinned:true|false`,
//! `before:<date>`, `after:<date>` and `during:<date>` (dates are
//! `YYYY-MM-DD` in the searcher's time zone). Values with spaces go in
//! quotes: `from:"Tim the Tester"`. [`parse`] is shared so the client can
//! show what was understood; the server runs it with SQLite FTS5
//! (`0024_message_search.sql`) over the channels the caller can see.

use serde::{Deserialize, Serialize};
use server::{server, ServerError};

#[cfg(feature = "server")]
use crate::db::Db;
#[cfg(feature = "server")]
use crate::permissions::Permissions;

/// Results per page.
pub const PAGE_SIZE: i64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Has {
    Link,
    Image,
    File,
}

/// A parsed query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    /// Words and quoted phrases to match in the message text.
    pub terms: Vec<String>,
    pub from: Vec<String>,
    pub mentions: Vec<String>,
    pub in_channels: Vec<String>,
    pub has: Vec<Has>,
    pub pinned: Option<bool>,
    /// Days as `(year, month, day)`.
    pub before: Option<(i64, u32, u32)>,
    pub after: Option<(i64, u32, u32)>,
    pub during: Option<(i64, u32, u32)>,
}

impl Query {
    /// Whether the query asks for anything at all.
    pub fn is_empty(&self) -> bool {
        *self == Query::default()
    }
}

/// Split a query into words, keeping quoted runs (and `key:"quoted value"`)
/// together. Quotes are kept in the output so [`parse`] can tell phrases.
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

/// `YYYY-MM-DD` as `(year, month, day)`, checked against the calendar.
pub fn parse_date(s: &str) -> Option<(i64, u32, u32)> {
    let mut parts = s.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || d == 0 {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1) as usize];
    (d <= days).then_some((y, m, d))
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil((y, m, d): (i64, u32, u32)) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parse a query. Unknown `key:value` words and bad dates are searched as
/// plain text, so nothing typed is silently dropped.
pub fn parse(text: &str) -> Query {
    let mut q = Query::default();
    for tok in tokens(text) {
        let Some((key, value)) = tok.split_once(':') else {
            push_term(&mut q, &tok);
            continue;
        };
        let value = unquote(value);
        let lower = value.to_ascii_lowercase();
        let handled = !value.is_empty()
            && match key.to_ascii_lowercase().as_str() {
                "from" => {
                    q.from.push(value.trim_start_matches('@').to_string());
                    true
                }
                "mentions" => {
                    q.mentions.push(value.trim_start_matches('@').to_string());
                    true
                }
                "in" => {
                    q.in_channels.push(value.trim_start_matches('#').to_string());
                    true
                }
                "has" => match lower.as_str() {
                    "link" => Some(Has::Link),
                    "image" => Some(Has::Image),
                    "file" => Some(Has::File),
                    _ => None,
                }
                .map(|h| q.has.push(h))
                .is_some(),
                "pinned" => match lower.as_str() {
                    "true" | "yes" => Some(true),
                    "false" | "no" => Some(false),
                    _ => None,
                }
                .map(|p| q.pinned = Some(p))
                .is_some(),
                "before" => parse_date(&value).map(|d| q.before = Some(d)).is_some(),
                "after" => parse_date(&value).map(|d| q.after = Some(d)).is_some(),
                "during" | "on" => parse_date(&value).map(|d| q.during = Some(d)).is_some(),
                _ => false,
            };
        if !handled {
            push_term(&mut q, &tok);
        }
    }
    q
}

fn push_term(q: &mut Query, tok: &str) {
    let t = unquote(tok);
    if !t.trim().is_empty() {
        q.terms.push(t);
    }
}

/// The FTS5 MATCH expression for the terms: each quoted (so operators and
/// punctuation are literal), single words as prefixes, all required.
pub fn fts_expression(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| {
            let escaped = t.replace('"', "\"\"");
            if t.contains(char::is_whitespace) {
                format!("\"{escaped}\"")
            } else {
                format!("\"{escaped}\"*")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The input for [`search_messages`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchInput {
    pub server_id: i64,
    pub query: String,
    /// The searcher's offset from UTC in minutes (e.g. -300 for UTC-5), for
    /// the date filters.
    #[serde(default)]
    pub tz_offset_minutes: i64,
    /// How many results to skip (pages of [`PAGE_SIZE`]).
    #[serde(default)]
    pub offset: i64,
}

/// One matching message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHit {
    pub message_id: i64,
    pub channel_id: i64,
    pub channel_name: String,
    pub author_id: i64,
    pub author_username: String,
    pub author_display_name: Option<String>,
    pub author_avatar: Option<String>,
    pub content: String,
    pub created_at: i64,
    pub attachment_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SearchResults {
    /// All matches, not just this page.
    pub total: i64,
    /// Newest first.
    pub hits: Vec<SearchHit>,
    /// What the query was understood as.
    pub query: Query,
}

#[cfg(feature = "server")]
pub struct SearchService {
    db: Db,
}

#[cfg(feature = "server")]
impl SearchService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn search(&self, caller: i64, input: &SearchInput) -> Result<SearchResults, String> {
        use rusqlite::types::Value;
        let query = parse(&input.query);
        let empty = || Ok(SearchResults { total: 0, hits: Vec::new(), query: query.clone() });
        if query.is_empty() {
            return empty();
        }
        let (ctx, member) = crate::permissions::member_in(&self.db, input.server_id, caller)?;
        let channels = self.db.channels_for_server(input.server_id);
        let mut channel_ids: Vec<i64> = channels
            .iter()
            .filter(|(id, _, kind, _, _)| {
                kind == "text" && ctx.in_channel(&member, *id).contains(Permissions::VIEW_CHANNEL)
            })
            .filter(|(_, name, ..)| {
                query.in_channels.is_empty() || query.in_channels.iter().any(|n| n.eq_ignore_ascii_case(name))
            })
            .map(|(id, ..)| *id)
            .collect();
        channel_ids.sort_unstable();
        let members = self.db.server_members(input.server_id);
        let people = |names: &[String]| -> Vec<i64> {
            members
                .iter()
                .filter(|(id, username, _)| {
                    let shown = self.db.user_display_name(*id);
                    names.iter().any(|n| {
                        n.eq_ignore_ascii_case(username) || shown.as_deref().is_some_and(|s| n.eq_ignore_ascii_case(s))
                    })
                })
                .map(|(id, ..)| *id)
                .collect()
        };
        let from = people(&query.from);
        let mentions = people(&query.mentions);
        if channel_ids.is_empty()
            || (!query.from.is_empty() && from.is_empty())
            || (!query.mentions.is_empty() && mentions.is_empty())
        {
            return empty();
        }

        let mut sql = String::from(" FROM messages m ");
        let mut params: Vec<Value> = Vec::new();
        if !query.terms.is_empty() {
            sql.push_str("JOIN messages_fts ON messages_fts.rowid = m.id AND messages_fts MATCH ? ");
            params.push(Value::Text(fts_expression(&query.terms)));
        }
        let list = |ids: &[i64], params: &mut Vec<Value>| {
            params.extend(ids.iter().map(|id| Value::Integer(*id)));
            vec!["?"; ids.len()].join(",")
        };
        sql.push_str(&format!("WHERE m.channel_id IN ({}) ", list(&channel_ids, &mut params)));
        if !from.is_empty() {
            sql.push_str(&format!("AND m.author_id IN ({}) ", list(&from, &mut params)));
        }
        if !mentions.is_empty() {
            sql.push_str(&format!(
                "AND EXISTS (SELECT 1 FROM message_mentions mm WHERE mm.message_id = m.id AND mm.user_id IN ({})) ",
                list(&mentions, &mut params)
            ));
        }
        let blocked = self.db.blocked_ids_for_user(caller);
        if !blocked.is_empty() {
            sql.push_str(&format!("AND m.author_id NOT IN ({}) ", list(&blocked, &mut params)));
        }
        for has in &query.has {
            sql.push_str(match has {
                Has::Link => "AND (m.content LIKE '%http://%' OR m.content LIKE '%https://%') ",
                Has::Image => "AND EXISTS (SELECT 1 FROM message_attachments a WHERE a.message_id = m.id AND a.mime LIKE 'image/%') ",
                Has::File => "AND EXISTS (SELECT 1 FROM message_attachments a WHERE a.message_id = m.id) ",
            });
        }
        match query.pinned {
            Some(true) => sql.push_str("AND EXISTS (SELECT 1 FROM message_pins p WHERE p.message_id = m.id) "),
            Some(false) => sql.push_str("AND NOT EXISTS (SELECT 1 FROM message_pins p WHERE p.message_id = m.id) "),
            None => {}
        }
        // Local midnight of a day, in UTC epoch ms.
        let day_start = |d| (days_from_civil(d) * 86_400 - input.tz_offset_minutes * 60) * 1000;
        if let Some(d) = query.before {
            sql.push_str("AND m.created_at < ? ");
            params.push(Value::Integer(day_start(d)));
        }
        if let Some(d) = query.after {
            sql.push_str("AND m.created_at >= ? ");
            params.push(Value::Integer(day_start(d) + 86_400_000));
        }
        if let Some(d) = query.during {
            sql.push_str("AND m.created_at >= ? AND m.created_at < ? ");
            params.push(Value::Integer(day_start(d)));
            params.push(Value::Integer(day_start(d) + 86_400_000));
        }

        let c = self.db.lock().map_err(|e| e.to_string())?;
        let fail = |e: rusqlite::Error| format!("search failed: {e}");
        let total: i64 = c
            .query_row(&format!("SELECT COUNT(*){sql}"), rusqlite::params_from_iter(params.iter()), |r| r.get(0))
            .map_err(fail)?;
        params.push(Value::Integer(PAGE_SIZE));
        params.push(Value::Integer(input.offset.max(0)));
        let rows: Vec<(i64, i64, i64, String, i64, i64)> = c
            .prepare(&format!(
                "SELECT m.id, m.channel_id, m.author_id, m.content, m.created_at,
                        (SELECT COUNT(*) FROM message_attachments a WHERE a.message_id = m.id)
                 {sql} ORDER BY m.id DESC LIMIT ? OFFSET ?"
            ))
            .and_then(|mut s| {
                s.query_map(rusqlite::params_from_iter(params.iter()), |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                })?
                .collect()
            })
            .map_err(fail)?;
        drop(c);
        let hits = rows
            .into_iter()
            .map(|(message_id, channel_id, author_id, content, created_at, attachment_count)| SearchHit {
                message_id,
                channel_id,
                channel_name: channels.iter().find(|c| c.0 == channel_id).map(|c| c.1.clone()).unwrap_or_default(),
                author_id,
                author_username: self.db.user_by_id(author_id).map(|(_, n)| n).unwrap_or_default(),
                author_display_name: self.db.user_display_name(author_id),
                author_avatar: self.db.user_avatar(author_id),
                content,
                created_at,
                attachment_count,
            })
            .collect();
        Ok(SearchResults { total, hits, query })
    }
}

/// Search a server's messages. Only channels the caller can see are
/// searched, and messages from people they blocked are left out.
#[server]
pub async fn search_messages(input: SearchInput) -> Result<SearchResults, ServerError> {
    let db = server::use_state::<Db>().ok_or_else(|| ServerError::failed("database not installed"))?;
    let caller = crate::session::acting_user().ok_or_else(|| ServerError::failed("unauthenticated"))?;
    SearchService::new(db).search(caller, &input).map_err(ServerError::failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_filters_terms_and_quotes() {
        let q = parse(r#"deploy from:@tim in:#general has:image "release notes" pinned:yes during:2026-10-28"#);
        assert_eq!(q.terms, vec!["deploy".to_string(), "release notes".to_string()]);
        assert_eq!(q.from, vec!["tim".to_string()]);
        assert_eq!(q.in_channels, vec!["general".to_string()]);
        assert_eq!(q.has, vec![Has::Image]);
        assert_eq!(q.pinned, Some(true));
        assert_eq!(q.during, Some((2026, 10, 28)));
        let q = parse(r#"from:"Tim the Tester" mentions:bob"#);
        assert_eq!((q.from, q.mentions), (vec!["Tim the Tester".to_string()], vec!["bob".to_string()]));
    }

    #[test]
    fn unknown_filters_and_bad_values_are_searched_as_text() {
        let q = parse("color:red has:video before:2026-02-30 http://x.y from:");
        assert_eq!(q.terms, vec!["color:red", "has:video", "before:2026-02-30", "http://x.y", "from:"]);
        assert!(q.has.is_empty() && q.before.is_none() && q.from.is_empty());
        assert!(parse("   ").is_empty());
    }

    #[test]
    fn dates_and_days() {
        assert_eq!(parse_date("2024-02-29"), Some((2024, 2, 29)));
        assert_eq!(parse_date("2023-02-29"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(days_from_civil((1970, 1, 1)), 0);
        assert_eq!(days_from_civil((2000, 3, 1)), 11017);
        assert_eq!(days_from_civil((1969, 12, 31)), -1);
    }

    #[test]
    fn fts_expression_quotes_everything() {
        let terms = vec!["deploy".to_string(), "release notes".to_string(), "a\"b".to_string(), "NOT".to_string()];
        assert_eq!(fts_expression(&terms), r#""deploy"* "release notes" "a""b"* "NOT"*"#);
    }

    #[cfg(feature = "server")]
    mod db_tests {
        use super::super::*;
        use std::path::Path;

        /// alice owns server 1 (#general 10, #secret 11 hidden from
        /// @everyone); bob (display name "Bobby") and carol are members.
        fn setup() -> (tempfile::TempDir, Db) {
            let dir = tempfile::tempdir().unwrap();
            let db = Db::migrate(&dir.path().join("t.db"), &Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations")).unwrap();
            db.lock()
                .unwrap()
                .execute_batch(
                    "INSERT INTO users (id, username, email, created_at, display_name) VALUES
                        (1,'alice','a@x',1,NULL),(2,'bob','b@x',1,'Bobby'),(3,'carol','c@x',1,NULL);
                     INSERT INTO servers (id, name, owner_id, created_at) VALUES (1,'S',1,1);
                     INSERT INTO server_memberships (server_id, user_id, role, joined_at) VALUES
                        (1,1,'owner',1),(1,2,'member',1),(1,3,'member',1);
                     INSERT INTO channels (id, server_id, name, kind, position, created_at) VALUES
                        (10,1,'general','text',0,1),(11,1,'secret','text',1,1);
                     INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny) VALUES (11,'everyone',0,0,1);
                     INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES
                        (1,10,1,'Deploying the release tonight',1761609600000),
                        (2,10,2,'release notes are at https://example.com',1761696000000),
                        (3,11,1,'secret release plans',1761696000000),
                        (4,10,3,'unrelated chatter',1761782400000);
                     INSERT INTO message_mentions (message_id, user_id) VALUES (4,2);
                     INSERT INTO message_pins (message_id, pinned_by, pinned_at) VALUES (2,1,1);",
                )
                .unwrap();
            (dir, db)
        }

        fn ids(db: &Db, caller: i64, query: &str) -> Vec<i64> {
            let input = SearchInput { server_id: 1, query: query.into(), tz_offset_minutes: 0, offset: 0 };
            SearchService::new(db.clone()).search(caller, &input).unwrap().hits.iter().map(|h| h.message_id).collect()
        }

        #[test]
        fn text_search_respects_channel_visibility() {
            let (_d, db) = setup();
            assert_eq!(ids(&db, 1, "release"), vec![3, 2, 1], "the owner sees #secret; prefixes match");
            assert_eq!(ids(&db, 2, "release"), vec![2, 1], "bob can't see #secret");
            assert_eq!(ids(&db, 2, "\"release notes\""), vec![2]);
            assert!(ids(&db, 2, "nothing-like-this").is_empty());
        }

        #[test]
        fn filters_narrow_the_results() {
            let (_d, db) = setup();
            assert_eq!(ids(&db, 1, "from:bob"), vec![2]);
            assert_eq!(ids(&db, 1, "from:Bobby"), vec![2], "display names work too");
            assert_eq!(ids(&db, 1, "mentions:bob"), vec![4]);
            assert_eq!(ids(&db, 1, "in:secret"), vec![3]);
            assert!(ids(&db, 2, "in:secret").is_empty());
            assert_eq!(ids(&db, 1, "has:link"), vec![2]);
            assert_eq!(ids(&db, 1, "pinned:true"), vec![2]);
            assert_eq!(ids(&db, 1, "release pinned:false in:general"), vec![1]);
            // 1761609600000 is 2025-10-28 00:00 UTC.
            assert_eq!(ids(&db, 1, "during:2025-10-28"), vec![1]);
            assert_eq!(ids(&db, 1, "after:2025-10-29"), vec![4]);
            assert_eq!(ids(&db, 1, "before:2025-10-29 in:general"), vec![1]);
            assert!(ids(&db, 1, "from:nobody").is_empty());
        }

        #[test]
        fn edits_and_deletes_update_the_index_and_blocks_hide_authors() {
            let (_d, db) = setup();
            db.lock().unwrap().execute_batch("UPDATE messages SET content = 'totally different' WHERE id = 1;").unwrap();
            assert_eq!(ids(&db, 1, "deploying"), Vec::<i64>::new());
            assert_eq!(ids(&db, 1, "totally"), vec![1]);
            db.lock().unwrap().execute_batch("DELETE FROM messages WHERE id = 2;").unwrap();
            assert_eq!(ids(&db, 1, "notes"), Vec::<i64>::new());
            db.insert_user_block(1, 3);
            assert!(ids(&db, 1, "chatter").is_empty());
        }

        #[test]
        fn strangers_are_refused_and_results_page() {
            let (_d, db) = setup();
            let input = SearchInput { server_id: 1, query: "release".into(), tz_offset_minutes: 0, offset: 0 };
            assert!(SearchService::new(db.clone()).search(99, &input).is_err());
            let page = SearchService::new(db.clone()).search(1, &SearchInput { offset: 2, ..input }).unwrap();
            assert_eq!((page.total, page.hits.len()), (3, 1));
        }
    }
}
