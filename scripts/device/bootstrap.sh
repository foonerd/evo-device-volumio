#!/usr/bin/env bash
#
# bootstrap.sh - bring a freshly-flashed Raspberry Pi OS Lite (aarch64)
# to a state where evo-device-volumio is ready to run.
#
# This is the automated path. The manual equivalent of each phase is
# documented in BUILD.md section 7.
#
# Phases:
#   1. Parse arguments.
#   2. Confirm root (or usable with sudo).
#   3. Install Population A (Debian packages declared by the plugin set).
#   4. Create the evo filesystem footprint (per BOUNDARY.md section 9).
#   5. Install the vendor public trust material.
#   6. Fetch the artefact manifest for the chosen channel.
#   7. Verify the manifest signature.
#   8. Fetch and verify each piece named by the manifest.
#   9. Place pieces into the footprint.
#   10. Seed configuration from the examples shipped with the piece set.
#   11. Install the systemd unit.
#   12. Enable and start the evo service.
#   13. Print status and next steps.
#
# Today's state:
#   Phase 4 runs fully (creates the footprint).
#   Phase 5 runs if --trust-key is provided.
#   Phase 3 runs apt-get update as a baseline but has no plugin deps to install yet.
#   Phases 6 through 12 are pending and print PENDING markers naming what they need.
#   Milestone 3 onward populates those phases.
#
# Idempotent: safe to re-run. Footprint directories are created with install -d;
# trust keys overwrite cleanly.

set -euo pipefail

# ---------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------

readonly DEFAULT_CHANNEL="dev"
readonly DEFAULT_MANIFEST_BASE_URL="https://github.com/foonerd/evo-device-volumio-artefacts/raw/main"
readonly EVO_OPT="/opt/evo"
readonly EVO_ETC="/etc/evo"
readonly EVO_VAR="/var/lib/evo"

# ---------------------------------------------------------------------
# Logging helpers
# ---------------------------------------------------------------------

log()     { printf '[bootstrap] %s\n' "$*"; }
warn()    { printf '[bootstrap] WARN: %s\n' "$*" >&2; }
err()     { printf '[bootstrap] ERROR: %s\n' "$*" >&2; }
pending() { printf '[bootstrap] PENDING (%s): %s\n' "$1" "$2"; }

# ---------------------------------------------------------------------
# 1. Parse arguments
# ---------------------------------------------------------------------

channel="${DEFAULT_CHANNEL}"
manifest_base_url="${DEFAULT_MANIFEST_BASE_URL}"
trust_key=""

usage() {
    cat <<'EOF'
Usage: sudo bootstrap.sh [options]

Options:
  --channel <dev|test|prod>    Channel to install from (default: dev).
  --manifest-url <url>         Override the artefacts repo base URL.
  --trust-key <path>           Path to a vendor public key file to install
                               under /opt/evo/trust/ (required for any
                               signature verification to succeed later).
  -h, --help                   Show this message.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --channel)      channel="$2";           shift 2;;
        --manifest-url) manifest_base_url="$2"; shift 2;;
        --trust-key)    trust_key="$2";         shift 2;;
        -h|--help)      usage; exit 0;;
        *) err "unknown argument: $1"; usage; exit 2;;
    esac
done

case "${channel}" in
    dev|test|prod) ;;
    *) err "channel must be one of dev, test, prod (got: ${channel})"; exit 2;;
esac

# ---------------------------------------------------------------------
# 2. Root check
# ---------------------------------------------------------------------

if [[ ${EUID} -ne 0 ]]; then
    err "must run as root (try: sudo $0 $*)"
    exit 1
fi

log "channel:          ${channel}"
log "manifest base:    ${manifest_base_url}"
log "trust key:        ${trust_key:-<none supplied>}"
log ""

# ---------------------------------------------------------------------
# 3. Install Population A (Debian runtime dependencies)
# ---------------------------------------------------------------------
#
# The runtime deps are declared by each plugin's manifest and aggregated
# at install time. Until plugins land, the aggregate is empty.

