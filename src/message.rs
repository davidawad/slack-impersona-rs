//! The unified message type this crate returns.
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Message {
    /// Opaque, channel-scoped id (`<channel>:<ts>`).
    pub id: String,
    /// What [`crate::Client::history`] accepts to open this message's
    /// conversation or thread.
    pub thread: String,
    pub from: String,
    /// RFC 3339 UTC when Slack gives a time, else the raw value.
    pub date: String,
    pub body: String,
    pub links: Vec<String>,
}

/// Which messages a search wants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// Slack search syntax; `None` means "no text, just the time window".
    pub text: Option<String>,
    /// Only messages at or after this Unix time.
    pub since: Option<i64>,
    pub limit: usize,
}

/// http(s) URLs in `text`, in order, deduplicated. Slack wraps links as
/// `<url>` or `<url|label>`, so `<`, `>` and `|` end a URL.
pub fn extract_links(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for start in text.match_indices("http").map(|(i, _)| i) {
        let rest = &text[start..];
        if !(rest.starts_with("http://") || rest.starts_with("https://")) {
            continue;
        }
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '|' | '"'))
            .unwrap_or(rest.len());
        let url = rest[..end].trim_end_matches(['.', ',', ')', ';', ':']);
        if url.len() > "https://".len() && !out.iter().any(|u| u == url) {
            out.push(url.to_string());
        }
    }
    out
}

/// Seconds since the epoch as RFC 3339 UTC; `None` when out of range.
pub fn rfc3339(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_from_slack_markup_and_plain_text() {
        let t = "see <https://a.example/x?y=1|the doc> and https://b.example/p. also <https://a.example/x?y=1>";
        assert_eq!(
            extract_links(t),
            vec!["https://a.example/x?y=1", "https://b.example/p"]
        );
        assert!(extract_links("httpx://nope http:// none").is_empty());
    }

    #[test]
    fn epoch_formatting() {
        assert_eq!(
            rfc3339(1_700_000_000).as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
    }
}
