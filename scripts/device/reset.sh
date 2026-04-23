#!/usr/bin/env bash
#
# reset.sh - wipe the evo footprint on a device.
#
# Intended use: developer iteration on the same Pi without re-flashing
# the SD card. Safe to run on a machine that has no evo installed (all
# phases skip cleanly when their target does not exist).
#
# By default wipes all three evo roots:
#   /opt/evo         (package-owned content)
#   /etc/evo         (operator policy - use --keep-policy to preserve)
#   /var/lib/evo     (runtime state)
#
# Also disables and removes /etc/systemd/system/evo.service if present.

set -euo pipefail

# ---------------------------------------------------------------------
# Arg parsing
# ---------------------------------------------------------------------

keep_policy=0

usage() {
    cat <<'EOF'
Usage: sudo reset.sh [options]

Options:
  --keep-policy    Preserve /etc/evo (operator config, trust keys under
                   trust.d/). Useful when re-bootstrapping with the same
                   trust material already in place.
  -h, --help       Show this message.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep-policy) keep_policy=1; shift;;
        -h|--help)     usage; exit 0;;
        *) printf 'unknown argument: %s\n' "$1" >&2; usage; exit 2;;
    esac
done

# ---------------------------------------------------------------------
# Root check
# ---------------------------------------------------------------------

if [[ ${EUID} -ne 0 ]]; then
    printf 'must run as root (try: sudo %s %s)\n' "$0" "$*" >&2
    exit 1
fi

log() { printf '[reset] %s\n' "$*"; }

# ---------------------------------------------------------------------
# Systemd unit
# ---------------------------------------------------------------------

if systemctl list-unit-files 2>/dev/null | grep -q '^evo\.service'; then
    log "stopping and disabling evo.service"
    systemctl disable --now evo.service || true
fi

if [[ -f /etc/systemd/system/evo.service ]]; then
    log "removing /etc/systemd/system/evo.service"
    rm -f /etc/systemd/system/evo.service
    systemctl daemon-reload
fi

# ---------------------------------------------------------------------
# Unconditional roots: /opt/evo and /var/lib/evo
# ---------------------------------------------------------------------

for d in /opt/evo /var/lib/evo; do
    if [[ -d "$d" ]]; then
        log "removing $d"
        rm -rf "$d"
    fi
done

# ---------------------------------------------------------------------
# /etc/evo respects --keep-policy
# ---------------------------------------------------------------------

if [[ ${keep_policy} -eq 1 ]]; then
    if [[ -d /etc/evo ]]; then
        log "keeping /etc/evo (--keep-policy)"
    fi
else
    if [[ -d /etc/evo ]]; then
        log "removing /etc/evo"
        rm -rf /etc/evo
    fi
fi

log "reset complete."
log "to re-install: sudo scripts/device/bootstrap.sh [options]"
