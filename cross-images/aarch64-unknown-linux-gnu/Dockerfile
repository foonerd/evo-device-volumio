# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026 Just a Nerd
#
# Cross-build image for the evo-device-volumio vendor
# distribution steward, targeting aarch64-unknown-linux-gnu.
#
# Base pinned to `debian:trixie` for runtime-parity ABI. The
# target Volumio devices run a Debian-based OS aligned with
# trixie's package universe; building against Ubuntu 20.04
# focal (cross-rs's default) introduces a build-vs-runtime
# vendor divergence with no upside. Same discipline the sister
# repos evo-device-audio + evo-kiosk-eng adopted; keeps the
# fleet's build environment coherent.
#
# Cross toolchain shape:
#
#   - Host arch (amd64) build-essential for build scripts.
#   - Cross-linker for aarch64: gcc-aarch64-linux-gnu.
#   - dpkg multi-arch: arm64 enabled so apt can install the
#     target-arch dev library.
#   - Target-arch dev library: libasound2-dev:arm64.
#   - pkg-config env vars for cargo cross-linking.
#
# Compatible with cross-rs 0.2.5's `[target.<triple>] dockerfile`
# mechanism.

FROM debian:trixie

ENV DEBIAN_FRONTEND=noninteractive

RUN dpkg --add-architecture arm64 && \
    apt-get update && apt-get install -y --no-install-recommends \
      build-essential \
      pkg-config \
      ca-certificates \
      gcc-aarch64-linux-gnu \
      libc6-dev:arm64 \
      libasound2-dev:arm64 \
    && rm -rf /var/lib/apt/lists/*
# libc6-dev:arm64 required in addition to gcc-aarch64-linux-gnu's
# built-in cross libc: cc-rs-invoked cross builds (e.g. aws-lc-sys /
# ring / rustls native backends) resolve <stdlib.h> under Debian's
# multiarch layout to /usr/include/stdlib.h which indirects to
# bits/libc-header-start.h from libc6-dev:arm64. Without it, the
# cross-compile fails with `fatal error: bits/libc-header-start.h:
# No such file or directory`.

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    PKG_CONFIG_ALLOW_CROSS_aarch64_unknown_linux_gnu=1 \
    PKG_CONFIG_PATH_aarch64_unknown_linux_gnu=/usr/lib/aarch64-linux-gnu/pkgconfig \
    PKG_CONFIG_SYSROOT_DIR_aarch64_unknown_linux_gnu=/
