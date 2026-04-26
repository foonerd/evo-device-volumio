//! # com-volumio-artwork-local
//!
//! **Milestone 4** — first respondent on this distribution, stocking the
//! `artwork.providers` shelf next to the MPD playback warden. This crate is
//! the home for local album-art resolution (embedded tags, well-known cover
//! filenames, and later refinements) over the `track` / `album` graph
//! announced by `com.volumio.playback.mpd`.
//!
//! # Request surface
//!
//! - **`artwork.resolve`**: resolve visual material for a subject. The
//!   payload/response schema will align with the stewards and projections
//!   as phases land; today the handler returns a **stub** JSON body so
//!   routing and tests can be exercised.
//!
//! # Version alignment
//!
//! [`PluginIdentity::version`], the embedded `manifest.toml` `[plugin]`
//! section, and this crate’s `CARGO_PKG_VERSION` must match; see
//! [`plugin_crate_version`].
//!
//! # Reference
//!
//! [`evo_plugin_sdk::contract::Respondent`] and
//! `docs/engineering/PLUGIN_AUTHORING.md` (singleton respondent). The
//! warden reference in-tree is `evo-core/crates/evo-example-warden/`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// `Plugin` / `Respondent` use return-position `impl Future + Send` on
// trait methods; same as com-volumio-playback-mdp.
#![allow(clippy::manual_async_fn)]

use std::future::Future;

use evo_plugin_sdk::contract::{
    BuildInfo, HealthReport, LoadContext, Plugin, PluginDescription, PluginError, PluginIdentity,
    Request, Respondent, Response, RuntimeCapabilities,
};
use evo_plugin_sdk::Manifest;

/// Embedded manifest.
pub const MANIFEST_TOML: &str = include_str!("../manifest.toml");

/// Plugin reverse-DNS name; shared with the manifest and tests.
pub const PLUGIN_NAME: &str = "com.volumio.artwork.local";

/// Request type: resolve cover / visual material for a subject.
const REQUEST_ARTWORK_RESOLVE: &str = "artwork.resolve";

/// Stub body until real resolution is implemented. UTF-8 JSON.
const STUB_RESOLVE_JSON: &str =
    r#"{"v":1,"status":"stub","detail":"Milestone 4: artwork.resolve not yet implemented"}"#;

/// Parse the embedded [`Manifest`].
pub fn manifest() -> Manifest {
    Manifest::from_toml(MANIFEST_TOML)
        .expect("com-volumio-artwork-local: embedded manifest must parse")
}

fn plugin_crate_version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("CARGO_PKG_VERSION is valid semver")
}

/// Local artwork respondent. Skeleton: admission, describe, and stub
/// `handle_request`; art logic and subject graph walks follow in later
/// work items.
pub struct ArtworkLocalPlugin {
    /// `true` after a successful [`Plugin::load`].
    loaded: bool,
    /// Count of `handle_request` invocations.
    requests_handled: u64,
}

impl ArtworkLocalPlugin {
    /// New plugin, not yet [`Plugin::load`]ed.
    pub fn new() -> Self {
        Self {
            loaded: false,
            requests_handled: 0,
        }
    }

    /// Cumulative `handle_request` invocations.
    pub fn requests_handled(&self) -> u64 {
        self.requests_handled
    }

    /// For unit tests: simulate a successful [`Plugin::load`] without
    /// building a full [`LoadContext`].
    #[cfg(test)]
    fn set_loaded_for_test(&mut self) {
        self.loaded = true;
    }
}

impl Default for ArtworkLocalPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for ArtworkLocalPlugin {
    fn describe(&self) -> impl Future<Output = PluginDescription> + Send + '_ {
        async move {
            PluginDescription {
                identity: PluginIdentity {
                    name: PLUGIN_NAME.to_string(),
                    version: plugin_crate_version(),
                    contract: 1,
                },
                runtime_capabilities: RuntimeCapabilities {
                    request_types: vec![REQUEST_ARTWORK_RESOLVE.to_string()],
                    accepts_custody: false,
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
        ctx: &'a LoadContext,
    ) -> impl Future<Output = Result<(), PluginError>> + Send + 'a {
        async move {
            tracing::info!(
                plugin = PLUGIN_NAME,
                config_keys = ctx.config.len(),
                "artwork local plugin load"
            );
            self.loaded = true;
            Ok(())
        }
    }

    fn unload(&mut self) -> impl Future<Output = Result<(), PluginError>> + Send + '_ {
        async move {
            self.loaded = false;
            Ok(())
        }
    }

    fn health_check(&self) -> impl Future<Output = HealthReport> + Send + '_ {
        async move {
            if self.loaded {
                HealthReport::healthy()
            } else {
                HealthReport::unhealthy("artwork plugin not loaded")
            }
        }
    }
}

