#!/usr/bin/env sh
set -eu

usage() {
  cat <<'EOF'
Usage:
  prerequisite.sh --install-dir <path> [--target <rust-target>] [--package-list <file>]

Description:
  Platform dispatcher for prerequisite installation.
  - Deb-supported Linux target: execute prerequisite-deb.sh
  - Other platforms/targets: return error
EOF
}

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
DEB_SCRIPT="${SCRIPT_DIR}/prerequisite-deb.sh"
INSTALL_DIR=""

parse_install_dir() {
  prev=""
  for arg in "$@"; do
    if [ "${prev}" = "--install-dir" ]; then
      INSTALL_DIR="${arg}"
      return 0
    fi
    prev="${arg}"
  done
  return 1
}

read_min_glibc_from_manifest() {
  manifest_path="$1/manifest.txt"
  if [ ! -f "${manifest_path}" ]; then
    echo "error: manifest not found: ${manifest_path}" >&2
    return 1
  fi
  awk -F= '/^min_glibc=/{print $2; exit}' "${manifest_path}"
}

detect_host_glibc() {
  if command -v getconf >/dev/null 2>&1; then
    getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}'
    return 0
  fi
  if command -v ldd >/dev/null 2>&1; then
    ldd --version 2>/dev/null | head -n1 | sed -E 's/.* ([0-9]+\.[0-9]+).*/\1/'
    return 0
  fi
  return 1
}

version_ge() {
  required="$1"
  actual="$2"
  req_major="${required%%.*}"
  req_minor="${required#*.}"
  act_major="${actual%%.*}"
  act_minor="${actual#*.}"
  [ "${act_major}" -gt "${req_major}" ] && return 0
  [ "${act_major}" -lt "${req_major}" ] && return 1
  [ "${act_minor}" -ge "${req_minor}" ]
}

check_min_glibc() {
  required="$1"
  actual="$(detect_host_glibc || true)"
  if [ -z "${actual}" ]; then
    echo "error: unable to detect host glibc version (requires getconf or ldd)" >&2
    return 1
  fi
  if ! version_ge "${required}" "${actual}"; then
    echo "error: host glibc ${actual} is lower than required ${required}" >&2
    return 1
  fi
  echo "glibc check passed: host ${actual}, required >= ${required}"
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

if ! parse_install_dir "$@"; then
  echo "error: --install-dir is required" >&2
  usage >&2
  exit 1
fi

MIN_GLIBC="$(read_min_glibc_from_manifest "${INSTALL_DIR}" || true)"
if [ -n "${MIN_GLIBC}" ]; then
  check_min_glibc "${MIN_GLIBC}"
fi

OS="$(uname -s 2>/dev/null || echo unknown)"
TARGET_ARG=""
prev=""
for arg in "$@"; do
  if [ "${prev}" = "--target" ]; then
    TARGET_ARG="${arg}"
    break
  fi
  prev="${arg}"
done

is_deb_supported_target() {
  case "$1" in
    x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|armv7-unknown-linux-gnueabihf)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

if [ "${OS}" != "Linux" ]; then
  echo "error: platform ${OS} is not supported by prerequisite-deb.sh" >&2
  exit 1
fi

if [ -n "${TARGET_ARG}" ]; then
  if ! is_deb_supported_target "${TARGET_ARG}"; then
    echo "error: target ${TARGET_ARG} is not a supported deb platform" >&2
    exit 1
  fi
else
  ARCH="$(uname -m 2>/dev/null || echo unknown)"
  case "${ARCH}" in
    x86_64|amd64) TARGET_ARG="x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) TARGET_ARG="aarch64-unknown-linux-gnu" ;;
    armv7l|armv7) TARGET_ARG="armv7-unknown-linux-gnueabihf" ;;
    *)
      echo "error: host architecture ${ARCH} is not a supported deb platform; pass --target explicitly" >&2
      exit 1
      ;;
  esac
fi

if [ ! -x "${DEB_SCRIPT}" ]; then
  echo "error: prerequisite deb script not found or not executable: ${DEB_SCRIPT}" >&2
  exit 1
fi

exec "${DEB_SCRIPT}" "$@"
