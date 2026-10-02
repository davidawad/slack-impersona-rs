//! The client end to end over recorded Web API responses (no network).
use slack_impersona::transport::{Reply, Transport};
use slack_impersona::{Client, Query, Result};
use std::cell::RefCell;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// (method, form fields) of one Web API call.
type Call = (String, Vec<(String, String)>);

/// Answers each Web API method with its fixture and records every call.
struct Recorded(RefCell<Vec<Call>>);

impl Transport for Recorded {
    fn post(&self, method: &str, form: &[(&str, String)]) -> Result<Reply> {
        self.0.borrow_mut().push((
            method.into(),
            form.iter().map(|(k, v)| ((*k).into(), v.clone())).collect(),
        ));
        let body = match method {
            "auth.test" => fixture("slack_auth_test.json"),
            "conversations.history" | "conversations.replies" => fixture("slack_history.json"),
            "search.messages" => fixture("slack_search.json"),
            "users.info" if form.iter().any(|(_, v)| v == "U03BOB") => {
                fixture("slack_users_info.json")
            }
            _ => r#"{"ok":false,"error":"user_not_found"}"#.into(),
        };
        Ok(Reply {
            status: 200,
            retry_after: None,
            body,
        })
    }
}

fn arg<'a>(calls: &'a [Call], method: &str, key: &str) -> Option<&'a str> {
    calls
        .iter()
        .find(|(m, _)| m == method)?
        .1
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn whoami_reports_the_account() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    let who = c.whoami().unwrap();
    assert_eq!(who["team"], "Example Team");
    assert_eq!(who["user_id"], "U01ALICE");
}

#[test]
fn history_resolves_names_and_keeps_oldest_first() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    let msgs = c.history("C01GENERAL", 50).unwrap();
    assert_eq!(msgs.len(), 3, "the ts-less join event is dropped");
    let root = &msgs[0];
    assert_eq!(root.id, "C01GENERAL:1759228000.000100");
    assert_eq!(
        root.thread, "C01GENERAL:1759228000.000100",
        "a thread root opens its thread"
    );
    assert_eq!(root.from, "U01ALICE", "no resolvable name, stays raw");
    assert_eq!(root.date, "2025-09-30T10:26:40Z");
    assert_eq!(
        root.links,
        vec!["https://example-team.slack.com/files/U01ALICE/F1/memo.pdf"]
    );
    assert_eq!(
        msgs[1].thread, "C01GENERAL:1759228000.000100",
        "a reply points at its root"
    );
    let bot = &msgs[2];
    assert_eq!(bot.from, "deploybot");
    assert_eq!(bot.thread, "C01GENERAL");
    assert_eq!(
        bot.body,
        "deploy <prod> done & green: <https://ci.example.com/run/42|run 42>"
    );
    assert_eq!(bot.links, vec!["https://ci.example.com/run/42"]);
}

#[test]
fn history_distinguishes_conversation_and_thread_and_rejects_bad_refs() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    c.history("C01GENERAL:1759228000.000100", 50).unwrap();
    let calls = t.0.borrow();
    assert_eq!(
        arg(&calls, "conversations.history", "channel"),
        None,
        "a thread ref calls conversations.replies, not history"
    );
    assert_eq!(
        arg(&calls, "conversations.replies", "ts"),
        Some("1759228000.000100")
    );
    drop(calls);
    assert!(c.history("not a thread", 5).is_err());
}

#[test]
fn history_limit_keeps_the_newest_messages() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    let msgs = c.history("C01GENERAL", 2).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].from, "deploybot");
}

#[test]
fn search_resolves_names_newest_first_across_channels() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    let q = Query {
        text: Some("memo".into()),
        since: None,
        limit: 30,
    };
    let msgs = c.search(&q).unwrap();
    let calls = t.0.borrow();
    assert_eq!(arg(&calls, "search.messages", "query"), Some("memo"));
    assert_eq!(arg(&calls, "search.messages", "sort"), Some("timestamp"));
    assert_eq!(
        msgs.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec!["D01DM:1759231000.000500", "C01GENERAL:1759228000.000100"]
    );
    assert_eq!(msgs[0].from, "Bob Builder", "users.info resolves the id");
    assert_eq!(msgs[1].from, "alice");
    assert_eq!(msgs[1].links, vec!["https://docs.example.com/memo"]);
}

#[test]
fn search_needs_text_or_a_time_window() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    let q = Query {
        text: None,
        since: None,
        limit: 10,
    };
    assert!(c.search(&q).is_err());
}

#[test]
fn search_since_adds_the_after_clause_and_filters_older_matches() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    let q = Query {
        text: Some("memo".into()),
        since: Some(1_759_230_000),
        limit: 30,
    };
    let msgs = c.search(&q).unwrap();
    let calls = t.0.borrow();
    assert_eq!(
        arg(&calls, "search.messages", "query"),
        Some("memo after:2025-09-29")
    );
    assert_eq!(msgs.len(), 1, "alice's earlier match is filtered out");
}

#[test]
fn user_looks_up_the_display_name() {
    let t = Recorded(RefCell::new(vec![]));
    let c = Client::new(&t, &|_| {});
    let u = c.user("U03BOB").unwrap();
    assert_eq!(u.id, "U03BOB");
    assert_eq!(u.real_name.as_deref(), Some("Bob Builder"));
    assert!(c.user("U99MISSING").is_err());
}
