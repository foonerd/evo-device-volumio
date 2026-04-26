#!/usr/bin/env bash
# Clone foonerd/evo-core as a sibling of this repo (../evo-core from GITHUB_WORKSPACE) so
#   evo-plugin-sdk = { path = "../evo-core/crates/evo-plugin-sdk" }
# resolves. On GitHub Actions the workspace is a fresh copy; a sibling is usually empty.
# Locally, if a sibling `evo-core` already exists, it is updated and re-used (not deleted).
# Environment:
#   GITHUB_WORKSPACE - required; the evo-device-volumio root
#   EVO_CORE_TAG     - optional; default v0.1.9. If the tag is missing, falls back to main
set -euo pipefail

ROOT="${GITHUB_WORKSPACE:-}"
if [[ -z "$ROOT" ]]; then
  echo "error: GITHUB_WORKSPACE is not set" >&2
  exit 1
fi
TAG="${EVO_CORE_TAG:-v0.1.9}"
PARENT="$(dirname "$ROOT")"
EEC="${PARENT}/evo-core"

if [[ -d "${EEC}/.git" ]]; then
  echo "Using existing evo-core at ${EEC}"
  git -C "${EEC}" remote set-url origin "https://github.com/foonerd/evo-core.git" || true
  git -C "${EEC}" fetch --no-tags --depth 1 origin main || true
  if git -C "${EEC}" fetch --depth 1 origin "refs/tags/${TAG}:refs/tags/${TAG}" 2>/dev/null; then
    git -C "${EEC}" -c advice.detachedHead=false checkout "${TAG}" || true
    echo "Using evo-core @ ${TAG}"
  else
    echo "Tag ${TAG} not found; trying main" >&2
    git -C "${EEC}" fetch --depth 1 origin main
    git -C "${EEC}" checkout main
  fi
else
  echo "Cloning evo-core into ${EEC} (sibling to workspace)..."
  git clone "https://github.com/foonerd/evo-core.git" "${EEC}"
  if git -C "${EEC}" fetch --depth 1 origin "refs/tags/${TAG}:refs/tags/${TAG}" 2>/dev/null; then
    git -C "${EEC}" -c advice.detachedHead=false checkout "${TAG}"
    echo "Using evo-core @ ${TAG}"
  else
    echo "Tag ${TAG} not found; using main" >&2
    git -C "${EEC}" fetch --depth 1 origin main
    git -C "${EEC}" checkout main
    echo "Using evo-core @ main"
  fi
fi
# Cross.toml mounts this path; must be absolute
export EVO_CORE="${EEC}"
echo "EVO_CORE=${EVO_CORE}"
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "EVO_CORE=${EVO_CORE}" >> "$GITHUB_ENV"
fi
