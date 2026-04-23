//! # Playback supervisor
//!
//! Long-lived orchestrator over two [`crate::mpd::MpdConnection`]
//! instances: the warden's answer to "what actually happens during
//! a custody". Phase 3.2c wires this module into the warden trait
//! impls in `lib.rs`; the lint suppressions that guarded the
//! declared-but-unused surface during Phase 3.2b have been
//! retired alongside the wiring.
//!
//! ## Layers
//!
//! - [`command`]: the [`PlaybackCommand`] enum (the things the
//!   warden tells the supervisor to do) and the
//!   [`PlaybackError`] hierarchy classifying supervisor failures
//!   for the warden to map onto `PluginError` variants.
//! - [`report`]: the `PlaybackStateReport` struct emitted on
//!   every state transition, plus its hand-rolled TOML serialiser
//!   (no `toml` or `serde` dependency in the critical path).
//!   Internal to the module; not re-exported.
//! - [`actor`]: [`SupervisorHandle`] and [`spawn`]. Two tokio
//!   tasks communicate via channels to serve custody commands and
//!   emit state reports; reconnection with bounded exponential
//!   backoff is handled transparently.
//! - `test_mock` (cfg(test) only): shared fixtures used by both
//!   this module's tests and the warden's integration tests.

mod actor;
mod command;
mod report;

#[cfg(test)]
pub(crate) mod test_mock;

// Public-within-crate surface. `lib.rs` in Phase 3.2c consumes
// these from `crate::playback_supervisor::{...}`. `report` types
// are not re-exported because they are internal helpers used only
// inside the module graph.
pub(crate) use actor::{spawn, SupervisorHandle};
pub(crate) use command::{PlaybackCommand, PlaybackError};