impl Respondent for ArtworkLocalPlugin {
    fn handle_request<'a>(
        &'a mut self,
        req: &'a Request,
    ) -> impl Future<Output = Result<Response, PluginError>> + Send + 'a {
        async move {
            if !self.loaded {
                return Err(PluginError::Permanent(
                    "artwork plugin not loaded".to_string(),
                ));
            }

            if req.is_past_deadline() {
                return Err(PluginError::Transient(
                    "request deadline already expired".to_string(),
                ));
            }

            self.requests_handled += 1;

            if req.request_type == REQUEST_ARTWORK_RESOLVE {
                tracing::info!(
                    plugin = PLUGIN_NAME,
                    request_type = %req.request_type,
                    cid = req.correlation_id,
                    payload_len = req.payload.len(),
                    "artwork.resolve (stubbed)"
                );
                return Ok(Response::for_request(
                    req,
                    STUB_RESOLVE_JSON.as_bytes().to_vec(),
                ));
            }

            Err(PluginError::Permanent(format!(
                "unknown request type: {:?} (not one of: {:?})",
                req.request_type,
                [REQUEST_ARTWORK_RESOLVE]
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evo_plugin_sdk::contract::HealthStatus;
    use evo_plugin_sdk::manifest::InteractionShape;

    #[test]
    fn manifest_parses() {
        let m = manifest();
        assert_eq!(m.plugin.name, PLUGIN_NAME);
        assert_eq!(m.plugin.contract, 1);
        assert_eq!(m.kind.interaction, InteractionShape::Respondent);
        let cap = m
            .capabilities
            .respondent
            .as_ref()
            .expect("manifest must have respondent capabilities");
        assert!(cap
            .request_types
            .iter()
            .any(|s| s == REQUEST_ARTWORK_RESOLVE));
    }

    #[tokio::test]
    async fn describe_matches_embedded_manifest() {
        let p = ArtworkLocalPlugin::new();
        let d = p.describe().await;
        let m = manifest();
        assert_eq!(d.identity.name, m.plugin.name);
        assert_eq!(
            d.identity.version, m.plugin.version,
            "CARGO_PKG_VERSION / describe / manifest [plugin].version must match"
        );
        assert!(!d.runtime_capabilities.accepts_custody);
        assert_eq!(
            d.runtime_capabilities.request_types,
            vec![REQUEST_ARTWORK_RESOLVE]
        );
    }

    #[tokio::test]
    async fn health_unhealthy_before_load() {
        let p = ArtworkLocalPlugin::new();
        assert!(matches!(
            p.health_check().await.status,
            HealthStatus::Unhealthy
        ));
    }

    #[tokio::test]
    async fn handle_rejects_before_load() {
        let mut p = ArtworkLocalPlugin::new();
        let r = Request {
            request_type: REQUEST_ARTWORK_RESOLVE.to_string(),
            payload: vec![],
            correlation_id: 1,
            deadline: None,
        };
        let e = p.handle_request(&r).await.unwrap_err();
        assert!(matches!(e, PluginError::Permanent(_)));
        assert_eq!(p.requests_handled(), 0);
    }

    #[tokio::test]
    async fn handle_unknown_request_type() {
        let mut p = ArtworkLocalPlugin::new();
        p.set_loaded_for_test();
        let r = Request {
            request_type: "metadata.query".to_string(),
            payload: vec![],
            correlation_id: 2,
            deadline: None,
        };
        let e = p.handle_request(&r).await.unwrap_err();
        assert!(matches!(e, PluginError::Permanent(_)));
        assert_eq!(p.requests_handled(), 1);
    }

    #[tokio::test]
    async fn handle_resolve_stub() {
        let mut p = ArtworkLocalPlugin::new();
        p.set_loaded_for_test();
        let r = Request {
            request_type: REQUEST_ARTWORK_RESOLVE.to_string(),
            payload: b"{}".to_vec(),
            correlation_id: 99,
            deadline: None,
        };
        let out = p.handle_request(&r).await.unwrap();
        assert_eq!(out.correlation_id, 99);
        let text = String::from_utf8(out.payload).unwrap();
        assert!(text.contains("stub"), "{text}");
        assert_eq!(p.requests_handled(), 1);
    }

    #[tokio::test]
    async fn handle_past_deadline() {
        let mut p = ArtworkLocalPlugin::new();
        p.set_loaded_for_test();
        let r = Request {
            request_type: REQUEST_ARTWORK_RESOLVE.to_string(),
            payload: vec![],
            correlation_id: 3,
            deadline: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
        };
        let e = p.handle_request(&r).await.unwrap_err();
        assert!(matches!(e, PluginError::Transient(_)));
        // Deadline short-circuits before increment; keep handler accounting
        // for successful dispatch attempts only.
        assert_eq!(p.requests_handled(), 0);
    }
}
