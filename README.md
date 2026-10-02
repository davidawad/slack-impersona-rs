# slack-impersona

A Rust library (`slack_impersona`) and CLI (`slack-impersona`) that talk to
Slack's Web API as a logged-in user, using the `xoxc-` session token plus
the `d` cookie that a signed-in browser holds -- no Slack app or admin
approval needed.

Read-only by default: `whoami`, `history`, `search`, and `users` (user
lookup), JSON out. Posting a message exists only as a library function
behind an opt-in `send` Cargo feature (default off) -- there is no `send`
CLI command, on purpose.

How you obtain the token and cookie is up to you; this crate only uses
them. It does not read browser state or automate a browser.

## Install

```sh
cargo install --path .
# or, to build only:
cargo build --release
```

## Usage

Every command needs credentials. By default the CLI reads them from the
`SLACK_TOKEN` and `SLACK_COOKIE` environment variables;
`--token-env`/`--cookie-env` let you point it at differently named
variables, and `--creds-file <path>` gives it a JSON fallback
(`{"token": "...", "cookie": "..."}`).

```sh
export SLACK_TOKEN="xoxc-..."
export SLACK_COOKIE="xoxd-..."        # the raw `d` cookie value, or a full "d=..." header

slack-impersona whoami
slack-impersona history C0123ABCDEF --limit 50
slack-impersona history 'C0123ABCDEF:1700000000.000100'   # a specific thread
slack-impersona search 'from:@alice has:link' --since 1700000000 --limit 20
slack-impersona users U0123ABCDE
```

### Getting credentials

Slack's web client stores your session as an `xoxc-` token in
`localStorage` (`localConfig_v2`) and a `d` session cookie, both scoped to
the workspace. You can read them from your browser's developer tools while
signed in. Treat the pair like a password: it carries your whole Slack
identity.

### Sending (opt-in, library only)

```toml
slack-impersona = { version = "0.1", features = ["send"] }
```

```rust
use slack_impersona::send::send;
// send(&transport, &sleep, "C0123ABCDEF", "hello", None)?;
```

There is no `slack-impersona send` CLI command. Enabling the feature is a
decision you make in your own `Cargo.toml`, not a flag this binary exposes.

## A note on terms of service

This authenticates the way a signed-in browser tab does, not through a
registered Slack app approved by a workspace admin. Slack's terms of
service do not clearly sanction session-token access outside its own
official clients; using this against a workspace you don't own, or in a
way your workspace's admins haven't agreed to, may violate those terms.
Use it on your own account, for your own data, and at your own risk.

## License

GPL-3.0-only. See `LICENSE`.
