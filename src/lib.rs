//! `slack_impersona`: a Rust client for Slack's Web API that authenticates
//! the way a signed-in browser tab does -- an `xoxc-` session token plus
//! the `d` session cookie -- instead of a registered Slack app. That lets
//! a personal tool read a workspace's own history and search on the
//! owner's behalf without admin approval, at the cost of using
//! credentials that Slack's terms of service do not clearly sanction
//! outside its own client; see the README before relying on this for
//! anything you can't afford to lose.
//!
//! Read-only by default: [`Client::whoami`], [`Client::history`],
//! [`Client::search`] and [`Client::user`] only ever call read methods of
//! Slack's Web API. Posting a message ([`send::send`], behind the `send`
//! feature) is a typed library function with no CLI wiring, so turning it
//! on is a deliberate choice made at the `Cargo.toml` level, not a flag a
//! user can pass by accident.

pub mod client;
pub mod credentials;
pub mod error;
pub mod message;
pub mod parse;
#[cfg(feature = "send")]
pub mod send;
pub mod transport;

pub use client::Client;
pub use credentials::{Credentials, EnvNames};
pub use error::{Error, Result};
pub use message::{Message, Query};
pub use transport::{HttpTransport, Reply, Transport};
