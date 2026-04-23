//! # MPD connection layer
//!
//! Private implementation module for the MPD playback warden. Owns
//! the MPD wire protocol end-to-end: the implementation does not
//! depend on any third-party MPD crate, so the critical-path
//! dependency surface is bounded to crates the showcase fully
//! vendors and audits (tokio, tracing, thiserror).
//!
//! ## Design
//!
//! The module is structured as a short stack, each layer
//! responsible for one concern:
//!
//! - [`types`]: domain types (play state, version, narrow status
//!   and song shapes). No I/O, no parsing.
//! - [`error`]: classified error hierarchy. Every variant carries
//!   its underlying source through `#[source]` so `tracing`
//!   captures full causal chains.
//! - [`endpoint`]: server address type (TCP or Unix). Validates at
//!   construction; cannot represent an invalid endpoint.
//! - [`protocol`]: wire-format serialisation (commands out) and
//!   parsing (fields, OK/ACK terminators, welcome banner). Pure,
//!   no I/O, no time, no async - unit-testable against exact byte
//!   strings.
//! - [`framing`]: line-based reader/writer over arbitrary async
//!   byte streams, with mandatory timeouts and a hard line-length
//!   limit. Transport-agnostic: TCP, Unix, and in-memory duplex
//!   streams all work.
//! - [`connection`]: ties it together. Opens the transport, reads
//!   the welcome banner, dispatches commands with timeout budgets,
//!   projects protocol fields into the narrow domain types.
//!
//! ## Scope in this phase
//!
//! Phase 3.1 delivers everything above. Phase 3.2 will add:
//!
//! - Transport course-correction commands (play, pause, stop,
//!   next, previous, seek, volume).
//! - Event-driven state via the MPD `idle` subprotocol. That
//!   requires a second [`connection::MpdConnection`] held per
//!   custody - MPD blocks the connection during `idle`, so
//!   issuing a transport command requires a separate connection.
//! - Reconnection supervisor (retry with backoff when the
//!   connection fails mid-custody).
//! - Wiring into the warden's `take_custody` / `course_correct` /
//!   `release_custody` trait implementations.
//!
//! Phase 3.3 adds the configuration layer that produces the
//! [`endpoint::MpdEndpoint`] the connection opens. Phase 3.4 uses
//! the parsed [`types::MpdSong`] to assert `track` and `album`
//! subjects for Milestone 4's album-art respondent to walk.
//!
//! ## Lint suppressions (Phase 3.1 only)
//!
//! The two inner attributes below exist because this module's
//! public-within-crate surface is declared now but not yet
//! consumed by the warden impl in `lib.rs`:
//!
//! - `dead_code`: the pub(crate) items inside the submodules
//!   (endpoint, connection, types, error) have no call sites
//!   outside tests until Phase 3.2 wires them in.
//! - `unused_imports`: the `pub(crate) use` re-exports below are
//!   unused for the same reason.
//!
//! Phase 3.2's warden wiring is the natural retirement for both
//! suppressions; removing these attributes is a deliverable of
//! that phase.

#![allow(dead_code)]
#![allow(unused_imports)]

mod connection;
mod endpoint;
mod error;
mod framing;
mod protocol;
mod types;

// Public surface within the crate. Phase 3.2 consumes these from
// `crate::mpd::{...}`; internal paths into the submodules are not
// part of the module's contract.

pub(crate) use connection::{ConnectTimeouts, MpdConnection};
pub(crate) use endpoint::MpdEndpoint;
pub(crate) use error::{ConfigError, MpdError, ProtocolError, TransportError};
pub(crate) use types::{MpdSong, MpdStatus, MpdVersion, PlayState};
