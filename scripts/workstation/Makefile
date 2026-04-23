# Makefile - workstation-side wrappers for building evo-device-volumio
# and the evo-core steward for the aarch64 target.
#
# Prerequisites on the workstation:
#   - Rust toolchain with the aarch64-unknown-linux-gnu target:
#         rustup target add aarch64-unknown-linux-gnu
#   - cross, with Docker or Podman running:
#         cargo install cross --locked
#   - evo-core cloned locally (default: ../../evo-core relative to this
#     Makefile's directory, i.e. ~/src/evo/evo-core). Override with
#     EVO_CORE_PATH=<path>.
#
# Usage from the workspace root:
#     make -C scripts/workstation help
#     make -C scripts/workstation build-plugins
#     make -C scripts/workstation build-steward
#     make -C scripts/workstation build-all
#
# See BUILD.md section 6 for the wider build procedure.

# Paths are relative to this Makefile's directory.
REPO_ROOT     := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/../..)
EVO_CORE_PATH ?= $(abspath $(REPO_ROOT)/../evo-core)
EVO_CORE_TAG  ?= v0.1.8
TARGET_ARCH   ?= aarch64-unknown-linux-gnu

.PHONY: help
help:
	@echo "evo-device-volumio workstation build helpers"
	@echo ""
	@echo "Targets:"
	@echo "  build-plugins   Cross-compile all plugin crates in this workspace."
	@echo "  build-steward   Cross-compile the steward from evo-core at EVO_CORE_TAG."
	@echo "  build-all       Both of the above."
	@echo "  check-env       Verify the workstation has the required tools."
	@echo ""
	@echo "Variables:"
	@echo "  REPO_ROOT       $(REPO_ROOT)"
	@echo "  EVO_CORE_PATH   $(EVO_CORE_PATH)"
	@echo "  EVO_CORE_TAG    $(EVO_CORE_TAG)"
	@echo "  TARGET_ARCH     $(TARGET_ARCH)"

.PHONY: check-env
check-env:
	@command -v cross >/dev/null 2>&1 \
	    || { echo "ERROR: 'cross' not found. Install with: cargo install cross --locked"; exit 1; }
	@command -v git >/dev/null 2>&1 \
	    || { echo "ERROR: 'git' not found."; exit 1; }
	@if [ ! -d "$(EVO_CORE_PATH)" ]; then \
	    echo "ERROR: evo-core path not found: $(EVO_CORE_PATH)"; \
	    echo "Clone evo-core or set EVO_CORE_PATH=<path>"; \
	    exit 1; \
	fi
	@echo "check-env: OK"

.PHONY: build-plugins
build-plugins: check-env
	@echo "[build-plugins] cross-compiling plugin workspace for $(TARGET_ARCH)"
	cd $(REPO_ROOT) && cross build --release --target $(TARGET_ARCH) --workspace

.PHONY: build-steward
build-steward: check-env
	@echo "[build-steward] cross-compiling evo-core@$(EVO_CORE_TAG) for $(TARGET_ARCH)"
	cd $(EVO_CORE_PATH) && git fetch --tags
	cd $(EVO_CORE_PATH) && git checkout $(EVO_CORE_TAG)
	cd $(EVO_CORE_PATH) && cross build --release --target $(TARGET_ARCH) -p evo

.PHONY: build-all
build-all: build-steward build-plugins
	@echo "[build-all] done"
