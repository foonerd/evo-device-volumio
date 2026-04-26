# SHOWCASE

`evo-device-volumio` is the first distribution built on the evo framework. This document has two audiences.

The first is this distribution's own engineering: the people writing the catalogue and the plugins in this repository. The second is any future `evo-device-<brand>` that comes after it. This document exists primarily for the second audience. Its purpose is to show, in enough concrete detail to be reproducible, how a vendor builds an evo distribution from zero.

Everywhere this document names "Volumio", a future distribution reads "their own brand". Everywhere it names "Raspberry Pi", a future distribution reads "their own target hardware". The pattern survives the substitution; the instance illustrates it. If a section only makes sense for Volumio, it belongs elsewhere. If it makes sense for any distribution, it belongs here.

## 1. Prerequisites

Six conditions must be true before work on any `evo-device-<brand>` distribution begins.

1.  **The framework exists and is pinnable.** A tagged release of `evo-core` the distribution can depend on. For this distribution: evo-core v0.1.9.
2.  **The domain is stated.** A concept document in the distribution's own voice, naming what the device does in catalogue vocabulary (racks, shelves, relation predicates). Everything else flows from it. For this distribution: `volumio-evo-concept.md`.
3.  **The target is chosen.** Hardware platform and operating system. For this distribution: Raspberry Pi, Raspberry Pi OS Lite, `aarch64`.
4.  **The minimum is defined.** The one-sentence behaviour that makes the device a Proof of Concept rather than a scaffolded shell. For this distribution: "plays an audio file from local storage to the configured ALSA output, with metadata and artwork visible to any consumer that asks".
5.  **Build capability exists.** A workstation that can cross-compile to the target architecture. Standard Rust cross-compilation; not designed, assumed. The device never builds its own software.
6.  **Access to the target exists.** SSH over a network, or equivalent. For this distribution: SSH on stock Pi OS Lite.

## 2. Who is responsible for what

The distribution vendor owns the supply chain from source to device. Specifically:

-   `evo-core` ships source and tags. Never binaries.
-   The vendor clones `evo-core` at the pinned tag, builds the steward together with its own plugins and catalogue, signs the results with its own key, and publishes to its own location.
-   Every artefact a device trusts is signed by the vendor. Operator trust is placed in the vendor. The framework does not sign for devices.
-   The vendor owns the artefacts' lifecycle: build, sign, publish, promote between channels, revoke when necessary.

A consequence worth stating plainly: the framework's release cadence does not drive the distribution's release cadence. The distribution bumps its `evo-core` pin when it chooses.

## 3. Three repositories

Three repositories are in scope for this distribution. Each has one job.

### 3.1 `evo-core` (upstream)

-   Role: the framework. Source and engineering docs. Tagged releases.
-   Location: `github.com/foonerd/evo-core`, pinned at `v0.1.9`.
-   What the distribution does with it: clones at the pinned tag during builds. Never modifies.

### 3.2 `evo-device-volumio` (this repository)

-   Role: distribution source. Everything a human writes and edits for this distribution.
-   Location: `github.com/foonerd/evo-device-volumio`.
-   Contents: Rust source for plugins, catalogue TOML, branding assets, trust material public halves, documentation, and the build and release workflow files.
-   What lives elsewhere: no compiled binaries, no release artefacts, no copies of upstream sources.

### 3.3 `evo-device-volumio-artefacts` (the vendor's release plane)

-   Role: the artefacts plane. Every byte a device ever pulls lives here.
-   Location: `github.com/foonerd/evo-device-volumio-artefacts`.
-   Contents: compiled binaries, manifests, signatures, per-version metadata. Structured by channel (`dev`, `test`, `prod`) and by piece.
-   Why separate: release timing is independent of development. Editing documentation in the source repo does not touch release assets. The device-facing surface is cleanly versioned and does not churn with source edits.

### 3.4 The pattern for future distributions

One source repo `evo-device-<brand>`, one artefacts repo `evo-device-<brand>-artefacts`, both owned by the same actor.

## 4. The distribution as a set of pieces

A distribution is not one thing. It is a set of pieces, each independently versioned, each stocking a named place in the fabric.

