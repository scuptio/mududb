#!/bin/bash
# Monitor tpcc-benchmark client CPU usage (cores) during a benchmark run.
# Usage: client_cpu_monitor.sh <out.txt> <duration_secs>
out="$1"; dur="${2:-240}"
> "$out"
end=$((SECONDS + dur))
while [ $SECONDS -lt $end ]; do
  total=0
  for p in /proc/[0-9]*/comm; do
    if [ "$(cat $p 2>/dev/null)" = "tpcc-benchmark" ]; then
      pid=$(echo "$p" | cut -d/ -f3)
      ticks=$(awk '{print $14+$15}' /proc/$pid/stat 2>/dev/null)
      total=$((total + ticks))
    fi
  done
  echo "$SECONDS $total" >> "$out"
  sleep 1
done