log "phase 3: install Population A (Debian packages)"
log "running apt update"
apt-get update -q
pending "Milestone 3+" "no plugin deps to aggregate yet; apt install step is a no-op today"
log ""

# ---------------------------------------------------------------------
# 4. Create the evo filesystem footprint
# ---------------------------------------------------------------------

log "phase 4: create evo filesystem footprint"
install -d -o root -g root -m 0755 \
    "${EVO_OPT}/bin" \
    "${EVO_OPT}/plugins" \
    "${EVO_OPT}/catalogue" \
    "${EVO_OPT}/trust" \
    "${EVO_OPT}/share/systemd" \
    "${EVO_OPT}/share/examples" \
    "${EVO_ETC}" \
    "${EVO_ETC}/plugins.d" \
    "${EVO_ETC}/trust.d" \
    "${EVO_VAR}/state" \
    "${EVO_VAR}/cache"
log "created:          ${EVO_OPT}/{bin,plugins,catalogue,trust,share/systemd,share/examples}"
log "created:          ${EVO_ETC}/{,plugins.d,trust.d}"
log "created:          ${EVO_VAR}/{state,cache}"
log ""

# ---------------------------------------------------------------------
# 5. Install vendor trust material
# ---------------------------------------------------------------------

log "phase 5: install vendor trust material"
if [[ -n "${trust_key}" ]]; then
    if [[ ! -f "${trust_key}" ]]; then
        err "--trust-key: file not found: ${trust_key}"
        exit 1
    fi
    install -m 0644 "${trust_key}" "${EVO_OPT}/trust/"
    log "installed:        $(basename "${trust_key}") -> ${EVO_OPT}/trust/"
else
    pending "operator input" "no --trust-key provided; all later signature verification depends on this"
fi
log ""

# ---------------------------------------------------------------------
# 6. Fetch artefact manifest
# ---------------------------------------------------------------------

log "phase 6: fetch artefact manifest"
pending "artefact-plane format" "manifest schema not yet defined (BUILD.md section 14)"
log ""

# ---------------------------------------------------------------------
# 7. Verify manifest signature
# ---------------------------------------------------------------------

log "phase 7: verify manifest signature"
pending "signing format" "signing tool and format not yet chosen (BUILD.md section 14)"
log ""

# ---------------------------------------------------------------------
# 8. Fetch and verify each piece
# ---------------------------------------------------------------------

log "phase 8: fetch and verify pieces"
pending "Milestone 3+" "no pieces on the artefact plane yet"
log ""

# ---------------------------------------------------------------------
# 9. Place pieces into the footprint
# ---------------------------------------------------------------------

log "phase 9: place pieces into footprint"
pending "Milestone 3+" "depends on phase 8"
log ""

# ---------------------------------------------------------------------
# 10. Seed configuration
# ---------------------------------------------------------------------

log "phase 10: seed configuration from examples"
pending "Milestone 3+" "configuration examples ship with the piece set"
log ""

# ---------------------------------------------------------------------
# 11. Install systemd unit
# ---------------------------------------------------------------------

log "phase 11: install systemd unit"
pending "Milestone 3+" "systemd unit ships with the piece set"
log ""

# ---------------------------------------------------------------------
# 12. Enable and start the evo service
# ---------------------------------------------------------------------

log "phase 12: enable and start the evo service"
pending "Milestone 3+" "depends on phase 11"
log ""

# ---------------------------------------------------------------------
# 13. Status
# ---------------------------------------------------------------------

log "=============================================="
log "bootstrap complete."
log ""
log "executed today:"
log "  - apt update (phase 3 baseline)"
log "  - evo filesystem footprint created (phase 4)"
if [[ -n "${trust_key}" ]]; then
    log "  - vendor trust material installed (phase 5)"
fi
log ""
log "to finish the bootstrap once later milestones land, the following"
log "must exist on the artefact plane:"
log "  - an artefact manifest for channel '${channel}'"
log "  - signed pieces listed in that manifest"
log "  - a systemd unit in the piece set"
log ""
log "see BUILD.md sections 3 (automation) and 7 (first install)."
log "to wipe and re-try: sudo scripts/device/reset.sh"
