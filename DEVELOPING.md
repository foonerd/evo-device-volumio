# Developing evo-device-volumio

Contributor workflow for this repository. Companion to the Milestone 1 scaffolding.

## Related docs

-   [SHOWCASE.md](SHOWCASE.md) - the WHAT and WHY of this distribution at architecture level. Read this for the model: three repositories, three planes, piece-granular deployment, channels, trust, the POC path. Explains why the conventions below exist.
-   [BUILD.md](BUILD.md) - step-by-step runbook from blank Pi to running device. Read this when you need to build, sign, publish, install, update, or promote.
-   This document (DEVELOPING.md) - contributor workflow for day-to-day work on the source tree.

## Prerequisites

-   Rust stable, matching the workspace `rust-version = "1.80"`.
-   Network access to github.com/foonerd for the `evo-plugin-sdk` git dependency.
-   A local checkout of [foonerd/evo-core](https://github.com/foonerd/evo-core) at tag `v0.1.7` if you want to run the steward against this repo's catalogue (see "Running the steward" below).

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
