//! The HTTP hop to Slack's Web API, behind the [`Transport`] trait so the
//! client can run on fixtures in tests. [`HttpTransport`] sends the
//! session token as both a bearer header and a form field (the way
//! Slack's own web client does), plus the `d` session cookie; a 429 or
//! `ok:false ratelimited` is retried with capped exponential backoff,
//! every other `ok:false` is an error.
use crate::error::{Error, Result};
use serde_json::Value;
use std::time::Duration;

pub const API_BASE: &str = "https://slack.com/api";
const MAX_RETRIES: u32 = 5;
const INITIAL_BACKOFF: f64 = 2.0;
const MAX_BACKOFF: f64 = 32.0;
/// A hostile or mistaken Retry-After must not park a caller indefinitely.
const MAX_RETRY_AFTER: f64 = 60.0;

pub struct Reply {
    pub status: u16,
    pub retry_after: Option<String>,
    pub body: String,
}

/// The one HTTP call the client needs, so it can run on fixtures in tests.
pub trait Transport {
    fn post(&self, method: &str, form: &[(&str, String)]) -> Result<Reply>;
}

/// A real Slack Web API connection: the session token/cookie pair sent
/// the way a signed-in browser tab sends them.
pub struct HttpTransport {
    client: reqwest::blocking::Client,
    token: String,
    cookie: String,
}

impl HttpTransport {
    pub fn new(token: String, cookie: String) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            client,
            token,
            cookie,
        })
    }

    pub fn from_credentials(creds: &crate::Credentials) -> Result<Self> {
        Self::new(creds.token.clone(), creds.cookie.clone())
    }
}

impl Transport for HttpTransport {
    fn post(&self, method: &str, form: &[(&str, String)]) -> Result<Reply> {
        let mut fields: Vec<(&str, &str)> = vec![("token", self.token.as_str())];
        fields.extend(form.iter().map(|(k, v)| (*k, v.as_str())));
        // Never format the token or cookie into an error: only the method.
        let resp = self
            .client
            .post(format!("{API_BASE}/{method}"))
            .bearer_auth(&self.token)
            .header(reqwest::header::COOKIE, &self.cookie)
            .form(&fields)
            .send()?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.text()?;
        Ok(Reply {
            status,
            retry_after,
            body,
        })
    }
}

/// Seconds to wait before the next attempt: a numeric Retry-After (capped),
/// else the exponential backoff.
pub fn retry_wait(retry_after: Option<&str>, backoff: f64) -> f64 {
    retry_after
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map_or(backoff, |v| v.clamp(0.0, MAX_RETRY_AFTER))
}

enum Outcome {
    Done(Value),
    RateLimited,
}

fn classify(method: &str, reply: &Reply) -> Result<Outcome> {
    if reply.status == 429 {
        return Ok(Outcome::RateLimited);
    }
    if !(200..300).contains(&reply.status) {
        return Err(Error::Http {
            method: method.to_string(),
            status: reply.status,
        });
    }
    let v: Value = serde_json::from_str(&reply.body).map_err(|source| Error::InvalidResponse {
        method: method.to_string(),
        source,
    })?;
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(Outcome::Done(v));
    }
    let error = v
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown_error")
        .to_string();
    match error.as_str() {
        "ratelimited" => Ok(Outcome::RateLimited),
        "invalid_auth" | "not_authed" | "token_revoked" | "token_expired" | "account_inactive" => {
            Err(Error::StaleCredentials {
                method: method.to_string(),
                error,
            })
        }
        _ => Err(Error::Api {
            method: method.to_string(),
            error,
        }),
    }
}

/// One Web API call with the retry policy; `sleep` is injected for tests.
pub fn call(
    t: &dyn Transport,
    sleep: &dyn Fn(f64),
    method: &str,
    form: &[(&str, String)],
) -> Result<Value> {
    let mut backoff = INITIAL_BACKOFF;
    for attempt in 0..=MAX_RETRIES {
        let reply = t.post(method, form)?;
        match classify(method, &reply)? {
            Outcome::Done(v) => return Ok(v),
            Outcome::RateLimited if attempt == MAX_RETRIES => break,
            Outcome::RateLimited => {
                sleep(retry_wait(reply.retry_after.as_deref(), backoff));
                backoff = (backoff * 2.0).min(MAX_BACKOFF);
            }
        }
    }
    Err(Error::RateLimited {
        method: method.to_string(),
        attempts: MAX_RETRIES + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Script(RefCell<Vec<Reply>>);

    impl Transport for Script {
        fn post(&self, _: &str, _: &[(&str, String)]) -> Result<Reply> {
            Ok(self.0.borrow_mut().remove(0))
        }
    }

    fn reply(status: u16, body: &str, retry_after: Option<&str>) -> Reply {
        Reply {
            status,
            retry_after: retry_after.map(str::to_string),
            body: body.into(),
        }
    }

    #[test]
    fn retries_429_then_succeeds_honoring_retry_after() {
        let t = Script(RefCell::new(vec![
            reply(429, "", Some("3")),
            reply(200, r#"{"ok":false,"error":"ratelimited"}"#, None),
            reply(200, r#"{"ok":true,"x":1}"#, None),
        ]));
        let waits = RefCell::new(vec![]);
        let v = call(&t, &|s| waits.borrow_mut().push(s), "m", &[]).unwrap();
        assert_eq!(v["x"], 1);
        assert_eq!(*waits.borrow(), vec![3.0, 4.0]);
    }

    #[test]
    fn gives_up_after_budget() {
        let t = Script(RefCell::new(
            (0..=MAX_RETRIES)
                .map(|_| reply(429, "", Some("999")))
                .collect(),
        ));
        let waits = RefCell::new(vec![]);
        let e = call(&t, &|s| waits.borrow_mut().push(s), "m", &[]).unwrap_err();
        assert!(e.to_string().contains("rate-limited after 6"));
        assert!(waits.borrow().iter().all(|w| *w == MAX_RETRY_AFTER));
    }

    #[test]
    fn errors_are_not_swallowed_and_stale_auth_says_so() {
        let t = Script(RefCell::new(vec![reply(
            200,
            r#"{"ok":false,"error":"channel_not_found"}"#,
            None,
        )]));
        assert!(call(&t, &|_| {}, "m", &[])
            .unwrap_err()
            .to_string()
            .contains("channel_not_found"));
        let t = Script(RefCell::new(vec![reply(
            200,
            r#"{"ok":false,"error":"invalid_auth"}"#,
            None,
        )]));
        assert!(call(&t, &|_| {}, "m", &[])
            .unwrap_err()
            .to_string()
            .contains("get a fresh one"));
        let t = Script(RefCell::new(vec![reply(500, "oops", None)]));
        assert!(call(&t, &|_| {}, "m", &[]).is_err());
    }

    #[test]
    fn retry_wait_parsing() {
        assert_eq!(retry_wait(Some(" 5 "), 2.0), 5.0);
        assert_eq!(retry_wait(Some("soon"), 2.0), 2.0);
        assert_eq!(retry_wait(Some("-3"), 2.0), 0.0);
        assert_eq!(retry_wait(None, 8.0), 8.0);
    }
}
