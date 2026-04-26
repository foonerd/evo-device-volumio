# Developing evo-device-volumio

Contributor workflow for this repository. Companion to the Milestone 1 scaffolding.

## Related docs

-   [SHOWCASE.md](SHOWCASE.md) - the WHAT and WHY of this distribution at architecture level. Read this for the model: three repositories, three planes, piece-granular deployment, channels, trust, the POC path. Explains why the conventions below exist.
-   [BUILD.md](BUILD.md) - step-by-step runbook from blank Pi to running device. Read this when you need to build, sign, publish, install, update, or promote.
-   This document (DEVELOPING.md) - contributor workflow for day-to-day work on the source tree.

## Prerequisites

-   Rust **1.85** or newer, matching the workspace `rust-version` (same MSRV as [evo-core](https://github.com/foonerd/evo-core)).
-   A **sibling** clone of [foonerd/evo-core](https://github.com/foonerd/evo-core) next to this repository (`../evo-core`), because `[workspace.dependencies]` currently points `evo-plugin-sdk` at `../evo-core/crates/evo-plugin-sdk` (same sources as the upcoming **`v0.1.9`** tag). After `v0.1.9` is published on GitHub, replace that path with the git + tag pin (see comment in the root `Cargo.toml`) so checkouts without a local `evo-core` tree resolve from the network alone.

## Workspace conventions

Mirrors evo-core; any deviation is deliberate.

-   `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]` as workspace lints.
-   Native async traits for plugin code (`impl Future + Send + '_`), matching the SDK.
-   One pin for `evo-plugin-sdk` in `[workspace.dependencies]`. Plugin crates consume it via `evo-plugin-sdk = { workspace = true }`. There is exactly one place to change the version.
-   Shared crate metadata in `[workspace.package]`. Plugin crates set `package = { workspace = true }` and override only what they must.
-   Conventional-commit messages. Same style as evo-core.
-   Pre-1.0 versioning: patch for incremental work (including internal breaking changes), minor for public-surface breaking changes, major for milestones. Docs-only changes do not bump.
-   ASCII-only in source files and docs unless there is a concrete reason otherwise (e.g. a locale string). No smart quotes, em dashes, or other non-ASCII punctuation.

## Build and test

From the workspace root:

```
cargo build --workspace
cargo test --workspace
```

Both must be green before any version bump. The workspace is empty at Milestone 1; build and test will succeed trivially until plugin crates land in Milestone 3.

## GitHub Actions

Source under [`.github/workflows/`](.github/workflows/); see [SHOWCASE.md](SHOWCASE.md) section 7 for the three workflow roles.

-   **build** — on every `pull_request` and `push`: `cargo fmt`, `clippy` (`-D warnings`), `cargo test --workspace`, with a sibling [foonerd/evo-core](https://github.com/foonerd/evo-core) clone so the path dependency on `evo-plugin-sdk` resolves (see [`scripts/ci/setup-evo-core.sh`](scripts/ci/setup-evo-core.sh)).
-   **continuous-dev** — on `push` to `main` when code, catalogue, `ci/`, `keys/`, or build config change: same checks, then `cross build` for `aarch64-unknown-linux-gnu`, then optional `evo-plugin-tool` sign/verify. Publishing to the artefacts repository is not wired yet.
-   **manual-build** — `workflow_dispatch` with a git `ref` and a `channel` input (for logging; same publish gap as above).
-   **promote** — placeholder for channel pointer moves on the artefacts repo (no rebuild).

**Repository secret** `PLUGIN_SIGNING_KEY_PEM` (optional for green CI): PKCS#8 PEM for the **private** key that pairs with the public key in [`keys/vendor-plugin-signing-public.pem`](keys/vendor-plugin-signing-public.pem) and its [`keys/vendor-plugin-signing-public.meta.toml`](keys/vendor-plugin-signing-public.meta.toml) sidecar. When set, the continuous-dev and manual-build workflows sign and verify the out-of-process bundle in [`ci/oob-sign-smoke/`](ci/oob-sign-smoke/) only. That exercise exists because [evo-plugin-tool](https://github.com/foonerd/evo-core) `sign` / `verify` require an on-disk out-of-process artefact; the production Volumio plugins in this repository are **in-process** (`exec = "<compiled-in>"` in their manifests) and are not what `evo-plugin-tool sign` signs today.

## Plugin operator TOML (in-tree reference)

Plugins receive `LoadContext::config` from per-plugin TOML (convention: `/etc/evo/plugins.d/<plugin name>.toml` on a device). For the stock local respondents in this repo:

-   **`com.volumio.metadata.local`** — optional `[library]` with `roots` / `root` (absolute paths) so `mpd-path` and `mpd-album` resolve under your music tree; optional `[metadata] profile` = `standard` (default) or `extended` to control `metadata.query` payload size. **Full wire schema and profile field matrix:** [plugins/com.volumio.metadata.local/docs/METADATA_QUERY_V1.md](plugins/com.volumio.metadata.local/docs/METADATA_QUERY_V1.md).
-   **`com.volumio.artwork.local`** — same `[library]` idea for `artwork.resolve`; see that crate’s `manifest.toml` and `src/config.rs`.

Re-read after edit depends on the plugin manifest `lifecycle.hot_reload` and the steward; a service restart is always a safe fallback.

## Running the steward against this repo's catalogue

Once Milestone 2 lands and `catalogue/volumio.toml` exists:

1.  Build evo-core once:

    ```
    cargo build --release --manifest-path /path/to/evo-core/Cargo.toml
    ```

2.  Run it pointing at this repo's catalogue:

    ```
    /path/to/evo-core/target/release/evo \
        --catalogue ./catalogue/volumio.toml \
        --socket /tmp/evo-volumio.sock \
        --log-level info
    ```

3.  Plugin admission details for local development are in evo-core's `DEVELOPING.md` sections 5 and 6.

For development runs, expect `allow_unsigned = true` in a local `evo.toml`. Production packaging uses signed plugins under the `com.volumio.*` namespace; see evo-core `VENDOR_CONTRACT.md`.

## Boundary discipline

This repository holds everything Volumio-specific:

-   The `volumio.toml` catalogue.
-   Every plugin crate (MPD, album art, networking, storage, kiosk, metadata, branding, and so on).
-   Trust roots and signing keys for the `com.volumio.*` namespace.
-   Distribution packaging (Debian Trixie layer install/uninstall scripts).
-   Frontend and bridges, if and when a web UI or HTTP bridge is written as part of this distribution.
-   Branding assets.

It does not hold:

-   The steward.
-   The plugin SDK.
-   The catalogue loader and validator.
-   Generic engineering-layer contracts (manifest schema, wire protocol, client socket protocol).
-   Anything that names a specific service, piece of hardware, or protocol outside a plugin.

If a change here seems to require modifying evo-core, re-read evo-core `docs/engineering/BOUNDARY.md` section 5. The usual answer is "it goes in a plugin here, and the contract it speaks is already declared in evo-core". If you genuinely find an evo-core gap, open an issue on `foonerd/evo-core` rather than patching around it here.

## Upgrading the evo-core pin

1.  Verify the new evo-core tag is green (`cargo test --workspace` in evo-core).
2.  Update `[workspace.dependencies].evo-plugin-sdk` in this repo's `Cargo.toml`: bump `tag = "..."` and `version = "..."` to match.
3.  Rerun `cargo build --workspace` and `cargo test --workspace` here.
4.  Commit with a message naming the new evo-core version and any public-surface changes the bump forced.

## Git

Claude (the assistant used during development) proposes file changes. The user commits, tags, and pushes. Claude does not run git commands.

## License

Apache 2.0. Each source file carries the SPDX identifier `Apache-2.0` in its header once code lands.
