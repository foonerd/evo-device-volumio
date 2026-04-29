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

## Plugin operator TOML

Plugins receive `LoadContext::config` from per-plugin TOML (convention: `/etc/evo/plugins.d/<plugin name>.toml` on a device). The brand-neutral plugins this distribution admits (`org.evoframework.playback.mpd`, `org.evoframework.metadata.local`, `org.evoframework.artwork.local`) document their config schemas in [evo-device-audio](https://github.com/foonerd/evo-device-audio); see each plugin's `manifest.toml` prerequisites and the per-plugin docs in that repository.

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

Three repository tiers per ADR-0032 (supersedes ADR-0026):

This repository is the **vendor distribution** tier. It holds material specific to the Volumio brand:

-   The `volumio.toml` catalogue.
-   Volumio-specific plugin crates under `plugins/<full.dotted.name>/` (the first planned candidate is a Volumio-specific metadata pipeline integration).
-   Trust roots: `vendor-plugin-signing-public.pem` (`com.volumio.*` namespace) and `commons-plugin-signing-public.pem` (bundled so the catalogue can admit `org.evoframework.*` plugins from the reference generic device).
-   Distribution packaging (Debian Trixie layer install/uninstall scripts).
-   Frontend and bridges, if and when a web UI or HTTP bridge is written.
-   Branding assets.

[evo-device-audio](https://github.com/foonerd/evo-device-audio) is the **reference generic audio device** tier. It holds brand-neutral audio plugins (MPD playback, ALSA composition, file-tag metadata, local artwork, etc.) under the `org.evoframework.*` namespace, plus the device build that links them, plus (when authored) a generic audio UI. This distribution admits the plugins by name; it does not duplicate them.

[evo-core](https://github.com/foonerd/evo-core) is the **framework** tier: steward, SDK, engineering-layer contracts. This repository pins evo-core via `[workspace.dependencies]`; it does not modify the framework.

If a change here seems to require modifying evo-core, re-read evo-core's `docs/engineering/BOUNDARY.md` section 5. If a change is brand-neutral and would be useful to other audio distributions, the right home is evo-device-audio, not here. If you find a genuine evo-core gap, open an issue on `foonerd/evo-core`.

## Framework non-enforcement boundary (Volumio specifics)

evo-core enforces the portable half of the plugin manifest contract. The OS-level half — kernel sandboxing, resource limits, network/filesystem scopes — is distribution-owned per `evo-core/docs/engineering/PLUGIN_PACKAGING.md` section 2 and elaborated canonically in [evo-device-audio's `DEVELOPING.md` section "Framework non-enforcement boundary"](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#framework-non-enforcement-boundary).

This section names what *the Volumio vendor distribution applies on top of that split* — the concrete systemd, cgroup, and Debian-packaging primitives this repo's deployment tooling owns.

**Steward process hardening (steward-level, applies once):**

The steward's systemd unit derived from `evo-core/dist/systemd/evo.service.example` ships with baseline hardening for the `evo` process itself. The Volumio packaging layer applies these as-is and adds Volumio-specific tightening:

```ini
# Inherited from evo-core's example, kept as-is:
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=false       # the steward must reach /dev/snd indirectly via the audio plugin
NoNewPrivileges=true
LockPersonality=true
RestrictRealtime=true

# Volumio additions:
RuntimeDirectory=evo
RuntimeDirectoryMode=0755
StateDirectory=evo
StateDirectoryMode=0750
ReadWritePaths=/var/lib/evo /run/evo
CapabilityBoundingSet=             # empty: the steward needs no capabilities
SystemCallArchitectures=native     # refuse non-native syscalls
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
```

The steward runs as a dedicated `evo` system user (created by the Debian postinst per `dist/debian/postinst`); the user is a member of `audio` (for ALSA access by audio plugins) and no other groups.

**Per-OOP-plugin hardening (per-plugin systemd drop-in):**

Volumio applies per-plugin restrictions through systemd drop-in files at `/etc/systemd/system/evo.service.d/<plugin_name>.conf`. The framework spawns OOP plugins as direct children of the steward and they inherit the steward's namespace; the drop-in pattern lets Volumio apply additional restrictions to the steward's children when the OS supports cgroup propagation.

For Volumio's audio-plugin set the concrete mappings, derived from each plugin's manifest fields:

| Manifest field | Volumio enforcement |
| --- | --- |
| `resources.max_memory_mb` | `MemoryMax=` on the per-plugin cgroup, set to the manifest value rounded up to the nearest 4 MiB. |
| `resources.max_cpu_percent` | `CPUQuota=` on the per-plugin cgroup, expressed as a percentage. |
| `prerequisites.outbound_network = "denied"` | `RestrictAddressFamilies=AF_UNIX` on the per-plugin drop-in. |
| `prerequisites.outbound_network = "allowed"` | No address-family restriction; the plugin reaches the network through the host's normal routing. |
| `prerequisites.filesystem_scopes = ["state"]` | `ReadWritePaths=/var/lib/evo/plugins/<name>/state /var/lib/evo/plugins/<name>/credentials`; everything else read-only via `ProtectSystem=strict`. |
| `trust.class = "Sandbox"` | `PrivateNetwork=true`, `PrivateUsers=true`, `SystemCallFilter=` tightened to `@system-service ~@privileged ~@resources ~@network-io`. |
| `trust.class = "Standard"` | The default profile above (no `PrivateNetwork`, full `AF_UNIX`+`AF_INET*`). |
| `trust.class = "Privileged"` | Drop-in absent; the plugin runs with the same restrictions as the steward. |
| `trust.class = "Platform"` | Drop-in absent; same as `Privileged`. The Platform-trust plugins ship as part of the Volumio distribution and the vendor accepts the trust expansion. |

**What is NOT applied by Volumio:**

-   **AppArmor / SELinux profiles.** Debian Trixie ships AppArmor enabled but Volumio does not author per-plugin profiles in v0. A future hardening pass may add per-plugin AppArmor profiles for the most untrusted plugin classes (Sandbox); the framework's manifest does not require it.
-   **Per-plugin namespaces beyond systemd's defaults.** The systemd drop-in's `PrivateNetwork=`, `PrivateUsers=`, `PrivateMounts=` are the only namespace primitives Volumio applies. Custom `unshare`-driven namespacing is not in scope.
-   **Seccomp filters per syscall list (manifest-declared).** The manifest's `prerequisites.outbound_network` and `prerequisites.filesystem_scopes` fields are lossy abstractions over the underlying syscall surface; Volumio applies `SystemCallFilter=` per trust class as documented above, not per-plugin from the manifest.

The above list is intentionally explicit so plugin authors and operators can audit it. Items absent from the "applied" list are not silently applied somewhere else; they are not applied at all in the Volumio distribution.

**Verification:**

After installing the Volumio package on a target device, the operator can audit the applied hardening with:

```bash
systemctl show evo.service --property=ProtectSystem,ProtectHome,PrivateTmp,NoNewPrivileges,CapabilityBoundingSet,SystemCallFilter,RestrictAddressFamilies
systemd-analyze security evo.service
```

`systemd-analyze security` produces a per-directive score; the steward unit's score is documented in `BUILD.md` as part of the release acceptance criteria.

### Triage: Volumio's stance per item

The canonical inventory of items an audio distribution must triage lives in [evo-device-audio's `DEVELOPING.md` "Triage: what an audio distribution must assess"](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#triage-what-an-audio-distribution-must-assess). Item numbering in the table below matches that canonical list. Volumio's stance per item:

| # | Concern | Volumio posture |
| - | --- | --- |
| 1 | Kernel-level sandboxing | **Partial.** Per-trust-class systemd drop-ins (above) applied. AppArmor / SELinux profiles **not in v0**; future hardening pass when the most-untrusted plugin classes ship. |
| 2 | Resource limits (memory / CPU) | **Applied.** Manifest-derived `MemoryMax=` / `CPUQuota=` per per-plugin cgroup. |
| 3 | Network sandboxing | **Applied per trust class** (table above). |
| 4 | Filesystem scopes | **Applied per trust class** (table above). |
| 5 | Empty-catalogue refusal | **Not implemented.** Volumio accepts the framework's "starts anyway" default; the postinst does not refuse install of an empty catalogue. May add if a real failure mode surfaces. |
| 6 | Plugins administration operator verbs | **Wires consumer in v0.1.12**. Volumio frontend surfaces enable / disable / uninstall / purge as operator-facing controls in the device admin panel. |
| 7 | Flight mode device plugin | **Wires consumer in v0.1.12**. Concrete hardware-control plugin authored if and when Volumio expands to targets with vendor-managed Bluetooth / cellular radios. Current Volumio targets (Pi 4 / Pi 5 + USB DAC, x86 + onboard audio) expose no controllable radios under Volumio's management; OS-level network manager handles WiFi / Ethernet. |
| 8 | User Interaction Routing | **Wires consumer in v0.1.12**. Volumio frontend surfaces auth-flow prompts (Spotify, Tidal, NAS credentials) as modal dialogs. |
| 9 | Appointments rack | **Not used in v0.** No time-driven Volumio plugins; future "scheduled playback" feature would consume. |
| 10 | Watches rack | **Not used in v0.** No condition-driven Volumio plugins; future "auto-resume on network up" would consume. |
| 11 | Fast Path | **Wires consumer in v0.1.12**. Volumio frontend uses Fast Path for transport ops (volume, pause, seek) where latency budgets matter. |
| 12 | Steward Reconciliation Loop | **Wires consumer in v0.1.12**. Volumio's `composition.alsa` → `delivery.alsa` pipeline composes on the framework reconciliation surface. |
| 13 | Catalogue corruption resilience | **Inherited transparently.** Volumio packaging does not pre-seed `catalogue.lkg.toml`; the framework's three-tier fallback is sufficient. |
| 14 | CBOR codec | **Not used.** Volumio frontend (Vue.js) and bridge surfaces use JSON. |
| 15 | Hot-reload `Live` mode | **Not used.** Volumio plugins are content with `Restart` mode. |
| 16 | Happenings coalescing | **Not used in v0.** Volumio frontend handles `CustodyStateReported` bursts client-side. |
| 17 | Subject-grammar orphan migration verb | **Not surfaced.** Framework handles internally; no Volumio operator tooling consumes the verb. |
| 18 | Reload-catalogue / reload-manifest operator verbs | **Wires consumer in v0.1.12**. Volumio frontend surfaces these in the operator panel for distribution updates without full steward restart. |

This table is the source of truth for Volumio's distribution-side posture. Items 6 through 18 land in evo-core v0.1.12. As each ships and Volumio wires the consumer, the corresponding row's "Wires consumer in v0.1.12" prefix flips to "**Applied.**" in the same commit that wires the consumer.

## Upgrading the evo-core pin

1.  Verify the new evo-core tag is green (`cargo test --workspace` in evo-core).
2.  Update `[workspace.dependencies].evo-plugin-sdk` in this repo's `Cargo.toml`: bump `tag = "..."` and `version = "..."` to match.
3.  Rerun `cargo build --workspace` and `cargo test --workspace` here.
4.  Commit with a message naming the new evo-core version and any public-surface changes the bump forced.

## Git

Claude (the assistant used during development) proposes file changes. The user commits, tags, and pushes. Claude does not run git commands.

## License

Apache 2.0. Each source file carries the SPDX identifier `Apache-2.0` in its header once code lands.
