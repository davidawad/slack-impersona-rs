//! The Slack Web API surface this crate exposes: who the credentials
//! belong to, conversation history (and thread replies), search, and user
//! lookup. Everything here is read-only.
use crate::error::{Error, Result};
use crate::message::{Message, Query};
use crate::parse::{self, ThreadRef, User};
use crate::transport::{self, Transport};
use std::collections::BTreeMap;

const PAGE: usize = 100;
/// Bound on `users.info` lookups per call (names are best-effort).
const MAX_NAME_LOOKUPS: usize = 25;

/// A Slack Web API session over a [`Transport`] (real HTTP, or a fixture
/// in tests).
pub struct Client<'a> {
    transport: &'a dyn Transport,
    sleep: &'a dyn Fn(f64),
}

impl<'a> Client<'a> {
    pub fn new(transport: &'a dyn Transport, sleep: &'a dyn Fn(f64)) -> Self {
        Self { transport, sleep }
    }

    fn call(&self, method: &str, form: &[(&str, String)]) -> Result<serde_json::Value> {
        transport::call(self.transport, self.sleep, method, form)
    }

    /// `auth.test`: which team and user the credentials belong to.
    pub fn whoami(&self) -> Result<serde_json::Value> {
        let v = self.call("auth.test", &[])?;
        Ok(serde_json::json!({
            "team": v.get("team"), "user": v.get("user"), "url": v.get("url"),
            "team_id": v.get("team_id"), "user_id": v.get("user_id"),
        }))
    }

    /// `users.info`: one user's id, handle and names.
    pub fn user(&self, user_id: &str) -> Result<User> {
        let v = self.call("users.info", &[("user", user_id.to_string())])?;
        parse::user(&v).ok_or_else(|| Error::UnexpectedShape {
            method: "users.info".into(),
            pointer: "/user".into(),
        })
    }

    /// One conversation, or one thread within it (`C0123` or
    /// `C0123:1700000000.000100`): the newest `limit` messages, oldest first.
    pub fn history(&self, thread: &str, limit: usize) -> Result<Vec<Message>> {
        let (method, channel, mut form) = match ThreadRef::parse(thread)? {
            ThreadRef::Conversation(c) => {
                ("conversations.history", c.clone(), vec![("channel", c)])
            }
            ThreadRef::Thread { channel, ts } => (
                "conversations.replies",
                channel.clone(),
                vec![("channel", channel), ("ts", ts)],
            ),
        };
        form.push(("limit", limit.clamp(1, 1000).to_string()));
        let v = self.call(method, &form)?;
        let mut msgs = parse::history(&channel, &v)?;
        // History comes back newest-first from Slack; keep the newest `limit`.
        let skip = msgs.len().saturating_sub(limit);
        msgs.drain(..skip);
        Ok(self.named(msgs))
    }

    /// `search.messages` over [`Query`], newest first, paged until `limit`
    /// or Slack's results run out.
    pub fn search(&self, q: &Query) -> Result<Vec<Message>> {
        let query = search_query(q);
        if query.is_empty() {
            return Err(Error::Other("a search needs text or a time window".into()));
        }
        let msgs: Vec<Message> = self
            .search_pages(&query, q.limit)?
            .into_iter()
            .filter(at_or_after(q.since))
            .collect();
        Ok(self.named(msgs))
    }

    fn search_pages(&self, query: &str, limit: usize) -> Result<Vec<Message>> {
        let mut out = Vec::new();
        for page in 1.. {
            let v = self.call(
                "search.messages",
                &[
                    ("query", query.to_string()),
                    ("sort", "timestamp".into()),
                    ("sort_dir", "desc".into()),
                    ("count", PAGE.min(limit).to_string()),
                    ("page", page.to_string()),
                ],
            )?;
            let batch = parse::search(&v)?;
            let pages = v
                .pointer("/messages/paging/pages")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1);
            let done = batch.is_empty() || page as u64 >= pages;
            out.extend(batch);
            if done || out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    /// Replace user ids in `from` with display names, best-effort.
    fn named(&self, msgs: Vec<Message>) -> Vec<Message> {
        let names: BTreeMap<String, String> = parse::unresolved_users(&msgs)
            .into_iter()
            .take(MAX_NAME_LOOKUPS)
            .filter_map(|id| {
                let v = self.call("users.info", &[("user", id.clone())]).ok()?;
                Some((id, parse::user_name(&v)?))
            })
            .collect();
        msgs.into_iter()
            .map(|m| match names.get(&m.from) {
                Some(n) => Message {
                    from: n.clone(),
                    ..m
                },
                None => m,
            })
            .collect()
    }
}

/// The `search.messages` query for a [`Query`].
pub fn search_query(q: &Query) -> String {
    let after = q.since.map(parse::after_clause);
    [q.text.clone(), after]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn at_or_after(since: Option<i64>) -> impl Fn(&Message) -> bool {
    move |m| {
        let ts =
            m.id.rsplit(':')
                .next()
                .and_then(|t| t.split('.').next())
                .and_then(|t| t.parse::<i64>().ok());
        match (since, ts) {
            (Some(s), Some(t)) => t >= s,
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_joins_text_and_after_clause() {
        let q = Query {
            text: Some("memo".into()),
            since: Some(1_700_000_000),
            limit: 10,
        };
        assert_eq!(search_query(&q), "memo after:2023-11-13");
        let q = Query {
            text: None,
            since: None,
            limit: 10,
        };
        assert_eq!(search_query(&q), "");
    }
}
