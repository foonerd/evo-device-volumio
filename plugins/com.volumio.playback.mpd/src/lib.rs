//! # com-volumio-playback-mpd
//!
//! MPD playback warden for evo-device-volumio. Stocks the
//! `audio.playback` shelf declared by Milestone 2's catalogue.
//!
//! This is the Milestone 3 Phase 3.0 deliverable: a stub singleton
//! warden that implements the [`Plugin`] and [`Warden`] trait
//! surface required for admission and custody lifecycle, with real
//! MPD connection logic deferred to later phases:
//!
//! - Phase 3.1: MPD connection layer (private module, socket and
//!   protocol plumbing, mock-backed unit tests).
//! - Phase 3.2: real custody of a live MPD instance (idle events,
//!   state reports over time, transport course corrections,
//!   reconnect-on-disconnect).
//! - Phase 3.3: configuration file (`/etc/evo/plugins.d/
//!   com.volumio.playback.mpd.toml`).
//! - Phase 3.4: subject assertion (`track` and `album` with
//!   `album_of` edges for M4's album-art respondent to walk).
//!
//! The wire-transport binary lands after the in-process flow has
//! stabilised.
//!
//! The shape of this crate mirrors the reference warden in
//! `evo-core/crates/evo-example-warden/`; deviations are confined
//! to identity (name, trust class, custody exclusivity) and to the
//! prose that references the Volumio-specific phases above.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use evo_plugin_sdk::contract::{
    Assignment, BuildInfo, CourseCorrection, CustodyHandle, HealthReport,
    HealthStatus, LoadContext, Plugin, PluginDescription, PluginError,
    PluginIdentity, RuntimeCapabilities, Warden,
};
use evo_plugin_sdk::Manifest;
use std::collections::HashMap;
use std::future::Future;

/// The plugin's embedded manifest, as a static string.
///
/// Available so callers can validate the manifest at test time or
/// admit the plugin without disk I/O.
pub const MANIFEST_TOML: &str = include_str!("../manifest.toml");

/// The plugin's canonical reverse-DNS name. Single source of truth
/// shared between the manifest and [`Plugin::describe`]; the
/// `identity_name_matches_manifest` test enforces parity.
pub const PLUGIN_NAME: &str = "com.volumio.playback.mpd";

/// Parse the embedded manifest into a [`Manifest`] struct.
///
/// Panics if the embedded manifest fails to parse. Such a failure
/// is a build-time bug, not a runtime condition, so panicking is
/// acceptable.
pub fn manifest() -> Manifest {
    Manifest::from_toml(MANIFEST_TOML)
        .expect("com-volumio-playback-mpd's embedded manifest must parse")
}

/// Per-custody state tracked by the plugin for the lifetime of the
/// custody.
///
/// Phase 3.0 stores only the custody_type. Later phases that emit
/// state reports outside `take_custody` will retain the
/// [`evo_plugin_sdk::contract::CustodyStateReporter`] supplied on
/// the [`Assignment`]; the reference warden in
/// `evo-example-warden` documents why the reporter is not retained
/// by default.
#[derive(Debug, Clone)]
struct TrackedCustody {
    custody_type: String,
}

/// MPD playback warden plugin.
///
/// Phase 3.0 scope: admission surface complete, custody bookkeeping
/// in place, no connection to a real MPD instance yet.
#[derive(Debug, Default)]
pub struct MpdPlaybackPlugin {
    loaded: bool,
    custodies: HashMap<String, TrackedCustody>,
    /// Cumulative count of custodies accepted since construction.
    /// Does not decrement on release.
    custodies_taken: u64,
    /// Cumulative count of course corrections received since
    /// construction.
    corrections_received: u64,
}

impl MpdPlaybackPlugin {
    /// Construct a new plugin instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of custodies currently held (taken but not yet
    /// released).
    pub fn active_custody_count(&self) -> usize {
        self.custodies.len()
    }

    /// Cumulative count of custodies accepted since construction.
    pub fn custodies_taken(&self) -> u64 {
        self.custodies_taken
    }

    /// Cumulative count of course corrections received since
    /// construction.
    pub fn corrections_received(&self) -> u64 {
        self.corrections_received
    }
}

