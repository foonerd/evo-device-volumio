# scripts

Automation for evo-device-volumio. The primary path through BUILD.md runs through these scripts; the manual steps in BUILD.md sections 7 to 11 document what each script does, not what an operator types by hand during normal use.

Three families, one per audience:

-   `device/` - run on the Raspberry Pi. `bootstrap.sh` (first install), `reset.sh` (wipe), and in due course `update.sh` (CHECK / OFFER / APPLY).
-   `workstation/` - run by a developer. `Makefile` targets wrap `cross build` for the steward and the plugin set.
-   `scripts/ci/setup-evo-core.sh` — clones or updates a sibling [foonerd/evo-core](https://github.com/foonerd/evo-core) for GitHub Actions and for a local run with a fresh sibling tree.
-   `.github/workflows/` — `build`, `continuous-dev`, `manual-build`, `promote` (see `DEVELOPING.md` and `BUILD.md` section 3.3).

See BUILD.md section 3 for the full picture. Per-script headers document the scripts themselves.

## Script status today

| Script | State | Notes |
|---|---|---|
| `device/bootstrap.sh` | Skeleton | Creates the evo footprint and installs a trust key today. Fetch / verify / place phases are marked pending; land with Milestones 3+ and the artefact manifest format. |
| `device/reset.sh` | Working | Wipes `/opt/evo`, `/var/lib/evo`, and (unless `--keep-policy`) `/etc/evo`. Removes the systemd unit if present. Safe to re-run. |
| `workstation/Makefile` | Working skeleton | Targets work; `build-plugins` builds nothing until the workspace has members (Milestone 3). |

## Executable bit

On first checkout, make the shell scripts executable:

```
chmod +x scripts/device/*.sh
```

Git preserves the executable bit once committed.
