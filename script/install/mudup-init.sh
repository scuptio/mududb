#!/usr/bin/env sh
set -eu

REPO="${MUDUP_INIT_REPO:-scuptio/mududb}"
TARGET="${MUDUP_INIT_TARGET:-x86_64-unknown-linux-gnu}"
INSTALL_BIN_DIR="${HOME}/.local/bin"
MUDUP_HOME_DIR="${MUDUP_HOME:-${HOME}/.mududb}"
MUDUP_PROXY_BIN_DIR="${MUDUP_HOME_DIR}/bin"
BASHRC="${HOME}/.bashrc"
PATH_LINE='export PATH="${HOME}/.local/bin:${HOME}/.mududb/bin:${PATH}"'

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command not found: $1" >&2
    exit 1
  fi
}

need_cmd curl
need_cmd tar
need_cmd sha256sum
need_cmd install
need_cmd mktemp
need_cmd find

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT INT TERM

ARCHIVE="mudup-${TARGET}.tar.gz"
ARCHIVE_URL="https://github.com/${REPO}/releases/latest/download/${ARCHIVE}"
SHA256_URL="${ARCHIVE_URL}.sha256"

curl -fsSL "${ARCHIVE_URL}" -o "${TMP_DIR}/${ARCHIVE}"
curl -fsSL "${SHA256_URL}" -o "${TMP_DIR}/${ARCHIVE}.sha256"

(
  cd "${TMP_DIR}"
  sha256sum -c "${ARCHIVE}.sha256"
)

tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "${TMP_DIR}"
MUDUP_BIN_PATH="$(find "${TMP_DIR}" -type f -path '*/bin/mudup' | head -n 1)"
if [ -z "${MUDUP_BIN_PATH}" ]; then
  echo "error: mudup binary not found in archive ${ARCHIVE}" >&2
  exit 1
fi

mkdir -p "${INSTALL_BIN_DIR}"
install -m 0755 "${MUDUP_BIN_PATH}" "${INSTALL_BIN_DIR}/mudup"

if [ -f "${BASHRC}" ]; then
  if ! grep -qxF "${PATH_LINE}" "${BASHRC}"; then
    printf '%s\n' "${PATH_LINE}" >> "${BASHRC}"
  fi
else
  printf '%s\n' "${PATH_LINE}" > "${BASHRC}"
fi

export PATH="${HOME}/.local/bin:${MUDUP_PROXY_BIN_DIR}:${PATH}"

echo "mudup installed: ${INSTALL_BIN_DIR}/mudup"
echo "PATH updated for current shell and persisted in ${BASHRC}"
echo "next: mudup install"
