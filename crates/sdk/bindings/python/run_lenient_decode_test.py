# Runs the MSSP lenient-decode vector tests for the mgen-generated Python
# codec in `mududb/generated/`: drives the hand-built NON-canonical frames in
# crates/db-kernel/testing/fixtures/golden/v1/lenient_decode_v1.bin
# (8 vectors: integer width widening, unsigned markers for signed values,
# wide negative ints, record map key reordering, unknown/skipped map keys,
# missing request parameters, missing record fields/variant cases).
# The vectors are not re-encodable by the canonical encoder, so no encode or
# re-encode comparison is done; for every vector the runner checks, through
# the generated Python codec:
#   - the header routes to the expected message kind,
#   - decode -> sidecar expect shape == sidecar `expect` object from
#     lenient_decode_v1.json, which is the single source of truth.
# Usage: python3 run_lenient_decode_test.py   (from anywhere)
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import corpus_common as cc

CHECKS = [0]


def tearDownModule():
    print(f"{CHECKS[0]} checks passed across the lenient_decode_v1 corpus")


class LenientDecodeTest(unittest.TestCase):
    def test_vectors(self):
        segments, sidecar = cc.load_fixture(cc.find_fixture_dir(), "lenient_decode_v1")
        vectors = sidecar["vectors"]
        self.assertEqual(
            len(segments),
            len(vectors),
            f"segment count {len(segments)} != sidecar vector count {len(vectors)}",
        )
        for vector in vectors:
            label = (
                f"{vector['index']} {vector['kind']} "
                f"({vector['message_kind_name']} {vector['direction']})"
            )
            with self.subTest(vector=label):
                segment = segments[vector["index"]]
                kind = vector["message_kind"]

                CHECKS[0] += 1
                self.assertEqual(
                    cc.header_kind(segment), kind, f"{label}: header kind mismatch"
                )

                if vector["direction"] == "request":
                    actual = cc.decode_request_expect(kind, segment)
                elif vector["direction"] == "response":
                    actual = cc.decode_response_expect(kind, segment)
                else:
                    self.fail(f"{label}: unknown direction {vector['direction']}")

                CHECKS[0] += 1
                self.assertEqual(
                    actual,
                    vector["expect"],
                    f"{label}: decode mismatch vs sidecar expect ({vector['note']})",
                )


if __name__ == "__main__":
    unittest.main(verbosity=2)
