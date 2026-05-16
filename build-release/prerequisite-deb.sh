#!/usr/bin/env sh
set -eu

# Download deb prerequisites from a package list, then extract and install
# package contents into a specified installation directory.
usage() {
  cat <<'EOF'
Usage:
  prerequisite-deb.sh --install-dir <path> [--package-list <file>] [--target <rust-target>]

Options:
  --install-dir  Destination directory to install files from deb packages
  --package-list Path to package list file (default: script-adjacent package/deb/package-list.txt)
  --target       Optional Rust target triple used for filtering package-list items

Package list format:
  - Empty lines and lines starting with # are ignored.
  - A line with one field is treated as a deb URL for all targets:
      https://example.com/pkg.deb
  - A line with two fields is treated as: <target> <deb_url>
      x86_64-unknown-linux-gnu https://example.com/pkg-amd64.deb
EOF
}

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command not found: $1" >&2
    exit 1
  fi
}

TARGET=""
INSTALL_DIR=""
PACKAGE_LIST=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --install-dir)
      INSTALL_DIR="${2:-}"
      shift 2
      ;;
    --package-list)
      PACKAGE_LIST="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [ -z "${INSTALL_DIR}" ]; then
  echo "error: --install-dir is required" >&2
  usage >&2
  exit 1
fi

need_cmd curl
need_cmd dpkg-deb
need_cmd mktemp
need_cmd mkdir
need_cmd basename
need_cmd cp
need_cmd find
need_cmd readlink
need_cmd ln

if [ -z "${PACKAGE_LIST}" ]; then
  SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
  PACKAGE_LIST="${SCRIPT_DIR}/package/deb/package-list.txt"
fi

if [ ! -f "${PACKAGE_LIST}" ]; then
  echo "error: package list not found: ${PACKAGE_LIST}" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT INT TERM

mkdir -p "${INSTALL_DIR}"
mkdir -p "${INSTALL_DIR}/lib"
installed_count="0"

while IFS= read -r raw_line || [ -n "${raw_line}" ]; do
  line="${raw_line#"${raw_line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"

  [ -z "${line}" ] && continue
  case "${line}" in
    \#*) continue ;;
  esac

  set -- ${line}
  if [ "$#" -eq 1 ]; then
    entry_target=""
    entry_url="$1"
  elif [ "$#" -eq 2 ]; then
    entry_target="$1"
    entry_url="$2"
  else
    echo "error: invalid package-list line: ${line}" >&2
    exit 1
  fi

  # Two-field line: <target> <url>, filter by --target when provided.
  if [ -n "${entry_target}" ] && [ -n "${TARGET}" ] && [ "${entry_target}" != "${TARGET}" ]; then
    continue
  fi
  if [ -n "${entry_target}" ] && [ -z "${TARGET}" ]; then
    continue
  fi

  pkg_basename="$(basename "${entry_url}")"
  deb_path="${TMP_DIR}/${installed_count}-${pkg_basename}"

  echo "Downloading ${entry_url}"
  curl -fL "${entry_url}" -o "${deb_path}"
  dpkg-deb -x "${deb_path}" "${INSTALL_DIR}"
  installed_count=$((installed_count + 1))
done < "${PACKAGE_LIST}"

if [ "${installed_count}" = "0" ]; then
  echo "error: no deb packages installed from ${PACKAGE_LIST}" >&2
  if [ -n "${TARGET}" ]; then
    echo "hint: check whether package-list contains entries for target ${TARGET}" >&2
  fi
  exit 1
fi

# Keep the original deb filesystem layout, and also place shared libraries
# into <install-dir>/lib for simpler runtime linking.
find "${INSTALL_DIR}/usr/lib" -type f -name '*.so*' 2>/dev/null | while IFS= read -r so_file; do
  cp -f "${so_file}" "${INSTALL_DIR}/lib/"
done

# Recreate shared-library symlinks in <install-dir>/lib, pointing to the
# corresponding .so files within the same directory.
find "${INSTALL_DIR}/usr/lib" -type l -name '*.so*' 2>/dev/null | while IFS= read -r so_link; do
  link_name="$(basename "${so_link}")"
  link_target="$(readlink "${so_link}")"
  target_name="$(basename "${link_target}")"

  # Skip if a non-symlink file already exists with the same name.
  if [ -e "${INSTALL_DIR}/lib/${link_name}" ] && [ ! -L "${INSTALL_DIR}/lib/${link_name}" ]; then
    continue
  fi

  if [ -e "${INSTALL_DIR}/lib/${target_name}" ]; then
    ln -sfn "${target_name}" "${INSTALL_DIR}/lib/${link_name}"
  fi
done

echo "Installed ${installed_count} deb package(s) to ${INSTALL_DIR}"
