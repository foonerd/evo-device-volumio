# evo-device-volumio

> The first distribution of [evo-core](https://github.com/foonerd/evo-core). An audio player built as a fabric of independently versioned pieces, not a monolith.

Stock the plugins. Sign the pieces. The device composes.

A typo in an ALSA parameter is a one-line config edit, not a redeploy. A bug in playback is a one-plugin rebuild, not a firmware flash. A core bump is a deliberate act, not a surprise. This repository is what makes that possible for the Volumio-branded audio domain, and - because the evo fabric is domain-neutral - it is also a worked example for every evo distribution that comes after it.

## How it fits together

```mermaid
flowchart LR
    core["<b>evo-core</b><br/><i>framework, upstream</i><br/>source + tags (v0.1.9)"]
    src["<b>evo-device-volumio</b><br/><i>this repo</i><br/>catalogue + plugins + branding"]
    art["<b>evo-device-volumio-artefacts</b><br/><i>release plane</i><br/>manifest + signed bytes"]
    dev["<b>Device</b><br/><i>Raspberry Pi</i><br/>Pi OS Lite aarch64"]

    core ==>|pinned by tag| src
    src ==>|cross-compile, sign, publish| art
    art ==>|manifest-driven fetch| dev
```

Three repositories, one flow. `evo-core` ships source and tags only. This distribution pins a tag, cross-compiles the steward and its own plugins, signs every piece with the vendor's key, and publishes to a separate artefacts repository. Devices fetch what the manifest names, on the channel they track. Nothing else crosses the boundary.

## What the device does

Fifteen racks across three concerns. Full charters, kinds, and the mapping of Volumio's existing assets to each rack's role live in [volumio-evo-concept.md](volumio-evo-concept.md) sections 3 and 6.

-   **Domain** - what the product does. `audio`, `audio_sources`, `audio_processing`, `networking`, `storage`, `library`, `artwork`, `metadata`, `branding`, `kiosk`.
-   **Coordination** - when and why it acts. `appointments`, `watches`.
-   **Infrastructure** - how the fabric runs over time. `observability`, `identity`, `lifecycle`.

Each rack holds shelves; plugins stock slots in the shelves; the steward composes the lot. No plugin ever addresses another plugin. Adding a streaming service, a new DAC driver, or a fresh metadata provider is stocking an existing shelf with a new plugin; the rack list does not change.

## How a change reaches a device

```mermaid
sequenceDiagram
    autonumber
    participant Dev as Developer
    participant Src as Source repo
    participant CI as Workflow
    participant Art as Artefacts repo
    participant Pi as Device

    Dev->>Src: commit plugin fix
    Src->>CI: trigger continuous-dev
    CI->>Art: sign and publish to dev
    Pi->>Art: CHECK fetch manifest
    Art-->>Pi: new version on dev
    Pi-->>Dev: OFFER with changelog
    Dev->>Pi: confirm
    Pi->>Art: fetch signed piece
    Pi->>Pi: verify and APPLY
```

One piece replaced. Every other piece - steward, catalogue, other plugins, branding, trust material - untouched. Promoting this version from `dev` to `test` later is a manifest-pointer move, not a rebuild; the bytes on `test` are bit-identical to the bytes already on `dev`.

## Documentation

| If you are... | Read |
|---|---|
| **New to this repository** | [SHOWCASE.md](SHOWCASE.md) - the distribution-process showcase. Why three repos, how pieces flow, what channels are, how a future `evo-device-<brand>` follows the pattern. |
| **Bringing up a Pi from blank Pi OS Lite** | [BUILD.md](BUILD.md) - the step-by-step runbook. Workstation prerequisites, build procedure, first install, update flows, promotion, verification. |
| **Working on the source tree** | [DEVELOPING.md](DEVELOPING.md) - workspace conventions, build and test commands, pin-upgrade procedure. |
| **Learning the domain** | [volumio-evo-concept.md](volumio-evo-concept.md) - the full rack list, plugin mapping, fabric vocabulary specific to Volumio. |
| **Looking at the framework** | [evo-core](https://github.com/foonerd/evo-core) - upstream framework docs (CONCEPT, BOUNDARY, CATALOGUE, SCHEMAS, PLUGIN_AUTHORING, and more). |

## Status

Foundation complete. The three brand-neutral plugins that lived here through Milestones 3-5 (MPD playback warden, local file-tag metadata respondent, local album-artwork respondent) migrated to [evo-plugins-audio](https://github.com/foonerd/evo-plugins-audio) and are admitted by this distribution under the `org.evoframework.*` namespace. None encoded Volumio-specific behaviour; lifting them to the commons tier is their right home.

**Landed**

-   Milestone 0 - distribution-process showcase ([SHOWCASE.md](SHOWCASE.md)).
-   Milestone 1 - repository scaffolding (Cargo workspace, licence, docs, placeholder directories).
-   Milestone 2 - `catalogue/volumio.toml` declaring 15 racks, 26 shelves, and the track-album relation predicates.
-   Plugin-tier migration - brand-neutral plugins moved to [evo-plugins-audio](https://github.com/foonerd/evo-plugins-audio); the commons signing key is bundled in `keys/`.
-   [BUILD.md](BUILD.md) - executable runbook, companion to SHOWCASE.
-   `scripts/` - automation skeleton. `bootstrap.sh` (skeleton that completes as later milestones land), `reset.sh` (fully working today), workstation `Makefile` for cross-compiles.

**Consumed from evo-plugins-audio**

| Plugin | Slot | Source |
|--------|------|--------|
| `org.evoframework.playback.mpd` | `audio.playback` | [evo-plugins-audio](https://github.com/foonerd/evo-plugins-audio) |
| `org.evoframework.metadata.local` | `metadata.providers` | [evo-plugins-audio](https://github.com/foonerd/evo-plugins-audio) |
| `org.evoframework.artwork.local` | `artwork.providers` | [evo-plugins-audio](https://github.com/foonerd/evo-plugins-audio) |

**Next**

The first genuinely Volumio-specific plugin lands in this repository. Current candidate: a metadata plugin that integrates with Volumio’s existing metadata pipeline.

`evo-core` is pinned at tag `v0.1.9` via `[workspace.dependencies]` in `Cargo.toml`. Bumps are deliberate; see [DEVELOPING.md](DEVELOPING.md) for the procedure.

## For distributions that follow

`evo-device-bmw-alpine-900`, `evo-device-acme-player`, whichever distribution comes next reads this repository as a worked example. The pattern is the same everywhere:

-   A source repo named `evo-device-<brand>`, an artefacts repo named `evo-device-<brand>-artefacts`, both owned by the same vendor.
-   The framework pinned by tag at the distribution's discretion.
-   Every piece signed with the vendor's key.
-   Devices fetch what the manifest names, on the channel they track.

[SHOWCASE.md](SHOWCASE.md) is written specifically so the Volumio text reads as a generic pattern once brand-specific nouns are substituted. If a section breaks that test, it belongs somewhere else.

## Related

-   [foonerd/evo-core](https://github.com/foonerd/evo-core) - the framework.
-   [foonerd/evo-plugins-audio](https://github.com/foonerd/evo-plugins-audio) - brand-neutral audio plugin commons; consumed by this distribution.
-   [foonerd/evo-device-volumio-artefacts](https://github.com/foonerd/evo-device-volumio-artefacts) - the release plane for this distribution.

## License

Apache 2.0. See [LICENSE](LICENSE).