-   **The steward binary.** Built from `evo-core` at the pinned tag. One per supported architecture.
-   **The catalogue.** A TOML file declaring racks, shelves, and relation predicates.
-   **Plugin binaries.** One per plugin; each has its own manifest, its own version, its own signature.
-   **Branding assets.** Name, boot splash, colours, icon.
-   **Trust material (public halves).** The vendor's public signing keys.
-   **Systemd unit and filesystem glue.** Places the pieces into the evo footprint on a device.

The on-device footprint is fixed: `/opt/evo` for package-owned content, `/etc/evo` for operator-editable policy, `/var/lib/evo` for runtime state.

## 5. Piece-granular deployment

The core invariant of this distribution, and of any `evo-device-<brand>` that takes this pattern.

A change travels at the granularity of the piece it affects. A typo in one plugin's ALSA parameter string is fixed by replacing that plugin's config file under `/etc/evo/plugins.d/` (zero rebuild, zero transfer). A bug in one plugin is fixed by replacing that plugin's binary (rebuild one crate, move a few megabytes, drop into place). A catalogue revision replaces one TOML file. A steward bump replaces one binary.

The opposite posture - shipping everything together every time - is explicitly rejected. It contradicts the fabric's composition and wastes bandwidth, build time, and operator attention.

Three layers of change exist, with different typical frequencies:

-   **Configuration.** Per-plugin TOML under `/etc/evo/plugins.d/<plugin>.toml`. Hardware strings (for example ALSA device names), paths, endpoints, credentials. Frequent.
-   **Plugin artefact.** One plugin directory under `/opt/evo/plugins/<n>/` (manifest and binary). Common.
-   **Distribution-wide.** Catalogue revision, steward version bump, coordinated multi-plugin shape-version migration. Rare.

Configuration is data, not code. A plugin takes hardware strings, paths, and credentials as configuration handed to it at load time; it does not bake them into its binary. This separation is what makes the "fix one string" case cost one config edit and not one rebuild.

## 6. Three planes

Three planes, describing roles rather than storage locations.

-   **Source plane.** `evo-core` and `evo-device-volumio` as repositories. Consumed by developers.
-   **Artefact plane.** Compiled bytes plus manifests and signatures. Consumed by devices.
-   **Operational plane.** Running devices, consuming artefacts over time.

A device never reads source. A developer never hand-edits an artefact. Anything that blurs the line (on-device builds, source cloned to the device, hand-rolled binaries on the artefact plane) violates the shape.

## 7. Three workflows

Three workflows drive the vendor's release activity. Each has one purpose. All live in `evo-device-volumio/.github/workflows/` in this repo. All run on GitHub's runners. All produce output into `evo-device-volumio-artefacts`, signed with the vendor's key.

### 7.1 Continuous dev

-   Trigger: commits to the source repo that touch code or catalogue (plugin sources, catalogue files, Cargo manifests). Documentation-only commits do not trigger a release.
-   Action: cross-compile, sign, publish to the `dev` channel.
-   Automated. No human past the commit.
-   Purpose: every change reaches a dev device in minutes. Default iteration loop for plugin work.

### 7.2 Manual build

-   Trigger: manual workflow dispatch. Inputs: git reference to build, destination channel (`dev`, `test`, or `prod`).
-   Action: cross-compile from that reference, sign, publish to the chosen channel.
-   Human-initiated. Used for deliberate builds outside the iteration flow: hotfixes, first `prod` build of a given version, rebuilds of older tags.

### 7.3 Promotion

-   Trigger: manual workflow dispatch. Inputs: piece, version, destination channel.
-   Action: edit the manifest on the artefacts repo so the destination channel's pointer targets that version. Re-sign the manifest. **No rebuild.**
-   The critical invariant: the bytes of version X are bit-identical on every channel that names X. Only pointers move. A `test`-channel regression is a one-line revert.

The workflow YAML files are themselves showcase material. A future distribution's engineer reads them to understand how to build one of their own.

## 8. Channels

Three channels: `dev`, `test`, `prod`.

A channel is a named track of release readiness. A version of a piece is built once, signed once, stored once. As it earns trust, it is promoted: its pointer appears on a higher channel. The bytes do not change; the pointers do.

Channel selection is per-piece, per-device. The operator's device has a map: for each piece the device tracks, which channel's pointer it follows. The default map for a POC device puts everything on `dev`. A production deployment puts most pieces on `prod`, allows specific plugins on `test` or `dev` while work is in progress, and tracks the map as first-class device state.

