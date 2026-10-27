//! `slack-impersona`: whoami/history/search/users over Slack's Web
//! API, authenticated with a session token and cookie instead of a
//! registered app. JSON out, always. Sending is a library-only feature
//! (`send`) and has no command here.
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use slack_impersona::transport::HttpTransport;
use slack_impersona::{credentials, Client, Query};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "slack-impersona", bin_name = "slack-impersona")]
struct Cli {
    /// Environment variable holding the `xoxc-` session token.
    #[arg(long, global = true, default_value = "SLACK_TOKEN")]
    token_env: String,
    /// Environment variable holding the `d` session cookie.
    #[arg(long, global = true, default_value = "SLACK_COOKIE")]
    cookie_env: String,
    /// Credentials file to fall back to (JSON: `{"token": ..., "cookie":
    /// ...}`).
    #[arg(long, global = true)]
    creds_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Which account the credentials belong to (`auth.test`).
    Whoami,
    /// One conversation or thread: `C0123` or `C0123:1700000000.000100`.
    History {
        thread: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// `search.messages` with Slack's own search syntax.
    Search {
        query: String,
        /// Only messages at or after this Unix time.
        #[arg(long)]
        since: Option<i64>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// `users.info` for one user id.
    Users { user_id: String },
}

fn run(cli: &Cli) -> Result<serde_json::Value> {
    let names = credentials::EnvNames::new(cli.token_env.clone(), cli.cookie_env.clone());
    let creds = credentials::load(&names, cli.creds_file.as_deref())
        .context("loading Slack credentials")?;
    let transport = HttpTransport::from_credentials(&creds)?;
    let sleep = |s: f64| std::thread::sleep(Duration::from_secs_f64(s));
    let client = Client::new(&transport, &sleep);
    Ok(match &cli.command {
        Cmd::Whoami => client.whoami()?,
        Cmd::History { thread, limit } => serde_json::json!(client.history(thread, *limit)?),
        Cmd::Search {
            query,
            since,
            limit,
        } => {
            anyhow::ensure!(!query.trim().is_empty(), "search needs a query");
            let q = Query {
                text: Some(query.clone()),
                since: *since,
                limit: (*limit).clamp(1, 500),
            };
            serde_json::json!(client.search(&q)?)
        }
        Cmd::Users { user_id } => serde_json::json!(client.user(user_id)?),
    })
}

fn main() {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}
