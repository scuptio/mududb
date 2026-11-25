#!/usr/bin/env python3
"""Summarize Linux x86-64 gperftools CPU samples without a pprof dependency.

Records include setup/teardown unless the caller arranges a separate sampling
window. Symbols reflect optimized code, including inlining; percentages are
sample shares, not exact timings. Keep the exact executable used for sampling.
"""
import argparse
import bisect
import collections
import json
from pathlib import Path
import struct
import subprocess


def read_profile(path):
    data = path.read_bytes()
    header = struct.unpack_from('<5Q', data)
    if header[:3] != (0, 3, 0):
        raise ValueError('expected 64-bit little-endian gperftools CPU format')
    offset, samples = 40, []
    while offset + 16 <= len(data):
        count, depth = struct.unpack_from('<2Q', data, offset)
        offset += 16
        if depth > 4096 or offset + depth * 8 > len(data):
            raise ValueError('invalid stack record')
        stack = struct.unpack_from(f'<{depth}Q', data, offset)
        offset += depth * 8
        if depth == 1 and stack[0] == 0:
            break
        samples.append((count, stack))
    maps = []
    for line in data[offset:].decode().splitlines():
        fields = line.split(maxsplit=5)
        if len(fields) == 6 and '-' in fields[0]:
            start, end = (int(x, 16) for x in fields[0].split('-'))
            maps.append((start, end, int(fields[2], 16), fields[5]))
    return samples, maps, header[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('profile', type=Path)
    parser.add_argument('executable', type=Path)
    parser.add_argument('--measured-window', action='store_true', help='profile was explicitly started/stopped around benchmark operations')
    args = parser.parse_args()
    samples, maps, period = read_profile(args.profile)
    # Identify the original mapped executable even when the caller kept a copy.
    executable_maps = [m for m in maps if '/target/' in m[3] and ('/deps/index-' in m[3] or Path(m[3]).name == args.executable.name)]
    if not executable_maps:
        raise ValueError('index executable absent from profile maps')
    base = min(start - offset for start, _, offset, _ in executable_maps)
    output = subprocess.check_output(['nm', '-S', '-n', '-C', '--defined-only', str(args.executable)], text=True)
    symbols = []
    for line in output.splitlines():
        fields = line.split(maxsplit=3)
        if len(fields) == 4 and fields[2] in ('t', 'T', 'w', 'W'):
            symbols.append((int(fields[0], 16), int(fields[1], 16), fields[3]))
    symbols.sort()
    addresses = [s[0] for s in symbols]

    def symbol(pc):
        if any(start <= pc < end for start, end, _, _ in executable_maps):
            value = pc - base
            index = bisect.bisect_right(addresses, value) - 1
            if index >= 0 and value < symbols[index][0] + symbols[index][1]:
                return symbols[index][2]
            return '[index: unresolved]'
        for start, end, _, name in maps:
            if start <= pc < end:
                return '[' + Path(name).name + ']'
        return '[unmapped]'

    flat, inclusive = collections.Counter(), collections.Counter()
    for count, stack in samples:
        if stack:
            flat[symbol(stack[0])] += count
            for name in set(map(symbol, stack)):
                inclusive[name] += count
    total = sum(count for count, _ in samples)
    if not total:
        raise ValueError('profile contains no CPU samples; increase the workload duration')
    result = {'measured_window': args.measured_window, 'samples': total, 'period_us': period,
              'flat': flat.most_common(40), 'inclusive': inclusive.most_common(40)}
    args.profile.with_suffix('.json').write_text(json.dumps(result, indent=2) + '\n')
    lines = [f'Samples: {total}; nominal period: {period} us', ('Measured operation window including task/harness work; excludes preload/warmup/teardown.' if args.measured_window else 'Includes setup/teardown; optimized symbol attribution.'), '']
    if total < 100:
        lines += ['Warning: fewer than 100 samples; increase --ops (and --keys for delete) before attributing hotspots.', '']
    for title, counter in [('Self', flat), ('Inclusive (overlapping)', inclusive)]:
        lines.append(title)
        lines.extend(f'{count / total:6.2%} {count:8d} {name}' for name, count in counter.most_common(25))
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
