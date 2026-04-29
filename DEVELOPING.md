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
| 10 | Watches rack | **Wires consumer in v0.1.12** for audio-path-switching scenarios. Volumio ships pre-configured watches for HDMI ARC handover, Bluetooth peer connect / disconnect, 3.5mm jack insertion (where hardware exposes it via ALSA jack-detect), and USB DAC plug events. Sensor / hardware-event plugins (CEC, BT peer manager, USB enumerator) are Volumio-authored under `com.volumio.sensor.*` namespace. |
| 11 | Fast Path | **Wires consumer in v0.1.12**. Volumio frontend uses Fast Path for transport ops (volume, pause, seek) where latency budgets matter. |
| 12 | Steward Reconciliation Loop | **Wires consumer in v0.1.12**. Volumio's `composition.alsa` → `delivery.alsa` pipeline composes on the framework reconciliation surface. |
| 13 | Catalogue corruption resilience | **Inherited transparently.** Volumio packaging does not pre-seed `catalogue.lkg.toml`; the framework's three-tier fallback is sufficient. |
| 14 | CBOR codec | **Not used.** Volumio frontend (Vue.js) and bridge surfaces use JSON. |
| 15 | Hot-reload `Live` mode | **Wires consumer in v0.1.12** for in-process plugins where catalogue / operator-config reload should not drop runtime state (alarm-plugin pending alarms, library-scanner progress, metadata-cache contents). For OOP streaming-source plugins (planned Spotify, Tidal): Live mode is the schema-migration recovery path on plugin version bumps. Volumio's audio.delivery / audio.composition wardens stay on Restart — hardware-bound ALSA state is owned by Volumio's separate ALSA daemon (systemd-managed), not by the plugin code; warden-architecture-pattern preserves the audio pipeline across plugin reload without framework-side fd-passing. |
| 16 | Happenings coalescing | **Wires consumer in v0.1.12.** Volumio frontend declares per-subscription coalesce label lists for the high-rate streams it consumes (per-handle `CustodyStateReported` for the position-update meter, per-subject collapse for "now playing" updates, per-watch fire for hardware-event indicators). Volumio's sensor plugins emit through the new `Happening::PluginEvent` variant; consumers subscribing to sensor streams coalesce on payload-flattened labels (e.g., `sensor_id`). |
| 17 | Subject-grammar orphan migration verb | **Not surfaced.** Framework handles internally; no Volumio operator tooling consumes the verb. |
| 18 | Reload-catalogue / reload-manifest operator verbs | **Wires consumer in v0.1.12**. Volumio frontend surfaces these in the operator panel for distribution updates without full steward restart. |
| 19 | Time and Clock Trust | **Wires distribution in v0.1.12.** Volumio ships chrony as the NTP client (replacing systemd-timesyncd in newer Volumio releases for better drift handling and richer status surface). The Volumio Debian package's chrony config uses the public `pool.ntp.org` cluster as the default NTP source, with the operator's `client_acl.toml` allowing override. Per-target `evo.toml` declares `has_battery_rtc`: `true` for Pi 5 + battery-equipped Pi 4 with PiRTC HAT; `false` for stock Pi 3 / Pi 4 / Pi Zero (the dominant install base). Volumio's power warden (Debian-systemd-based) implements the RTC-wake callback for Pi 5; on no-RTC targets, the warden refuses appointments with `must_wake_device: true` and `wake_pre_arm_ms` below the chrony-determined sync minimum. |

This table is the source of truth for Volumio's distribution-side posture. Items 6 through 19 land in evo-core v0.1.12. As each ships and Volumio wires the consumer, the corresponding row's "Wires consumer in v0.1.12" prefix flips to "**Applied.**" in the same commit that wires the consumer.

### User Interaction Routing — Volumio specifics

