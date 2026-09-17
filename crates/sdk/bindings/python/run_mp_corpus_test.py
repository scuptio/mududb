# Runs the shared MessagePack primitive corpus tests against the handwritten
# canonical encoder/decoder in `mududb.codec.mpack`: for every vector of the
# cross-language corpus crates/db-kernel/testing/fixtures/golden/v1/
# mp_primitives_v1.bin (+ the mp_primitives_v1.json sidecar, generated from
# rmp_serde 1.3.1) the expected value is rebuilt from the sidecar (including
# the str_fill/bin_fill/array_fill/combo patterns), then checked that
#   - write_value encodes it to exactly the golden segment bytes (pins the
#     canonical encoder), and
#   - read_value decodes the segment back to the same value.
# Usage: python3 run_mp_corpus_test.py   (from anywhere)
import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import corpus_common as cc
from mududb.codec.mpack import F32, MpackReader, MpackWriter

CHECKS = [0]


def tearDownModule():
    print(f"{CHECKS[0]} checks passed across the mp_primitives_v1 corpus")


def f32_truncate(v):
    """Truncate a Python (double) float to single precision; e.g. 0.1 ->
    0.10000000149011612. This is the expected behavior for f32 vectors."""
    return struct.unpack(">f", struct.pack(">f", v))[0]


def pattern_bin(n):
    # Sidecar `bin_fill` rule: byte i = i mod 251.
    return bytes(i % 251 for i in range(n))


def expected_value(vector):
    """Rebuild the expected value from the sidecar vector entry."""
    kind = vector["kind"]
    if kind in ("u64", "i64"):
        return int(vector["value"])
    if kind == "f32":
        return F32(f32_truncate(float(vector["value"])))
    if kind == "f64":
        return float(vector["value"])
    if kind == "nil":
        return None
    if kind == "bool":
        return vector["value"]
    if kind == "str":
        return vector["value"] if "value" in vector else "a" * vector["len"]
    if kind == "bin":
        return pattern_bin(vector["len"])
    if kind == "array":
        return list(range(vector["len"]))
    if kind == "combo":
        return [42, "hi", bytes.fromhex("deadbeef")]
    raise ValueError(f"unknown vector kind {kind}")


def vector_label(vector):
    label = f"#{vector['index']} {vector['kind']}"
    if "value" in vector:
        label += f" {vector['value']!r}"
    if "len" in vector:
        label += f" len {vector['len']}"
    return label


class MpCorpusTest(unittest.TestCase):
    def test_vectors(self):
        segments, sidecar = cc.load_fixture(cc.find_fixture_dir(), "mp_primitives_v1")
        vectors = sidecar["vectors"]
        self.assertEqual(
            len(segments),
            len(vectors),
            f"segment count {len(segments)} != sidecar vector count {len(vectors)}",
        )
        for vector in vectors:
            with self.subTest(vector=vector_label(vector)):
                label = vector_label(vector)
                segment = segments[vector["index"]]
                value = expected_value(vector)

                writer = MpackWriter()
                writer.write_value(value)
                encoded = writer.to_bytes()
                CHECKS[0] += 1
                self.assertEqual(
                    encoded,
                    segment,
                    f"{label}: encode not byte-exact: {cc.diff_detail(encoded, segment)}",
                )

                reader = MpackReader(segment)
                decoded = reader.read_value()
                CHECKS[0] += 1
                self.assertEqual(
                    decoded,
                    float(value) if isinstance(value, F32) else value,
                    f"{label}: decode mismatch",
                )
                CHECKS[0] += 1
                self.assertTrue(reader.is_done(), f"{label}: trailing bytes after decode")


if __name__ == "__main__":
    unittest.main(verbosity=2)