impl Plugin for MpdPlaybackPlugin {
    fn describe(&self) -> impl Future<Output = PluginDescription> + Send + '_ {
        async move {
            PluginDescription {
                identity: PluginIdentity {
                    name: PLUGIN_NAME.to_string(),
                    version: semver::Version::new(0, 1, 0),
                    contract: 1,
                },
                runtime_capabilities: RuntimeCapabilities {
                    request_types: vec![],
                    accepts_custody: true,
                    flags: Default::default(),
                },
                build_info: BuildInfo {
                    plugin_build: env!("CARGO_PKG_VERSION").to_string(),
                    sdk_version: evo_plugin_sdk::VERSION.to_string(),
                    rustc_version: None,
                    built_at: None,
                },
            }
        }
    }

    fn load<'a>(
        &'a mut self,
        _ctx: &'a LoadContext,
    ) -> impl Future<Output = Result<(), PluginError>> + Send + 'a {
        async move {
            tracing::info!(plugin = PLUGIN_NAME, "plugin load");
            self.loaded = true;
            Ok(())
        }
    }

    fn unload(
        &mut self,
    ) -> impl Future<Output = Result<(), PluginError>> + Send + '_ {
        async move {
            tracing::info!(
                plugin = PLUGIN_NAME,
                active = self.custodies.len(),
                taken = self.custodies_taken,
                corrections = self.corrections_received,
                "plugin unload"
            );
            self.loaded = false;
            // Phase 3.0 does not emit final state reports; Phase 3.2
            // will, once the reporter is retained on TrackedCustody.
            self.custodies.clear();
            Ok(())
        }
    }

    fn health_check(&self) -> impl Future<Output = HealthReport> + Send + '_ {
        async move {
            if self.loaded {
                HealthReport::healthy()
            } else {
                HealthReport::unhealthy("playback plugin not loaded")
            }
        }
    }
}