The canonical statement of how plugins issue prompts and how consumer surfaces render them lives in [evo-device-audio's `DEVELOPING.md`](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#user-interaction-routing--implications-for-plugins-and-ui). Volumio's specifics:

**Volumio plugins issuing prompts:**

The Volumio frontend currently surfaces auth flows for streaming sources (Spotify, Tidal, YouTube Music when present), the network configuration wizard (WiFi SSID + password + captive-portal), the NAS mount credentials dialog, and the API-key dialogs for any third-party integration plugin. Each of these maps onto one or more prompt types from the closed vocabulary:

| Volumio user flow | Prompt types involved |
| --- | --- |
| Streaming-source OAuth (YouTube Music, Tidal) | `external_redirect` |
| Streaming-source email + password (legacy Tidal, others) | `multi_field` (text + password + confirm) with `retention_hint = until_revoked` |
| WiFi network setup | `select_with_other` → optional `select` (security type) → `password` → optional `external_redirect` (captive portal) |
| Static IP configuration | `select` (interface) → `select` (DHCP / static) → `multi_field` (IP / netmask / gateway / DNS) with re-prompt on validation failure |
| NAS mount credentials | `multi_field` (server + share + username + password) with `retention_hint = until_revoked` |
| Weather / streaming-rate-info API keys | `text` with validation hint, `retention_hint = until_revoked` |

Plugin authors targeting Volumio-specific shelves (under `com.volumio.*`) follow the canonical contract; nothing Volumio-specific changes the prompt vocabulary.

**Volumio consumer surfaces (responder capability):**

Volumio's web frontend (Vue.js) holds the `user_interaction_responder` capability by default. The `client_acl.toml` shipped with the Volumio Debian package grants it to the local Unix-socket peer running the frontend's bridge process. Other consumers (a future MQTT bridge, a CLI admin tool) connect without the responder capability and observe pending prompts as subjects via `list_subjects` / `subscribe_subject` but do not answer them.

Volumio's frontend renderer covers all ten prompt types per the canonical contract:

-   `text`, `password` — standard form fields with the existing Volumio password-strength + reveal-toggle affordances.
-   `select`, `select_with_other`, `multi_select` — Vuetify selects + free-text override.
-   `confirm` — modal confirmation dialog matching Volumio's existing destructive-action confirm pattern.
-   `multi_field` — multi-step modal grouped by `session_id`; pre-fills from `previous_answer` on re-prompt; renders `error_context` inline above the affected fields.
-   `external_redirect` — opens the URL in a new browser tab (web frontend) or embedded webview (kiosk-class Pi deployments without a browser); polls until the callback URL pattern matches; extracts the response per the prompt's `expected_response`.
-   `datetime` — Vuetify date / time picker.
-   `freeform` — fallback rendering: surfaces the `mime_type` + a generic input area; consumer logs that a vendor-specific prompt type may need extension.

The unknown-type fallback ("your client is out of date") renders for any prompt type the frontend does not recognise. Volumio frontend version pins follow the evo-core tag pin; an out-of-date frontend against a newer steward sees the fallback rather than crashing.

**Volumio plugins ON the Volumio side that initiate prompts:**

In v0 of this distribution, Volumio's own plugins (Volumio-specific metadata pipeline integration, future Volumio-specific bridge-style plugins) inherit the canonical contract from evo-device-audio. None ship in this repository today; the section above is the contract that any future Volumio-specific plugin issuing prompts honours.

**Search and other consumer-initiated queries** (browse the library, queue a track, change volume via the UI control, list available outputs) are NOT prompts. They use the standard `op = "request"` against the relevant plugin's shelf — the Volumio frontend's existing query surfaces translate to `request` ops, not `request_user_interaction`.

### Time and Clock Trust — Volumio specifics

The canonical statement of the framework / distribution split for time-trust lives in [evo-device-audio's `DEVELOPING.md`](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#time-and-clock-trust--distribution-and-plugin-implications). Volumio's specifics:

**NTP daemon and configuration:**

Volumio ships **chrony** as the NTP client. systemd-timesyncd is disabled in the Debian package postinst (it is replaced, not parallel-running). Chrony's default config uses the public `pool.ntp.org` cluster (`2.debian.pool.ntp.org`, `2.pool.ntp.org`) plus optional region-specific overrides via `/etc/evo/chrony.d/region.conf` for distributions targeting specific markets. Sync triggers: cold start, reboot, NetworkManager `connectivity-up` dispatcher hook (chrony's `online`/`offline` commands), and chrony's own periodic re-poll every 64–1024s per default tuning.

For RTC-equipped targets (Pi 5 + battery, Pi 4 + PiRTC HAT, x86 boards with CMOS RTC), chrony is configured with `rtcautotrim 30` so it writes back to the hardware RTC every 30 minutes when synced — keeping cold-start trust accurate within a few seconds.

For no-RTC targets (stock Pi 3 / Pi 4 / Pi Zero — the dominant install base today), chrony is configured with aggressive first-sync (`makestep 1.0 3` — step the clock immediately on the first 3 sync attempts after start, regardless of drift size) so the device reaches `Trusted` state as quickly as possible after boot or wake.

**`evo.toml` declarations:**

Per-target `evo.toml` (shipped via the Debian package's `/etc/evo/evo.toml.<target>` overlay):

```toml
# Pi 5
[time_trust]
has_battery_rtc = true
max_acceptable_staleness_ms = 86400000     # 24h

# Stock Pi 4 / Pi 3 / Pi Zero
[time_trust]
has_battery_rtc = false
max_acceptable_staleness_ms = 86400000
sync_minimum_ms = 30000                     # chrony reaches sync within 30s post-wake
```

**Power warden RTC integration:**

Volumio's power warden (`com.volumio.system.power`, Debian-systemd-based) implements:

-   On RTC-equipped targets: `program_rtc_wake(at: SystemTime)` writes the wake time to `/sys/class/rtc/rtc0/wakealarm` and registers the framework's wake callback with `systemctl set-property` for the suspend transition.
-   On no-RTC targets: refuses `must_wake_device: true` appointments at create time when `wake_pre_arm_ms < sync_minimum_ms` (30s default). Falls back to "stay-awake mode" for short suspend windows where RTC wake is unavailable.

**Volumio plugins requiring synced time:**

| Plugin | `requires_synced_time` | `synced_time_tolerance_ms` |
| --- | --- | --- |
| `org.evoframework.playback.mpd` (audio reference) | `false` | (n/a) |
| `org.evoframework.metadata.local` (audio reference) | `false` | (n/a) |
| `org.evoframework.artwork.local` (audio reference) | `false` | (n/a) |
| Volumio-specific Spotify integration (planned) | `true` | 5000 (5s tolerance for OAuth refresh windows) |
| Volumio-specific Tidal integration (planned) | `true` | 5000 |
| Volumio-specific multi-room sync (planned) | `true` | 100 (100ms for AirPlay-class sync) |

Audio reference plugins do not require synced time — local file playback and local metadata work regardless. Streaming-source plugins (Spotify, Tidal, similar) declare `requires_synced_time` for OAuth refresh-window correctness. Multi-room audio sync requires sub-second precision.

**Frontend rendering of trust state:**

Volumio's web frontend renders an "untrusted clock" banner across the top of the screen when `clock_trust ∈ {Untrusted, Stale}`, with a one-line reason ("device just booted; awaiting network time sync" / "device clock has not synced for over 24h"). On `Trusted` the banner disappears.

Time-stamped wire frames carry a `clock_trust` annotation the frontend uses to gray out historical entries from `Untrusted`-stamped events.

### Appointments — Volumio specifics

The canonical statement of the appointments contract lives in [evo-device-audio's `DEVELOPING.md`](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#appointments--implications-for-plugins-and-ui). Volumio's specifics:

**Alarm clock plugin home:**

The alarm-clock plugin is brand-neutral and lives in the audio reference (`org.evoframework.alarm` in `evo-device-audio`). Volumio inherits it unchanged via the catalogue admission. No Volumio-specific alarm-clock plugin needed; the canonical implementation handles the multi-period day schedule (morning, midday, workout, podcast, evening, prep-for-sleep) any audio distribution operator wants.

**Volumio-specific alarm-plugin operator config:**

Volumio's Debian package ships a default `/etc/evo/plugins.d/org.evoframework.alarm.toml` with no alarms (empty `[[alarms]]` array). Volumio's web frontend writes the operator's chosen alarms to this file on save; the alarm plugin's `reload_plugin` admission verb picks up the changes without a steward restart.

The frontend's alarm-management UI renders the per-day schedule the user describes (per-day-of-week different times) as one logical "morning alarm" with a per-day editor, but stores it as multiple TOML entries — one per distinct fire time. The frontend reads back the multiple entries on next render and reconstructs the logical view.

**Calendar integration (future):**

A Volumio-specific Google Calendar / Outlook bridge plugin is on the v0.1.13+ roadmap — not in v0 of this distribution. The canonical bridge pattern (audio reference's documentation) is the implementation guide when the integration ships.

**Power warden integration:**

The user's day-schedule pattern (sleep at 23:00, wake at 06:30 via RTC) routes through the alarm plugin → `system.power.suspend` (for sleep) and the framework's RTC-wake programming (for the 06:30 wake). Volumio's power warden owns this end-to-end on the distribution side; the alarm plugin just dispatches the standard `request` ops.

**Operator-config schema vendor extensions:**

Volumio reserves the namespace `[[alarms.volumio]]` for Volumio-specific alarm fields the canonical schema doesn't cover (e.g., custom Volumio sound-preset references, Volumio-frontend-rendering hints). The audio reference's alarm plugin ignores fields outside the canonical schema; Volumio-specific extensions are read by Volumio-specific tooling only.

### Watches — Volumio specifics

The canonical statement of the watches contract lives in [evo-device-audio's `DEVELOPING.md`](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#watches--implications-for-plugins-and-ui). Volumio's specifics — particularly the audio-path-switching scenarios Volumio ships pre-configured:

**Audio-path-switching watches Volumio ships out-of-box:**

| Scenario | Volumio sensor / event plugin | Watch shape | Action |
| --- | --- | --- | --- |
| HDMI ARC active | `com.volumio.sensor.cec` (per-target Pi 5 / x86 with HDMI; uses libcec or kernel CEC driver) | Edge `SubjectState` on the ARC port subject | Switch `audio.delivery` output to the ARC port |
| HDMI ARC inactive | Same plugin | Edge `SubjectState` transition out | Revert to default output |
| Bluetooth headphones connect | `com.volumio.sensor.bt_peer` (BlueZ-based on Linux targets) | Edge `SubjectState` on the BT peer subject | Switch output to the BT peer |
| Bluetooth peer disconnect | Same plugin | Edge `SubjectState` transition out | Revert to previous output |
| 3.5mm headphone jack insertion (where hardware supports ALSA jack-detect) | `com.volumio.sensor.alsa_jack` (per-target; some Pi HATs expose this) | `HappeningMatch` on jack-insertion variant | Switch output to headphone-3.5mm; mute internal amp |
| USB DAC plugged in | `com.volumio.sensor.usb_audio_enumerator` (factory plugin admitting one instance subject per DAC under the `evo-factory-instance` addressing scheme) | Watch on subject creation events for `evo-factory-instance` of `usb-dac-*` | Switch output to the new DAC |
| USB DAC unplugged | Same plugin (subject retract on disconnect) | Watch on subject retract events | Revert to previous output |

These watches are created at the `org.evoframework.alarm`-style level (Volumio's audio-path-management plugin reads operator config and creates the watches at admit time); operators can override the auto-switch behaviour via the Volumio frontend (e.g., "always use HDMI when active" vs "let me confirm before switching").

**Volumio-authored sensor and hardware-event plugins:**

| Plugin | Trust class | Hardware coverage |
| --- | --- | --- |
| `com.volumio.sensor.cec` | Privileged (CEC needs `/dev/cec0` access) | Pi 5 / x86 with HDMI |
| `com.volumio.sensor.bt_peer` | Privileged (BlueZ D-Bus) | All Volumio targets with BT |
| `com.volumio.sensor.alsa_jack` | Standard (ALSA jack-detect via /proc) | Per-HAT support; not all Volumio targets |
| `com.volumio.sensor.usb_audio_enumerator` | Privileged (udev events) | All Volumio targets |
| `com.volumio.sensor.cpu_temp` | Sandbox (read-only `/sys/class/thermal/`) | All Volumio targets |
| `com.volumio.sensor.network_state` | Standard (NetworkManager D-Bus) | All Volumio targets |

Each plugin is signed under the Volumio vendor key; admitted on `producer.sensor.*` or `producer.hardware.*` shelves the Volumio catalogue declares; emits structured happenings on the bus that audio-path watches subscribe to.

**Volumio frontend integration:**

The Volumio web frontend surfaces a "Watches" panel in the operator settings showing every active watch — both system-created (audio-path switching) and operator-created (custom condition-driven flows). The panel uses the `list_watches` and `subscribe_subject` ops; capability-negotiated `watches_admin` for the operator-tooling user.

For sensor plugins emitting state-change happenings, the frontend's "Devices" panel reflects the live state so operators see what hardware is connected (BT peers, USB DACs, ARC-active TVs) without leaving the Volumio UI.

**Watches NOT shipped in v0:**

-   Auto-pause when motion sensor reports "no motion for 30 minutes" — not in v0; future addition with operator opt-in.
-   Auto-throttle CPU on overheat — `com.volumio.sensor.cpu_temp` ships in v0 emitting readings; the throttle action is a future addition (the `system.power.cpu_throttle` action target needs implementation per the power warden's roadmap).
-   Auto-mount NAS on network-up — Volumio's existing NAS-mount logic is operator-driven; future automation could use a watch on `com.volumio.sensor.network_state`.

Composition with appointments: Volumio scenarios that combine time + condition (e.g., "set evening unwind playlist when network is up between 21:00 and 23:00") use both primitives — an appointment fires at 21:00 to issue a check; if network is up, dispatch the playlist; if not, set a watch on network-up that expires at 23:00. Volumio's audio-path-management plugin orchestrates these compositions; framework provides the primitives.

### Happenings coalescing — Volumio specifics

The canonical statement of the coalescing contract lives in [evo-device-audio's `DEVELOPING.md`](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#happenings-coalescing--implications-for-plugins-and-consumer-surfaces). Volumio's specifics:

**Volumio frontend coalesce subscriptions:**

The Volumio web frontend (Vue.js) opens multiple subscriptions to `subscribe_happenings` with different coalesce label lists, one per UI surface that consumes high-rate streams:

| UI surface | Filter | Coalesce labels | Window | Selection |
| --- | --- | --- | --- | --- |
| Now-playing position meter | `variants: ["custody_state_reported"]`, `plugins: ["org.evoframework.playback.mpd"]` | `["variant", "plugin", "shelf", "handle_id"]` | 100 ms | latest |
| Now-playing metadata + artwork composite | (any variant) | `["primary_subject_id"]` | 200 ms | latest |
| Volume / mute indicator | `variants: ["custody_state_reported"]`, `shelves: ["audio.volume"]` | `["variant", "plugin", "shelf"]` | 50 ms | latest |
| Audio-path-switching watch fires | `variants: ["watch_fired"]` | `["variant", "watch_id"]` | 0 (no coalesce; transitions matter) | n/a |
| CPU temp telemetry (admin panel) | `variants: ["plugin_event"]`, `plugins: ["com.volumio.sensor.cpu_temp"]` | `["variant", "plugin", "sensor_id"]` | 60000 ms | latest |
| Network state changes | `variants: ["plugin_event"]`, `plugins: ["com.volumio.sensor.network_state"]` | `["variant", "plugin", "interface"]` | 1000 ms | latest |
| Audit log / forensic stream | (any variant) | (no coalesce) | n/a | n/a |

The forensic stream is intentionally not coalesced — operators reviewing the audit trail need every event at fidelity. The other surfaces collapse their high-rate streams to UI-readable rates.

**Volumio's sensor and hardware-event plugins emit through `PluginEvent`:**

Each Volumio sensor plugin emits `Happening::PluginEvent { plugin: "com.volumio.sensor.<name>", event_type: "<name>", payload: {...}, at }`. The payload field schema is documented per plugin and stable across releases:

| Plugin | event_type | Payload field schema |
| --- | --- | --- |
| `com.volumio.sensor.cec` | `arc_state_change` | `{ port, state: "active" \| "inactive", source }` |
| `com.volumio.sensor.bt_peer` | `peer_state_change` | `{ peer_id, peer_name, state: "connected" \| "disconnected" }` |
| `com.volumio.sensor.alsa_jack` | `jack_change` | `{ jack: "headphone-3.5mm", inserted: bool }` |
| `com.volumio.sensor.usb_audio_enumerator` | (factory plugin: emits via subject announce / retract, not PluginEvent) | n/a |
| `com.volumio.sensor.cpu_temp` | `reading` | `{ sensor_id: "cpu", value_celsius, unit: "C" }` |
| `com.volumio.sensor.network_state` | `state_change` | `{ interface, state: "up" \| "down", reason }` |

These payload schemas are part of Volumio's public plugin contract; consumer surfaces (frontend, MQTT bridge) build coalesce configs against them. Volumio's plugin-author guide (separate doc, future) captures the schema versioning rule: payload fields can be added without breaking consumers; renames or removals require a coordinated frontend update.

**Volumio MQTT bridge (planned v0.1.13+):**

A future Volumio MQTT bridge plugin will translate the coalesced subscriptions to MQTT topics. Coarse-grained subscriptions (per-subject, per-handle) become low-frequency MQTT publishes; fine-grained subscriptions (forensic, per-individual-fire) stay on the Unix-socket path. The bridge declares its own coalesce configs per topic; the framework's per-subscriber coalescing means the bridge's downstream rate is decoupled from the bus's emission rate.

**`describe_capabilities` discovery flow:**

Volumio's frontend, on first connect, calls `describe_capabilities` once and caches the `coalesce_labels` map. Subsequent subscriptions validate their label lists against the cached map; typos surface as console warnings during development before reaching production. The cache is invalidated on any `wire_version` change observed in subsequent reconnects.

### Hot reload — Volumio specifics

The canonical statement of hot-reload Live mode contract lives in [evo-device-audio's `DEVELOPING.md`](https://github.com/foonerd/evo-device-audio/blob/main/DEVELOPING.md#hot-reload--live-mode-authoring). Volumio's specifics:

**Volumio plugin Live-mode posture:**

| Plugin class | Live mode opt-in | Notes |
| --- | --- | --- |
| `org.evoframework.alarm` (audio reference) | Yes (in-process) | Pending alarm state preserved across operator config reload (e.g., user adds a new alarm via the frontend; alarm plugin reloads without losing tracking of the morning alarm currently armed). |
| `org.evoframework.metadata.local` (audio reference) | Yes (in-process) | Metadata cache + in-flight scan progress preserved across catalogue reload. |
| `org.evoframework.artwork.local` (audio reference) | Yes (in-process) | Artwork cache preserved across config reload. |
| `org.evoframework.playback.mpd` (audio reference, OOP-shaped) | Restart only | Hardware-bound state (the live MPD socket connection + ALSA pipeline) is owned by the MPD daemon (Volumio's existing systemd-managed `mpd.service`), not by the plugin code. Plugin-process restart reconnects to the running MPD; ALSA state preserved by MPD's continuity. |
| `org.evoframework.composition.alsa` (audio reference, future v0.1.12+) | Restart only | Hardware-bound state owned by Volumio's ALSA daemon (separate process); plugin reload reconnects. |
| `com.volumio.streaming.spotify` (planned) | Yes (OOP, Live for schema migration) | Schema-migration recovery for OAuth refresh-token format changes between plugin versions; Live used on update specifically. Default install / update uses Restart. |
| `com.volumio.streaming.tidal` (planned) | Yes (OOP, Live for schema migration) | Same shape as Spotify. |
| `com.volumio.sensor.*` (cec, bt_peer, alsa_jack, cpu_temp, etc.) | Restart only | Sensor state is the kernel's; nothing for the plugin to hand over. Re-spawn re-reads kernel state. |
| `com.volumio.alarm` (vendor extensions over org.evoframework.alarm, future) | Yes (in-process) | Inherits the audio reference's Live-mode posture. |

**Volumio's warden-architecture-pattern for hardware-bound state:**

The audio playback flow is the canonical example. Volumio's architecture:

```text
Operator config →  com.volumio.audio.frontend (respondent, in-process)
                                  ↓
                                  ↓ admit / configure / control
                                  ↓
                        evo plugin: org.evoframework.playback.mpd (OOP)
                                  ↓
                                  ↓ MPD wire protocol (TCP socket)
                                  ↓
                        mpd.service (systemd-managed, separate from evo)
                                  ↓
                                  ↓ ALSA / output device control
                                  ↓
                                Hardware
```

The plugin code (`org.evoframework.playback.mpd`) speaks the MPD wire protocol to a separately-managed `mpd` daemon. When the plugin restarts (Restart mode), the MPD daemon keeps running with its current pipeline state intact; the plugin reconnects via TCP socket, queries current state via MPD's `status` command, resumes control. ALSA pipeline never drops; user hears no audio interruption.

This is the warden-architecture-pattern in action: the resource owner (`mpd.service`) outlives the reloadable plugin code (`org.evoframework.playback.mpd` plugin process). Live mode + framework-side state handover would not be needed even if the framework supported it; the architecture handles preservation outside the framework's hot-reload primitive.

**For the planned com.volumio.composition.alsa (v0.1.12+):**

Volumio plans a separate ALSA-management daemon (`volumio-alsa-bridge`, systemd-managed) that owns the multi-stream ALSA pipeline state. The composition plugin in evo-core speaks a per-vendor wire protocol to this bridge daemon; plugin reload (Restart) reconnects without disturbing the live pipeline. Same warden-architecture-pattern.

**`reload_plugin` invocation flow:**

Volumio's frontend admin panel surfaces a "reload plugin" affordance (operator-triggered) that issues `op = "reload_plugin"` against the steward. The wire op accepts an optional `mode` field (`restart` or `live`):

```json
{
  "op": "reload_plugin",
  "plugin": "org.evoframework.alarm",
  "mode": "live"
}
```

The frontend defaults `mode` to `live` for plugins whose manifest declares `lifecycle.hot_reload = "live"`; falls back to the manifest default otherwise. Operators can override per call (force Restart even on Live-capable plugins via UI checkbox).

## Upgrading the evo-core pin

1.  Verify the new evo-core tag is green (`cargo test --workspace` in evo-core).
2.  Update `[workspace.dependencies].evo-plugin-sdk` in this repo's `Cargo.toml`: bump `tag = "..."` and `version = "..."` to match.
3.  Rerun `cargo build --workspace` and `cargo test --workspace` here.
4.  Commit with a message naming the new evo-core version and any public-surface changes the bump forced.

## License

Apache 2.0. Each source file carries the SPDX identifier `Apache-2.0` in its header once code lands.