"Always latest" means "follow the channel's current pointer for that piece". The same meaning in every channel. Fast iteration on `dev` is a consequence of how often `dev`'s pointer moves, not of a separate policy.

## 9. Dependencies

Two populations of runtime dependencies.

-   **Population A: Debian packages.** Everything the plugins shell out to or link against that Debian already ships. Examples: `mpd`, ALSA tools, NetworkManager, `cifs-utils`. Each plugin declares what it needs in its own manifest. A device's installation step aggregates declarations across the installed plugin set and asks `apt` for the union, with `--no-install-recommends` to avoid littering. No distribution-level global dependency list.
-   **Population B: evo code.** The steward, the plugins, the catalogue, branding, trust material. Produced by the vendor's build. Shipped through the artefacts repo. Placed into the evo footprint.

Population A is Debian's problem; we pull from Debian's mirrors that already exist on a stock Pi OS install. Population B is the vendor's problem; we handle it end-to-end.

This distinction preserves the "layer atop stock Debian" commitment. We do not repackage `mpd`. We do not fork ALSA. We depend on them.

## 10. Trust

Operator trust is placed in the vendor. The vendor signs every artefact. The device verifies against the vendor's public key, which is part of the distribution's trust material and ships with the first install.

The framework does not sign for devices. An `evo-core` release tag is a source-plane event. The vendor picks up the tag, builds with it, signs the result. The device trusts the vendor's signature on the result, not `evo-core`'s absence of one.

Revocation paths exist at every level: vendor revokes a compromised version; operator revokes a vendor key locally; framework-side enrollment revocation for vendors who break the vendor contract. These are concept-level pathways; specific mechanisms land when they are needed.

Two questions deliberately left open: one vendor key for the whole distribution vs. per-piece keys; single-trigger vs. two-person promotion authorisation. Both deferred until a reason to settle them appears.

## 11. The POC path for this distribution

The seven-step pattern, with this distribution's answers:

1.  State the minimum: "plays an audio file from local storage to the configured ALSA output, with metadata and artwork visible to any consumer that asks".
2.  Declare the catalogue: racks and shelves the minimum requires. Milestone 2.
3.  Author the minimum plugin set: MPD playback warden (Milestone 3), album art respondent (Milestone 4). One plugin per slot the minimum needs filled. Further plugins arrive when further slots need filling.
4.  Build on the workstation, cross-compiled for `aarch64`.
5.  Place into the evo footprint on the target Raspberry Pi.
6.  Run and observe the minimum behaviour.
7.  Document the execution in this file and in supporting docs.

A future `evo-device-<brand>` reads this section, substitutes their minimum and their target, and has a plan.

## 12. Deferred concerns

These are real concerns, deferred because they belong to a later phase or because naming them earlier would have been roof-before-foundation.

-   The manifest schema on the artefact plane (what a device fetches to discover what is available and which version is on which channel). Lands when the first workflow writes one.
-   Manifest signing discipline (per-artefact, per-manifest, or both).
-   Freshness window length for manifests.
-   The update operation as observed by the device (`CHECK`, `OFFER`, `APPLY` phases and the local inventory the device maintains). Relevant once devices exist to update.
-   Plural sources on a device. The distribution currently has one source (its own artefacts repo). The fabric's sources concept supports more; adding is additive when a reason arises.
-   An apt repository as an additional source channel. Viable; deferred until GitHub Releases as a source works end-to-end.
-   A prepared SD-card image. Convenience over "stock Pi OS Lite plus install step". Deferred until there is a user population that cannot or should not follow the install step.
-   Continuous integration beyond the build and release workflows. Arrives when there is more than one plugin crate to coordinate.
-   Cross-architecture support beyond `aarch64`.

Each deferred item is named so the deferral is deliberate, not forgotten.

## 13. For future distributions

If you are writing `evo-device-<brand>`:

-   Pick your minimum. Write it in one sentence. Refuse to scope more into Milestone 0.
-   Author your brand's concept document parallel to `volumio-evo-concept.md`.
-   Create two repositories: `evo-device-<brand>` and `evo-device-<brand>-artefacts`. Same owner.
-   Pin `evo-core` at the current tag.
-   Copy this distribution's workflow YAML files. Substitute your names, your trust key, your channels.
-   Execute the seven steps in section 11.
-   Read this document as you hit a question. If the document did not anticipate the question, open an issue against the pattern, not a workaround in your distribution.