impl Warden for MpdPlaybackPlugin {
    fn take_custody<'a>(
        &'a mut self,
        assignment: Assignment,
    ) -> impl Future<Output = Result<CustodyHandle, PluginError>> + Send + 'a
    {
        async move {
            if !self.loaded {
                return Err(PluginError::Permanent(
                    "playback plugin not loaded".to_string(),
                ));
            }

            // Deterministic handle id tied to the assignment's
            // correlation id so integration tests can predict it.
            let handle = CustodyHandle::new(format!(
                "custody-{}",
                assignment.correlation_id
            ));

            // Emit one initial state report before returning the
            // handle. Called from within the plugin's own trait
            // method - same task as the SDK's host dispatch loop -
            // so no cross-task reporter sharing. Matches the pattern
            // in evo-example-warden. Failure to report is not fatal
            // to the custody in Phase 3.0; Phase 3.2 may reconsider
            // when reporting is no longer a best-effort signal.
            let report = assignment
                .custody_state_reporter
                .report(
                    &handle,
                    b"state=idle".to_vec(),
                    HealthStatus::Healthy,
                )
                .await;
            if let Err(e) = report {
                tracing::warn!(
                    plugin = PLUGIN_NAME,
                    handle = %handle.id,
                    error = %e,
                    "initial state report failed; accepting custody anyway"
                );
            }

            self.custodies.insert(
                handle.id.clone(),
                TrackedCustody {
                    custody_type: assignment.custody_type.clone(),
                },
            );
            self.custodies_taken += 1;

            tracing::info!(
                plugin = PLUGIN_NAME,
                handle = %handle.id,
                custody_type = %assignment.custody_type,
                cid = assignment.correlation_id,
                "custody accepted"
            );

            Ok(handle)
        }
    }

    fn course_correct<'a>(
        &'a mut self,
        handle: &'a CustodyHandle,
        correction: CourseCorrection,
    ) -> impl Future<Output = Result<(), PluginError>> + Send + 'a {
        async move {
            if !self.loaded {
                return Err(PluginError::Permanent(
                    "playback plugin not loaded".to_string(),
                ));
            }

            if !self.custodies.contains_key(&handle.id) {
                return Err(PluginError::Permanent(format!(
                    "unknown custody handle: {}",
                    handle.id
                )));
            }

            self.corrections_received += 1;

            tracing::info!(
                plugin = PLUGIN_NAME,
                handle = %handle.id,
                correction_type = %correction.correction_type,
                cid = correction.correlation_id,
                "course correction accepted"
            );

            // Phase 3.2 will act on the correction (play, pause,
            // stop, next, prev, seek) and emit a follow-up state
            // report. Phase 3.0 acknowledges without effect.
            Ok(())
        }
    }

    fn release_custody<'a>(
        &'a mut self,
        handle: CustodyHandle,
    ) -> impl Future<Output = Result<(), PluginError>> + Send + 'a {
        async move {
            if !self.loaded {
                return Err(PluginError::Permanent(
                    "playback plugin not loaded".to_string(),
                ));
            }

            let tracked = self.custodies.remove(&handle.id).ok_or_else(
                || {
                    PluginError::Permanent(format!(
                        "unknown custody handle: {}",
                        handle.id
                    ))
                },
            )?;

            tracing::info!(
                plugin = PLUGIN_NAME,
                handle = %handle.id,
                custody_type = %tracked.custody_type,
                "custody released"
            );

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_plugin_sdk::contract::{CustodyStateReporter, ReportError};
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    /// Capturing reporter: records every `report` invocation so
    /// tests can assert on them. Returns Ok for every call. Mirrors
    /// the fixture in evo-example-warden's tests.
    #[derive(Debug, Default)]
    struct CapturingReporter {
        reports: Mutex<Vec<(CustodyHandle, Vec<u8>, HealthStatus)>>,
    }

    impl CapturingReporter {
        fn count(&self) -> usize {
            self.reports.lock().unwrap().len()
        }

        fn last(&self) -> Option<(CustodyHandle, Vec<u8>, HealthStatus)> {
            self.reports.lock().unwrap().last().cloned()
        }
    }

    impl CustodyStateReporter for CapturingReporter {
        fn report<'a>(
            &'a self,
            handle: &'a CustodyHandle,
            payload: Vec<u8>,
            health: HealthStatus,
        ) -> Pin<
            Box<dyn Future<Output = Result<(), ReportError>> + Send + 'a>,
        > {
            let handle = handle.clone();
            Box::pin(async move {
                self.reports
                    .lock()
                    .unwrap()
                    .push((handle, payload, health));
                Ok(())
            })
        }
    }

    fn assignment(
        reporter: Arc<dyn CustodyStateReporter>,
        correlation_id: u64,
    ) -> Assignment {
        Assignment {
            custody_type: "playback-session".into(),
            payload: b"track-1".to_vec(),
            correlation_id,
            deadline: None,
            custody_state_reporter: reporter,
        }
    }

    #[test]
    fn embedded_manifest_parses() {
        let m = manifest();
        assert_eq!(m.plugin.name, PLUGIN_NAME);
        assert_eq!(m.plugin.contract, 1);
        assert_eq!(
            m.kind.interaction,
            evo_plugin_sdk::manifest::InteractionShape::Warden
        );
    }

    #[tokio::test]
    async fn identity_name_matches_manifest() {
        let p = MpdPlaybackPlugin::new();
        let d = p.describe().await;
        let m = manifest();
        assert_eq!(d.identity.name, m.plugin.name);
        assert_eq!(d.identity.name, PLUGIN_NAME);
    }

    #[tokio::test]
    async fn describe_returns_expected_identity() {
        let p = MpdPlaybackPlugin::new();
        let d = p.describe().await;
        assert_eq!(d.identity.name, PLUGIN_NAME);
        assert_eq!(d.identity.contract, 1);
        assert!(d.runtime_capabilities.accepts_custody);
        assert!(d.runtime_capabilities.request_types.is_empty());
    }

    #[tokio::test]
    async fn health_is_unhealthy_before_load() {
        let p = MpdPlaybackPlugin::new();
        let r = p.health_check().await;
        assert!(matches!(r.status, HealthStatus::Unhealthy));
    }

    #[tokio::test]
    async fn load_unload_is_idempotent() {
        let mut p = MpdPlaybackPlugin::new();
        // Dummy LoadContext: the reference implementation does not
        // expose a Default impl; for unit tests we skip through the
        // load/unload code paths by relying on the fact that our
        // implementation does not read the context. If the SDK
        // starts requiring context fields, this test updates to
        // construct one.
        //
        // Two load/unload cycles should leave the plugin unhealthy
        // with no active custodies and no cumulative counters
        // affected.
        for _ in 0..2 {
            p.loaded = true; // stand-in for load without a context
            assert!(matches!(
                p.health_check().await.status,
                HealthStatus::Healthy
            ));
            p.unload().await.unwrap();
            assert!(matches!(
                p.health_check().await.status,
                HealthStatus::Unhealthy
            ));
        }
        assert_eq!(p.active_custody_count(), 0);
        assert_eq!(p.custodies_taken(), 0);
        assert_eq!(p.corrections_received(), 0);
    }

    #[tokio::test]
    async fn take_custody_rejects_before_load() {
        let mut p = MpdPlaybackPlugin::new();
        let reporter: Arc<dyn CustodyStateReporter> =
            Arc::new(CapturingReporter::default());
        let a = assignment(reporter, 1);
        let e = p.take_custody(a).await.unwrap_err();
        assert!(matches!(e, PluginError::Permanent(_)));
    }

    #[tokio::test]
    async fn take_custody_returns_handle_and_emits_initial_report() {
        let mut p = MpdPlaybackPlugin::new();
        p.loaded = true;
        let reporter = Arc::new(CapturingReporter::default());
        let reporter_dyn: Arc<dyn CustodyStateReporter> = reporter.clone();
        let a = assignment(reporter_dyn, 42);

        let handle = p.take_custody(a).await.unwrap();
        assert_eq!(handle.id, "custody-42");
        assert_eq!(p.active_custody_count(), 1);
        assert_eq!(p.custodies_taken(), 1);

        assert_eq!(reporter.count(), 1);
        let (h, payload, health) = reporter.last().unwrap();
        assert_eq!(h.id, "custody-42");
        assert_eq!(payload, b"state=idle");
        assert_eq!(health, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn course_correct_acknowledges_known_handle() {
        let mut p = MpdPlaybackPlugin::new();
        p.loaded = true;
        let reporter: Arc<dyn CustodyStateReporter> =
            Arc::new(CapturingReporter::default());
        let handle =
            p.take_custody(assignment(reporter, 7)).await.unwrap();

        let correction = CourseCorrection {
            correction_type: "play".into(),
            payload: vec![],
            correlation_id: 100,
        };
        p.course_correct(&handle, correction).await.unwrap();
        assert_eq!(p.corrections_received(), 1);
    }

    #[tokio::test]
    async fn course_correct_rejects_unknown_handle() {
        let mut p = MpdPlaybackPlugin::new();
        p.loaded = true;
        let handle = CustodyHandle::new("custody-does-not-exist");
        let correction = CourseCorrection {
            correction_type: "play".into(),
            payload: vec![],
            correlation_id: 1,
        };
        let e = p.course_correct(&handle, correction).await.unwrap_err();
        assert!(matches!(e, PluginError::Permanent(_)));
        assert_eq!(p.corrections_received(), 0);
    }

    #[tokio::test]
    async fn release_custody_removes_from_tracking() {
        let mut p = MpdPlaybackPlugin::new();
        p.loaded = true;
        let reporter: Arc<dyn CustodyStateReporter> =
            Arc::new(CapturingReporter::default());
        let handle =
            p.take_custody(assignment(reporter, 5)).await.unwrap();
        assert_eq!(p.active_custody_count(), 1);

        p.release_custody(handle).await.unwrap();
        assert_eq!(p.active_custody_count(), 0);
        // Cumulative counter is not decremented.
        assert_eq!(p.custodies_taken(), 1);
    }

    #[tokio::test]
    async fn release_custody_rejects_unknown_handle() {
        let mut p = MpdPlaybackPlugin::new();
        p.loaded = true;
        let handle = CustodyHandle::new("custody-phantom");
        let e = p.release_custody(handle).await.unwrap_err();
        assert!(matches!(e, PluginError::Permanent(_)));
    }
}
