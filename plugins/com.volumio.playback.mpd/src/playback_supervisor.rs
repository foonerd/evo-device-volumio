//! # Playback supervisor
//!
//! Long-lived orchestrator over two [`crate::mpd::MpdConnection`]
//! instances: the warden's answer to "what actually happens during
//! a custody". Phase 3.2c wires this module into the warden trait
//! impls in `lib.rs`; until then it is declared but unconsumed,
//! hence the module-level lint suppressions below.
//!
//! ## Layers
//!
//! - [`command`]: the [`PlaybackCommand`] enum (the things the
//!   warden tells the supervisor to do) and the
//!   [`PlaybackError`] hierarchy classifying supervisor failures
//!   for the warden to map onto `PluginError` variants.
//! - [`report`]: the [`PlaybackStateReport`] struct emitted on
//!   every state transition, plus its hand-rolled TOML serialiser
//!   (no `toml` or `serde` dependency in the critical path).
//! - [`actor`]: [`SupervisorHandle`] and [`spawn`]. Two tokio
//!   tasks communicate via channels to serve custody commands and
//!   emit state reports; reconnection with bounded exponential
//!   backoff is handled transparently.
//!
//! ## Lint suppressions (Phase 3.2b only)
//!
//! Same pattern as `crate::mpd`: the public-within-crate surface
//! is declared now but not consumed by the warden impl in
//! `lib.rs` until Phase 3.2c. `dead_code` and `unused_imports`
//! fire on the re-exports below until 3.2c's wiring retires them.

#![allow(dead_code)]
#![allow(unused_imports)]

mod actor;
mod command;
mod report;

pub(crate) use actor::{spawn, SupervisorHandle};
pub(crate) use command::{PlaybackCommand, PlaybackError};
pub(crate) use report::{CurrentSongReport, PlaybackStateReport};
