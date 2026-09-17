# Runs the MSSP syscall corpus tests for the mgen-generated Python codec in
# `mududb/generated/`: drives the cross-language corpus
# crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin
# (47 frames: one request + one ok response per message kind 1..23 in
# MessageKind discriminant order, plus a trailing `get` UniError response).
# The sidecar syscall_payload_v1_all.json is the single source of truth for
# the semantic expectations: for every frame the runner checks, through the
# generated Python codec:
#   - the header routes to the expected message kind,
#   - requests encode from the documented inputs byte-exactly,
#   - decode -> sidecar expect shape == sidecar `expect` object (direct dict
#     comparison; u64/i64 as decimal strings, bytes as lowercase hex, UniOid
#     as {"h","l"}, unit as {"unit": true}, relation cells as hex-or-null),
#   - ok responses encode from the documented values byte-exactly and
#     decode -> re-encode byte-identically, then decode again to the same
#     expect shape (roundtrip),
#   - the err response decodes field-by-field (err_src carries the host's
#     '"None"' quirk verbatim) and re-encodes byte-identically.
# Usage: python3 run_syscall_corpus_test.py   (from anywhere)
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import corpus_common as cc

CHECKS = [0]


def tearDownModule():
    print(f"{CHECKS[0]} checks passed across the syscall_payload_v1_all corpus")


class SyscallCorpusTest(unittest.TestCase):
    def _check_bytes(self, label, what, actual, expected):
        CHECKS[0] += 1
        self.assertEqual(
            actual,
            expected,
            f"{label}: {what} not byte-exact: {cc.diff_detail(actual, expected)}",
        )

    def _check_expect(self, label, what, actual, expect):
        CHECKS[0] += 1
        self.assertEqual(
            actual,
            expect,
            f"{label}: {what} mismatch vs sidecar expect",
        )

    def test_frames(self):
        segments, sidecar = cc.load_fixture(cc.find_fixture_dir(), "syscall_payload_v1_all")
        frames = sidecar["frames"]
        self.assertEqual(
            len(segments),
            len(frames),
            f"segment count {len(segments)} != sidecar frame count {len(frames)}",
        )
        for frame in frames:
            label = f"{frame['index']} {frame['message_kind_name']} {frame['direction']}"
            with self.subTest(frame=label):
                segment = segments[frame["index"]]
                kind = frame["message_kind"]
                direction = frame["direction"]

                CHECKS[0] += 1
                self.assertEqual(
                    cc.header_kind(segment), kind, f"{label}: header kind mismatch"
                )

                if direction == "request":
                    self._check_bytes(label, "request encode", cc.encode_request(kind), segment)
                    self._check_expect(
                        label,
                        "request decode",
                        cc.decode_request_expect(kind, segment),
                        frame["expect"],
                    )
                elif direction == "response":
                    self._check_bytes(label, "response encode", cc.encode_ok_response(kind), segment)
                    self._check_expect(
                        label,
                        "response decode",
                        cc.decode_response_expect(kind, segment),
                        frame["expect"],
                    )
                    reencoded = cc.encode_response(kind, cc.decode_response(kind, segment))
                    self._check_bytes(label, "response decode+re-encode", reencoded, segment)
                    self._check_expect(
                        label,
                        "response re-encode+decode roundtrip",
                        cc.decode_response_expect(kind, reencoded),
                        frame["expect"],
                    )
                elif direction == "response_err":
                    self._check_bytes(label, "err response encode", cc.encode_get_err_response(), segment)
                    self._check_expect(
                        label,
                        "err response decode",
                        cc.decode_err_expect(kind, segment),
                        frame["expect"],
                    )
                    reencoded = cc.encode_response(kind, cc.decode_response(kind, segment))
                    self._check_bytes(label, "err response decode+re-encode", reencoded, segment)
                else:
                    self.fail(f"{label}: unknown direction {direction}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
