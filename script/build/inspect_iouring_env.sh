#!/usr/bin/env bash
set -euo pipefail

echo "== io_uring environment =="
echo "date: $(date -Is)"
echo "user: $(id)"
echo "uname: $(uname -a)"

echo
echo "== kernel toggles =="
if [[ -r /proc/sys/kernel/io_uring_disabled ]]; then
  echo -n "kernel.io_uring_disabled="
  cat /proc/sys/kernel/io_uring_disabled
else
  echo "kernel.io_uring_disabled=<unavailable>"
fi

echo
echo "== process status =="
grep -E '^(Seccomp|CapEff|NoNewPrivs):' /proc/self/status || true

echo
echo "== cgroup =="
cat /proc/self/cgroup || true

echo
echo "== container hints =="
if [[ -f /.dockerenv ]]; then
  echo "/.dockerenv=present"
else
  echo "/.dockerenv=absent"
fi

if command -v docker >/dev/null 2>&1; then
  echo
  echo "== docker socket =="
  ls -l /var/run/docker.sock 2>/dev/null || true
fi

