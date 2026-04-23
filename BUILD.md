# BUILD

The executable companion to [SHOWCASE.md](SHOWCASE.md). Step-by-step procedure for taking source in this repository and turning it into a running `evo-device-volumio` on a Raspberry Pi.

`SHOWCASE.md` explains WHAT and WHY. `DEVELOPING.md` explains how to work on the source code in this repo. This document explains HOW to build and deploy end to end.

## 1. Audience and scope

Two readers:

-   This distribution's engineer bringing up a prototype or cutting a release.
-   A future `evo-device-<brand>` engineer reading this as a worked example.

Scope: workstation-side build, signing, publishing to the artefacts repo, placing bytes on a Pi, running, verifying, updating, promoting. Not in scope: plugin internals (see each plugin crate's docs), the fabric concept (see `SHOWCASE.md` and `evo-core` engineering docs), contributor workflow on the source tree (see `DEVELOPING.md`).

Sections 5-10 describe the procedure at STEADY STATE - what this looks like once the plugin set exists, workflows run, and the artefact plane is populated. Section 2 states honestly what is executable TODAY and which milestone unlocks the rest.

## 2. Today's state

As of Milestone 1 + Milestone 0 (SHOWCASE.md), the source repo is scaffolded and the conceptual foundation is documented. What is actually executable today:

-   Clone this repo, run `cargo build --workspace`. Succeeds trivially; the workspace has no members yet.
-   Clone `evo-core` at tag `v0.1.7`, cross-compile the steward binary for aarch64.

What is NOT yet executable, and which milestone unlocks it:

-   **A catalogue to validate against the steward** - Milestone 2.
-   **A first plugin to exercise the build-sign-publish flow** - Milestone 3.
-   **A second plugin to exercise multi-piece composition** - Milestone 4.
-   **Continuous-dev, manual-build, and promotion workflows under `.github/workflows/`** - authored alongside Milestone 3 when there is a first piece to publish.
-   **The artefact manifest format on the artefacts repo** - authored when the first workflow writes one.
-   **The on-device update tool (CHECK / OFFER / APPLY)** - authored when devices exist to update.
-   **Signing tool and vendor key management** - decided when the first signature is cut.
-   **The systemd unit, packaged config examples, trust-material layout** - authored when there is enough to install.

Read sections 3-12 for the shape of the full procedure. Execute only the subset section 2 names as available.

## 3. Prerequisites: workstation

A Linux or macOS machine is the easiest path. Windows works via WSL2.

Required software:

-   `git`. For fetching source repos.
-   Rust stable toolchain with the aarch64 Linux target:
    ```
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup target add aarch64-unknown-linux-gnu
    ```
-   `cross`. Handles cross-compilation via container images so the workstation does not need a full aarch64 sysroot:
    ```
    cargo install cross --locked
    ```
    `cross` requires Docker or Podman installed and running.
-   `ssh` and `scp` (or `rsync`). For placing bytes on the Pi.
-   A signing tool and the vendor's private signing key. Exact tool deferred (see section 13).

Clone the three repos to a shared parent directory:

```
mkdir -p ~/src/evo && cd ~/src/evo
git clone https://github.com/foonerd/evo-core.git
git clone https://github.com/foonerd/evo-device-volumio.git
git clone https://github.com/foonerd/evo-device-volumio-artefacts.git
```

## 4. Prerequisites: target Pi

Hardware: any Raspberry Pi with aarch64 support. Pi 4 or Pi 5 for comfort during bring-up. Pi Zero 2 W is supported; the device never builds its own software, so its modest CPU is not a blocker.

OS install, once, via Raspberry Pi Imager on your workstation:

1.  Choose device (Pi 4 / Pi 5 / Pi Zero 2 W / etc.).
2.  Choose OS: Raspberry Pi OS Lite (64-bit).
3.  Open the customisation panel:
    -   Hostname: set something identifiable (e.g. `evo-volumio-01`).
    -   Username and password.
    -   Wireless LAN credentials (if not wired).
    -   Locale, keyboard layout, timezone.
    -   Enable SSH with password or key authentication.
4.  Write to microSD, boot the Pi, wait for first-boot provisioning.
5.  From the workstation, verify SSH:
    ```
    ssh <user>@<hostname>.local
    ```

No evo-specific preparation on the Pi yet. That happens in section 6.

## 5. Build procedure (workstation)

Performed on the workstation. Inputs: `evo-core` at the pinned tag, `evo-device-volumio` source. Outputs: signed artefacts published to `evo-device-volumio-artefacts` on the chosen channel.

### 5.1 Verify the pin

The distribution's `Cargo.toml` declares `evo-plugin-sdk` via a git tag (currently `v0.1.7`). The steward binary must be built from the same tag (step 5.3). Check both match.

### 5.2 Cross-compile plugins

From the distribution's source repo:

```
cd ~/src/evo/evo-device-volumio
cross build --release --target aarch64-unknown-linux-gnu --workspace
```

Outputs land in `target/aarch64-unknown-linux-gnu/release/`. One binary per plugin crate.

### 5.3 Cross-compile the steward

From the `evo-core` clone, at the pinned tag:

```
cd ~/src/evo/evo-core
git checkout v0.1.7
cross build --release --target aarch64-unknown-linux-gnu -p evo
```

Output: `target/aarch64-unknown-linux-gnu/release/evo`.

### 5.4 Assemble the piece set

For each piece (the steward, each plugin, the catalogue, branding, public trust material), collect:

-   The binary or file.
-   Its manifest (version, shelf target for plugins, declared Debian runtime dependencies, declared hot-reload policy).
-   Its version string.
-   Its SHA-256 digest.

### 5.5 Sign

Produce a detached signature per piece using the vendor's private signing key. The signature asserts "this version was produced by the vendor and has not been tampered with". Specific tool and format deferred (see section 13 and `SHOWCASE.md` section 10).

### 5.6 Write the artefact manifest

A single file on the artefacts repo that names every piece, version, path, digest, signature, Debian dependencies, and per-channel pointers (which version is currently on `dev` / `test` / `prod`). Signed with the vendor's key. Carries a freshness timestamp.

Format deferred; see section 13.

### 5.7 Publish to the artefacts repo

Copy the signed artefacts and the signed manifest into the `evo-device-volumio-artefacts` working copy, at the path the manifest declares. Commit. Push.

### 5.8 (Alternative) Workflow-driven

The above is what a human does by hand during bring-up. At steady state the three workflows in `.github/workflows/` do it:

-   Continuous dev: automatic on code commits to the source repo. Publishes to the `dev` channel.
-   Manual build: dispatched with a git ref and destination channel. Useful for hotfixes and rebuilds.
-   Promotion: dispatched with piece, version, destination channel. Edits the manifest only; no rebuild.

See `SHOWCASE.md` section 7 for the workflow shapes.

## 6. First install on the target

Performed on the Pi, over SSH, when a full artefact set exists to install. Pre-Milestone-3 this section is unexecutable; read it for shape.

### 6.1 Install Population A (Debian runtime dependencies)

Each plugin manifest declares its Debian dependencies. At install time these are aggregated across the selected plugin set and handed to apt with `--no-install-recommends` to avoid littering.

```
sudo apt update
sudo apt install --no-install-recommends <aggregated-dep-list>
```

The exact dependency list grows milestone by milestone. At Milestone 3 (MPD playback warden): `mpd`, `alsa-utils`, and whatever else the MPD plugin declares. At Milestone 4 and beyond: union with each new plugin's declarations.

### 6.2 Create the evo filesystem footprint

Per `evo-core/docs/engineering/BOUNDARY.md` section 9:

```
sudo mkdir -p /opt/evo/bin /opt/evo/plugins /opt/evo/catalogue /opt/evo/trust
sudo mkdir -p /opt/evo/share/systemd /opt/evo/share/examples
sudo mkdir -p /etc/evo /etc/evo/plugins.d /etc/evo/trust.d
sudo mkdir -p /var/lib/evo/state /var/lib/evo/cache
```

Ownership and permissions per the distribution's service-user decision (deferred).

### 6.3 Install the vendor's public trust material

```
sudo cp <vendor-public-key> /opt/evo/trust/
```

The trust material is bundled with the distribution and is what every subsequent signature verification checks against.

### 6.4 Fetch the manifest

```
curl -o /tmp/manifest <manifest-url-on-artefacts-repo>
```

URL shape deferred (see section 13).

### 6.5 Verify the manifest signature

Against `/opt/evo/trust/`. If verification fails, stop; do not fetch artefacts.

### 6.6 Fetch and verify each piece

For each piece named in the manifest:

-   Fetch its artefact file and its signature.
-   Verify the signature against the vendor key.
-   Fail fast on any mismatch.

### 6.7 Place artefacts into the footprint

-   Steward: `/opt/evo/bin/evo`.
-   Plugins: `/opt/evo/plugins/<reverse-dns-name>/` (one directory per plugin; contains binary + manifest).
-   Catalogue: `/opt/evo/catalogue/volumio.toml`.
-   Branding: `/opt/evo/share/branding/`.

### 6.8 Seed configuration

```
sudo cp /opt/evo/share/examples/evo.toml.example /etc/evo/evo.toml
sudo cp /opt/evo/share/examples/plugins.d/*.toml /etc/evo/plugins.d/
```

Edit `/etc/evo/evo.toml` and any plugin configs for this device's specifics (hardware strings, paths, etc.).

### 6.9 Install the systemd unit

```
sudo cp /opt/evo/share/systemd/evo.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now evo
```

### 6.10 Verify

```
systemctl status evo
journalctl -u evo -n 200
ls -la /var/run/evo/
```

Expected: steward active, catalogue loaded, plugins admitted, socket bound.

## 7. Updating a configuration parameter

Canonical tiny change: an ALSA device string was wrong (`wh:0,0`), fix to `hw:0,0`. Zero rebuild. One config file edit. One reload.

### 7.1 Edit the config on the Pi

```
sudo $EDITOR /etc/evo/plugins.d/com.volumio.playback.mpd.toml
```

### 7.2 Ask the plugin to reload

Depends on the plugin's declared `hot_reload` policy in its manifest (`evo-core/docs/engineering/PLUGIN_PACKAGING.md` section 2):

-   `live`: trigger reload through the steward's client socket; the plugin re-reads its config without a restart.
-   `restart`: restart the specific plugin via the steward.
-   `none`: restart the service: `sudo systemctl restart evo`.

The exact per-plugin reload tool is deferred; today the `none` path (service restart) works for any plugin.

### 7.3 Verify

`journalctl -u evo` shows the plugin loaded the new config.

No rebuild. No transfer. No other piece touched.

## 8. Updating a single plugin

Canonical case: a bug in `com.volumio.playback.mpd`.

### 8.1 Fix on the source repo

Edit the plugin source, commit, push.

### 8.2 Continuous-dev workflow builds and publishes

On push, the continuous-dev workflow cross-compiles the affected plugin, signs it, publishes it to the `dev` channel on the artefacts repo. The manifest's `dev` pointer for this plugin moves to the new version.

Alternative: dispatch the manual-build workflow against a specific git ref and channel.

### 8.3 Apply on the target

The operator runs the update tool on the Pi (tool TBD). The tool performs the three-phase operation from `SHOWCASE.md`:

-   CHECK: fetches the current manifest, diffs against local inventory, finds the new version for this plugin.
-   OFFER: displays the diff with changelog, awaits operator confirmation.
-   APPLY: fetches the new artefact, verifies signature, places into `/opt/evo/plugins/com.volumio.playback.mpd/`, honours the plugin's `hot_reload` policy.

### 8.4 Verify

`journalctl -u evo` shows the plugin loaded the new version. Every other piece is untouched.

## 9. Updating the steward

Canonical case: `evo-core` v0.2.0 is tagged.

### 9.1 Update the pin

On the source repo, edit `Cargo.toml`:

```
evo-plugin-sdk = { git = "...", tag = "v0.2.0", version = "0.2.0" }
```

Verify plugin crates still compile against the new SDK: `cargo build --workspace`. Address any API changes.

### 9.2 Rebuild

The continuous-dev or manual-build workflow cross-compiles the new steward (from the new `evo-core` tag) and all plugins (they are recompiled against the new SDK). Publish to the chosen channel.

### 9.3 Apply on the target

Same CHECK / OFFER / APPLY flow. The diff now contains the steward plus every plugin; operator confirms the set.

### 9.4 Verify

As in section 6.10.

A steward bump is a wider update than a single plugin because every plugin is linked against the SDK version. It is still piece-granular on the artefact plane; only the diff is wider.

## 10. Promoting a version between channels

Canonical case: `com.volumio.playback.mpd` v0.3.2 has proved itself on `dev`; promote to `test`. Later, to `prod`.

### 10.1 Dispatch the promotion workflow

From the source repo's Actions tab, dispatch the promotion workflow with inputs:

```
piece       = com.volumio.playback.mpd
version     = 0.3.2
destination = test
```

### 10.2 Workflow edits the manifest

The workflow commits a manifest change to the artefacts repo: the `test` channel pointer for this plugin now names version 0.3.2. The manifest is re-signed. NO REBUILD. NO NEW ARTEFACT.

### 10.3 Devices on `test` pick up

On next CHECK, devices tracking `test` for this plugin see the new pointer and proceed through OFFER / APPLY. The bytes they fetch are bit-identical to what `dev`-tracking devices already have.

### 10.4 Rollback

Dispatch the promotion workflow again, naming the previous version. Pointer moves back. No rebuild. This is the architectural payoff of "signatures on versions, pointers on channels".

## 11. Verification checklist

Used after any install or major update:

-   [ ] `systemctl status evo` shows `active (running)`.
-   [ ] `journalctl -u evo` shows the catalogue loaded.
-   [ ] `journalctl -u evo` shows each expected plugin admitted.
-   [ ] Steward socket exists at the configured path (default `/var/run/evo/evo.sock`).
-   [ ] A probe client (see `evo-core/README.md` sixty-seconds example) can connect and receive a response.
-   [ ] Minimum behaviour demonstrable: TBD at Milestone 3 once the MPD warden lands. For now: the steward admits zero plugins and serves the empty catalogue cleanly.

## 12. Troubleshooting

Minimal initial set. Grows with real operational experience.

-   **Steward fails to start with "catalogue not found"**: check `catalogue.path` in `/etc/evo/evo.toml`; verify the file exists and is readable.
-   **A plugin fails admission**: `journalctl -u evo` names the specific `StewardError` variant. Common ones: `IdentityMismatch` (plugin name in `describe` differs from manifest), `MissingShelf` (manifest targets a shelf the catalogue does not declare).
-   **Signature verification fails**: check `/opt/evo/trust/` and `/etc/evo/trust.d/` contain the vendor public key that signed the artefact. Key rotation invalidates old signatures.
-   **`cross build` fails with Docker errors on the workstation**: confirm Docker or Podman is running. `cross` spawns a container per build.
-   **Pi OOMs during a cargo build**: the Pi is the device, not the build machine. Build on the workstation and ship artefacts.

## 13. Deferred items tracked here

Named so their absence is visible and a future change to this document can close them by pointing back here:

-   The exact signing tool, key format, and signature file layout.
-   The vendor's signing-key-management process (generation, storage, rotation).
-   The artefact manifest file format (fields, serialisation, freshness window semantics).
-   The path layout within `evo-device-volumio-artefacts` (per-channel directories, per-version filenames, manifest location).
-   The URL shape a device fetches the manifest from.
-   The on-device update tool that performs CHECK / OFFER / APPLY.
-   The per-plugin reload mechanism accessible without restarting the steward.
-   The service user for the steward and associated file ownership.
-   The full aggregated Debian dependency list (grows with each plugin milestone).
-   The systemd unit file contents.
-   Concrete verification procedure for the POC minimum (depends on Milestones 2-4).

Each item resolves in a specific later milestone. Cross-references will land in-place as they do.
