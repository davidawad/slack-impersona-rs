//! A typed function for posting a Slack message. Behind the `send`
//! feature (default off) and never wired into the CLI: turning this on is
//! a decision made in your own `Cargo.toml`, not a flag this binary
//! exposes.
use crate::error::Result;
use crate::transport::{self, Transport};
use serde_json::Value;

/// `chat.postMessage`: post `text` into `channel`, optionally as a thread
/// reply to `thread_ts`. Returns Slack's raw response.
pub fn send(
    transport: &dyn Transport,
    sleep: &dyn Fn(f64),
    channel: &str,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<Value> {
    let mut form = vec![("channel", channel.to_string()), ("text", text.to_string())];
    if let Some(ts) = thread_ts {
        form.push(("thread_ts", ts.to_string()));
    }
    transport::call(transport, sleep, "chat.postMessage", &form)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Reply;
    use std::cell::RefCell;

    struct Script(RefCell<Vec<Reply>>);

    impl Transport for Script {
        fn post(&self, _: &str, _: &[(&str, String)]) -> Result<Reply> {
            Ok(self.0.borrow_mut().remove(0))
        }
    }

    #[test]
    fn posts_text_and_optional_thread_ts() {
        let t = Script(RefCell::new(vec![Reply {
            status: 200,
            retry_after: None,
            body: r#"{"ok":true,"ts":"1.1"}"#.into(),
        }]));
        let v = send(&t, &|_| {}, "C1", "hi", Some("1700000000.000100")).unwrap();
        assert_eq!(v["ts"], "1.1");
    }

    #[test]
    fn thread_ts_is_only_sent_when_given() {
        struct Capturing(RefCell<Vec<(String, String)>>);
        impl Transport for Capturing {
            fn post(&self, _: &str, form: &[(&str, String)]) -> Result<Reply> {
                *self.0.borrow_mut() = form
                    .iter()
                    .map(|(k, v)| ((*k).to_string(), v.clone()))
                    .collect();
                Ok(Reply {
                    status: 200,
                    retry_after: None,
                    body: r#"{"ok":true}"#.into(),
                })
            }
        }
        let t = Capturing(RefCell::new(vec![]));
        send(&t, &|_| {}, "C1", "hi", None).unwrap();
        assert!(!t.0.borrow().iter().any(|(k, _)| k == "thread_ts"));
    }
}
