//! Slack JSON -> this crate's types. Pure, and unit-tested on fixtures.
use crate::error::{Error, Result};
use crate::message::{extract_links, rfc3339, Message};
use serde::Serialize;
use serde_json::Value;

/// What [`crate::Client::history`] opens: a whole conversation, or one
/// thread within it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadRef {
    Conversation(String),
    Thread { channel: String, ts: String },
}

impl ThreadRef {
    /// `C0123` or `C0123:1700000000.000100`.
    pub fn parse(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        let ok_id = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric());
        let ok_ts = |s: &str| {
            s.split_once('.')
                .is_some_and(|(a, b)| is_digits(a) && is_digits(b))
        };
        match raw.split_once(':') {
            None if ok_id(raw) => Ok(Self::Conversation(raw.to_string())),
            Some((c, ts)) if ok_id(c) && ok_ts(ts) => Ok(Self::Thread {
                channel: c.into(),
                ts: ts.into(),
            }),
            _ => Err(Error::InvalidThreadRef {
                raw: raw.to_string(),
            }),
        }
    }
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Slack escapes only these three in message text.
pub fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// `1700000000.000100` -> RFC 3339; the raw ts if it does not parse.
pub fn ts_date(ts: &str) -> String {
    ts.split('.')
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(rfc3339)
        .unwrap_or_else(|| ts.to_string())
}

/// One Slack message object in `channel`. Messages without a `ts` are dropped.
pub fn message(channel: &str, m: &Value) -> Option<Message> {
    let ts = str_of(m, "ts")?;
    let body = unescape(str_of(m, "text").unwrap_or_default());
    let mut links = extract_links(&body);
    let file_links = m
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| str_of(f, "permalink"));
    for l in file_links {
        if !links.iter().any(|x| x == l) {
            links.push(l.to_string());
        }
    }
    let replies = m.get("reply_count").and_then(Value::as_u64).unwrap_or(0);
    let thread = match str_of(m, "thread_ts") {
        Some(t) => format!("{channel}:{t}"),
        None if replies > 0 => format!("{channel}:{ts}"),
        None => channel.to_string(),
    };
    let from = ["username", "user", "bot_id"]
        .iter()
        .find_map(|k| str_of(m, k))
        .unwrap_or("unknown")
        .to_string();
    Some(Message {
        id: format!("{channel}:{ts}"),
        thread,
        from,
        date: ts_date(ts),
        body,
        links,
    })
}

/// `conversations.history` / `conversations.replies`: oldest first.
pub fn history(channel: &str, payload: &Value) -> Result<Vec<Message>> {
    let msgs = payload
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::UnexpectedShape {
            method: "history".into(),
            pointer: "/messages".into(),
        })?;
    let mut out: Vec<Message> = msgs.iter().filter_map(|m| message(channel, m)).collect();
    out.sort_by(|a, b| sort_key(&a.id).total_cmp(&sort_key(&b.id)));
    Ok(out)
}

fn sort_key(id: &str) -> f64 {
    id.rsplit(':')
        .next()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0.0)
}

/// `search.messages`: each match carries its own `channel.id`; newest first.
pub fn search(payload: &Value) -> Result<Vec<Message>> {
    let matches = payload
        .pointer("/messages/matches")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::UnexpectedShape {
            method: "search".into(),
            pointer: "/messages/matches".into(),
        })?;
    let mut out: Vec<Message> = matches
        .iter()
        .filter_map(|m| message(m.pointer("/channel/id").and_then(Value::as_str)?, m))
        .collect();
    out.sort_by(|a, b| sort_key(&b.id).total_cmp(&sort_key(&a.id)));
    Ok(out)
}

/// User ids in `from` that still need a display name.
pub fn unresolved_users(msgs: &[Message]) -> Vec<String> {
    let mut ids: Vec<String> = msgs
        .iter()
        .map(|m| m.from.clone())
        .filter(|f| {
            f.len() > 1
                && (f.starts_with('U') || f.starts_with('W'))
                && f.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// A Slack user's identity, as returned by `users.info`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: String,
    pub name: Option<String>,
    pub real_name: Option<String>,
    pub display_name: Option<String>,
}

/// `users.info` -> a typed [`User`].
pub fn user(payload: &Value) -> Option<User> {
    let u = payload.get("user")?;
    Some(User {
        id: str_of(u, "id")?.to_string(),
        name: str_of(u, "name").map(str::to_string),
        real_name: str_of(u, "real_name").map(str::to_string),
        display_name: u
            .pointer("/profile/display_name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    })
}

/// `users.info` -> the name people see, best-effort (display name, else
/// real name, else the raw handle).
pub fn user_name(payload: &Value) -> Option<String> {
    let u = user(payload)?;
    u.display_name.or(u.real_name).or(u.name)
}

/// Slack `search.messages` date syntax for "at or after `secs`".
pub fn after_clause(secs: i64) -> String {
    // `after:` is exclusive of the named day, so name the day before.
    chrono::DateTime::from_timestamp(secs - 86_400, 0)
        .map(|d| format!("after:{}", d.format("%Y-%m-%d")))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_refs() {
        assert_eq!(
            ThreadRef::parse("C01").unwrap(),
            ThreadRef::Conversation("C01".into())
        );
        assert_eq!(
            ThreadRef::parse(" D9:1700000000.000100 ").unwrap(),
            ThreadRef::Thread {
                channel: "D9".into(),
                ts: "1700000000.000100".into()
            }
        );
        for bad in ["", "C01:abc", "C 1", "C01:1700", "../x"] {
            assert!(ThreadRef::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn after_clause_names_previous_day() {
        assert_eq!(after_clause(1_700_000_000), "after:2023-11-13");
    }

    #[test]
    fn user_name_prefers_display_then_real_then_handle() {
        let v = serde_json::json!({"user": {"id": "U1", "name": "bob", "real_name": "Bob Builder", "profile": {"display_name": ""}}});
        assert_eq!(user_name(&v).as_deref(), Some("Bob Builder"));
        let v = serde_json::json!({"user": {"id": "U1", "name": "bob", "profile": {"display_name": "bobby"}}});
        assert_eq!(user_name(&v).as_deref(), Some("bobby"));
        assert_eq!(user_name(&serde_json::json!({})), None);
    }
}
