#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PKGROOT="${REPO_ROOT}/.pkgroot"

if [[ -f "${HOME}/.cargo/env" ]]; then
  # Load the user-local Rust toolchain installed for this repo.
  # shellcheck disable=SC1090
  source "${HOME}/.cargo/env"
fi

export PKG_CONFIG_PATH="${PKGROOT}/usr/lib/x86_64-linux-gnu/pkgconfig"
export PKG_CONFIG_SYSROOT_DIR="${PKGROOT}"
export CFLAGS="-I${PKGROOT}/usr/include${CFLAGS:+ ${CFLAGS}}"
export C_INCLUDE_PATH="${PKGROOT}/usr/include${C_INCLUDE_PATH:+:${C_INCLUDE_PATH}}"
export LIBRARY_PATH="${PKGROOT}/usr/lib/x86_64-linux-gnu${LIBRARY_PATH:+:${LIBRARY_PATH}}"

cd "${SCRIPT_DIR}"

if [[ "${DATA_WALKER_SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --bin data_walker
fi

exec target/debug/data_walker gui "$@"
