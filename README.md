# evo-device-volumio

The first distribution of [evo-core](https://github.com/foonerd/evo-core): a brand-neutral steward fabric for appliance-class devices.

This repository is Volumio's device-facing concerns - catalogue, plugin set, branding, frontend integration, packaging - layered on top of evo-core's steward. It is both the first production evo distribution and a concept-showcase reference implementation for distributions that follow.

## What is this

Evo-core is the framework: a single long-running process that administers a declared catalogue, admits plugins that stock its slots, reconciles subject identities, and emits projections and happenings to any consumer that looks. Evo-core knows nothing about audio, networking, services, or hardware.

This repository is the distribution: everything that names an actual service, protocol, or piece of hardware. MPD playback. ALSA output. Album-art providers. Metadata providers. NetworkManager integration. NAS mount orchestration. Samba sharing. Kiosk surface. Boot branding. Alarm clocks. All of it ships here as plugins that stock slots declared by a catalogue that also ships here.

The split is load-bearing. See [evo-core BOUNDARY.md](https://github.com/foonerd/evo-core/blob/main/docs/engineering/BOUNDARY.md) for why, and [volumio-evo-concept.md](volumio-evo-concept.md) for the Volumio-specific rack list and plugin mapping.

## Status

Early. Completed milestones:

-   Milestone 0: `SHOWCASE.md` - the distribution-process showcase. Prerequisites, responsibilities, repositories, workflows, channels, trust, POC path. Written to be readable by future `evo-device-<brand>` distributions as a worked example.
-   Milestone 1: repository scaffolding. Workspace manifest, licence, developing guide, build guide, empty `catalogue/` and `plugins/` directories.

Upcoming milestones:

-   Milestone 2: `catalogue/volumio.toml` declaring the racks, shelves, and relation predicates from the concept document.
-   Milestone 3: the MPD playback warden (`com.volumio.playback.mpd`), stocking `audio.playback`.
-   Milestone 4: the album-art respondent (`com.volumio.artwork.local`), stocking `artwork.providers`.
-   Later: the remaining plugins per the concept document's mapping table, packaging for Debian Trixie, branding, frontend and kiosk integration, cross-architecture CI.

## Layout

-   `Cargo.toml` - workspace manifest, evo-core pin, shared dependencies.
-   `catalogue/` - the Volumio catalogue TOML. Populated at Milestone 2.
-   `plugins/` - plugin crates. Populated from Milestone 3 onward; will be added to the workspace members array as each lands.
-   `SHOWCASE.md` - the distribution-process showcase. The WHAT and WHY at architecture level; readable by this distribution's engineering and by future `evo-device-<brand>` authors.
-   `BUILD.md` - the executable runbook. Step-by-step procedure for taking source and turning it into a running device. The HOW end to end.
-   `DEVELOPING.md` - contributor workflow. How to work on the source code day-to-day.
-   `LICENSE` - Apache 2.0.

## Relationship to evo-core

Evo-core is pinned at tag `v0.1.7` via a git dependency in `[workspace.dependencies]`. The pin is deliberate and bumped with intent when an evo-core release justifies it. See evo-core's `docs/engineering/BOUNDARY.md` section 8 for the pinning contract and this repo's `DEVELOPING.md` for the bump procedure.

Nothing in this repository modifies evo-core. Framework changes go upstream to `foonerd/evo-core`; distribution changes live here. If a proposed change here seems to require touching evo-core, re-read `BOUNDARY.md` first - the answer is usually a plugin.

## License

Apache 2.0. See [LICENSE](LICENSE).
