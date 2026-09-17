# Unit tests for the handwritten MessagePack runtime `mududb.codec.mpack`.
# Runnable from anywhere: the package root is inserted into sys.path.
import os
import struct
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from mududb.codec.mpack import F32, MpackReader, MpackWriter


def encode(v):
    w = MpackWriter()
    w.write_value(v)
    return w.to_bytes()


def decode(data):
    return MpackReader(data).read_value()


class TestCanonicalInts(unittest.TestCase):
    def test_unsigned_boundaries(self):
        cases = [
            (0, bytes([0x00])),
            (1, bytes([0x01])),
            (127, bytes([0x7F])),
            (128, bytes([0xCC, 0x80])),
            (255, bytes([0xCC, 0xFF])),
            (256, bytes([0xCD, 0x01, 0x00])),
            (65535, bytes([0xCD, 0xFF, 0xFF])),
            (65536, bytes([0xCE, 0x00, 0x01, 0x00, 0x00])),
            (4294967295, bytes([0xCE, 0xFF, 0xFF, 0xFF, 0xFF])),
            (4294967296, bytes([0xCF, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00])),
            (18446744073709551615, b"\xcf" + b"\xff" * 8),
        ]
        for value, want in cases:
            with self.subTest(value=value):
                self.assertEqual(encode(value), want)
                w = MpackWriter()
                w.write_u64(value)
                self.assertEqual(w.to_bytes(), want)
                self.assertEqual(decode(want), value)

    def test_negative_boundaries(self):
        cases = [
            (-1, bytes([0xFF])),
            (-32, bytes([0xE0])),
            (-33, bytes([0xD0, 0xDF])),
            (-128, bytes([0xD0, 0x80])),
            (-129, bytes([0xD1, 0xFF, 0x7F])),
            (-32768, bytes([0xD1, 0x80, 0x00])),
            (-32769, bytes([0xD2, 0xFF, 0xFF, 0x7F, 0xFF])),
            (-2147483648, bytes([0xD2, 0x80, 0x00, 0x00, 0x00])),
            (-2147483649, bytes([0xD3, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F, 0xFF, 0xFF, 0xFF])),
            (-9223372036854775808, b"\xd3\x80" + b"\x00" * 7),
        ]
        for value, want in cases:
            with self.subTest(value=value):
                self.assertEqual(encode(value), want)
                w = MpackWriter()
                w.write_i64(value)
                self.assertEqual(w.to_bytes(), want)
                self.assertEqual(decode(want), value)

    def test_nonnegative_i64_uses_unsigned_chain(self):
        w = MpackWriter()
        w.write_i64(127)
        self.assertEqual(w.to_bytes(), bytes([0x7F]))
        w = MpackWriter()
        w.write_i64(128)
        self.assertEqual(w.to_bytes(), bytes([0xCC, 0x80]))

    def test_out_of_range_rejected(self):
        for v in (1 << 64, -(1 << 63) - 1):
            with self.subTest(value=v):
                with self.assertRaises(ValueError):
                    encode(v)
        with self.assertRaises(ValueError):
            MpackWriter().write_u64(-1)
        with self.assertRaises(ValueError):
            MpackWriter().write_u64(1 << 64)
        with self.assertRaises(ValueError):
            MpackWriter().write_i64(1 << 63)
        with self.assertRaises(ValueError):
            MpackWriter().write_i64(-(1 << 63) - 1)

    def test_bool_is_not_int(self):
        # bool must dispatch before int (bool is an int subclass)
        self.assertEqual(encode(True), bytes([0xC3]))
        self.assertEqual(encode(False), bytes([0xC2]))
        self.assertIs(decode(bytes([0xC3])), True)
        self.assertIs(decode(bytes([0xC2])), False)


class TestFloats(unittest.TestCase):
    def test_plain_float_is_f64(self):
        self.assertEqual(encode(1.5), b"\xcb" + struct.pack(">d", 1.5))

    def test_f32_marker_emits_ca(self):
        self.assertEqual(encode(F32(1.5)), b"\xca" + struct.pack(">f", 1.5))

    def test_f32_truncates_to_single_precision(self):
        v = F32(0.1)
        self.assertEqual(encode(v), b"\xca" + struct.pack(">f", 0.1))
        self.assertNotEqual(encode(v), b"\xca" + struct.pack(">d", 0.1)[:4])

    def test_lenient_float_reads(self):
        r = MpackReader(b"\xcb" + struct.pack(">d", 1.5))
        self.assertEqual(r.read_f32(), 1.5)
        r = MpackReader(b"\xca" + struct.pack(">f", 1.5))
        self.assertEqual(r.read_f64(), 1.5)
        # f64 wire value truncated to f32 by read_f32
        r = MpackReader(b"\xcb" + struct.pack(">d", 0.1))
        self.assertEqual(r.read_f32(), struct.unpack(">f", struct.pack(">f", 0.1))[0])
        # generic read_value maps both markers to float
        self.assertEqual(decode(b"\xca" + struct.pack(">f", 2.5)), 2.5)
        self.assertEqual(decode(b"\xcb" + struct.pack(">d", 2.5)), 2.5)

    def test_float_marker_mismatch(self):
        with self.assertRaises(ValueError):
            MpackReader(b"\x01").read_f32()
        with self.assertRaises(ValueError):
            MpackReader(b"\x01").read_f64()


class TestStringsAndBinary(unittest.TestCase):
    def test_str_width_thresholds(self):
        cases = [
            ("", 1),
            ("a" * 31, 1 + 31),   # fixstr
            ("a" * 32, 2 + 32),   # str8
            ("a" * 255, 2 + 255),
            ("a" * 256, 3 + 256), # str16
        ]
        for s, total in cases:
            with self.subTest(n=len(s)):
                data = encode(s)
                self.assertEqual(len(data), total)
                self.assertEqual(decode(data), s)
        self.assertEqual(encode("a" * 31)[0], 0xBF)
        self.assertEqual(encode("a" * 32)[:2], bytes([0xD9, 32]))
        self.assertEqual(encode("a" * 256)[:3], bytes([0xDA, 0x01, 0x00]))

    def test_str_unicode(self):
        s = "héllo世界"
        self.assertEqual(decode(encode(s)), s)

    def test_bin_width_thresholds(self):
        cases = [
            (b"", bytes([0xC4, 0])),
            (b"\x00" * 255, bytes([0xC4, 255]) + b"\x00" * 255),
            (b"\x00" * 256, bytes([0xC5, 0x01, 0x00]) + b"\x00" * 256),
        ]
        for b, want in cases:
            with self.subTest(n=len(b)):
                self.assertEqual(encode(b), want)
                self.assertEqual(decode(want), b)

    def test_bytearray_encodes_as_bin(self):
        self.assertEqual(encode(bytearray(b"\x01\x02")), bytes([0xC4, 0x02, 0x01, 0x02]))

    def test_bin_decodes_to_bytes(self):
        out = decode(bytes([0xC4, 0x02, 0x01, 0x02]))
        self.assertIsInstance(out, bytes)
        self.assertEqual(out, b"\x01\x02")


class TestContainers(unittest.TestCase):
    def test_array_header_thresholds(self):
        for n, header in [(0, bytes([0x90])), (15, bytes([0x9F])),
                          (16, bytes([0xDC, 0x00, 0x10])), (65536, bytes([0xDD, 0x00, 0x01, 0x00, 0x00]))]:
            with self.subTest(n=n):
                w = MpackWriter()
                w.write_array_header(n)
                self.assertEqual(w.to_bytes(), header)

    def test_map_header_thresholds(self):
        for n, header in [(0, bytes([0x80])), (15, bytes([0x8F])), (16, bytes([0xDE, 0x00, 0x10]))]:
            with self.subTest(n=n):
                w = MpackWriter()
                w.write_map_header(n)
                self.assertEqual(w.to_bytes(), header)

    def test_nested_roundtrip(self):
        value = {
            1: [1, -2, 3.5, "four", b"\x05", None, True],
            2: {"k": (1, 2, {"inner": []})},
        }
        self.assertEqual(decode(encode(value)), {1: [1, -2, 3.5, "four", b"\x05", None, True],
                                                  2: {"k": [1, 2, {"inner": []}]}})

    def test_map_keys_encode_recursively(self):
        self.assertEqual(encode({"a": 1}), b"\x81\xa1a\x01")

    def test_headers_read_back(self):
        self.assertEqual(MpackReader(bytes([0x93])).read_array_header(), 3)
        self.assertEqual(MpackReader(bytes([0x82])).read_map_header(), 2)
        self.assertEqual(MpackReader(bytes([0xDC, 0x00, 0x10])).read_array_header(), 16)
        with self.assertRaises(ValueError):
            MpackReader(bytes([0x01])).read_array_header()
        with self.assertRaises(ValueError):
            MpackReader(bytes([0x01])).read_map_header()


class TestLenientReads(unittest.TestCase):
    def test_read_u64_accepts_wider_unsigned_markers(self):
        self.assertEqual(MpackReader(bytes([0x02])).read_u64(), 2)
        self.assertEqual(MpackReader(bytes([0xCC, 0x02])).read_u64(), 2)
        self.assertEqual(MpackReader(bytes([0xCE, 0, 0, 0, 2])).read_u64(), 2)

    def test_read_u64_rejects_negative(self):
        with self.assertRaises(ValueError):
            MpackReader(bytes([0xFF])).read_u64()  # negfixint -1
        with self.assertRaises(ValueError):
            MpackReader(bytes([0xD0, 0xDF])).read_u64()  # i8 -33

    def test_read_i64_accepts_unsigned_markers(self):
        self.assertEqual(MpackReader(bytes([0xCC, 0x08])).read_i64(), 8)
        self.assertEqual(MpackReader(bytes([0xD3]) + struct.pack(">q", -2)).read_i64(), -2)

    def test_read_i64_rejects_too_wide_u64(self):
        with self.assertRaises(ValueError):
            MpackReader(bytes([0xCF]) + struct.pack(">Q", 1 << 63)).read_i64()

    def test_read_bool_and_nil(self):
        self.assertTrue(MpackReader(b"\xc0").is_nil())
        self.assertFalse(MpackReader(b"\xc2").is_nil())
        MpackReader(b"\xc0").read_nil()
        with self.assertRaises(ValueError):
            MpackReader(b"\x01").read_nil()
        self.assertTrue(MpackReader(b"\xc3").read_bool())
        with self.assertRaises(ValueError):
            MpackReader(b"\x01").read_bool()

    def test_read_str_bin(self):
        self.assertEqual(MpackReader(b"\xa3foo").read_str(), "foo")
        self.assertEqual(MpackReader(bytes([0xC4, 0x02]) + b"\x01\x02").read_bin(), b"\x01\x02")
        with self.assertRaises(ValueError):
            MpackReader(b"\x01").read_str()
        with self.assertRaises(ValueError):
            MpackReader(b"\x01").read_bin()

    def test_read_map_key(self):
        # integer keys of any width
        r = MpackReader(bytes([0x05, 0xCC, 0x07, 0xD0, 0xFE]))
        self.assertEqual(r.read_map_key(), 5)
        self.assertEqual(r.read_map_key(), 7)
        # negative key collapses to the never-used key 0
        self.assertEqual(r.read_map_key(), 0)
        # non-integer key is consumed and reported as 0
        r = MpackReader(b"\xa4note\x01")
        self.assertEqual(r.read_map_key(), 0)
        self.assertEqual(r.read_value(), 1)  # only the value remains
        with self.assertRaises(ValueError):
            MpackReader(b"").read_map_key()

    def test_is_done(self):
        r = MpackReader(b"\x01\x02")
        self.assertFalse(r.is_done())
        r.read_value()
        self.assertFalse(r.is_done())
        r.read_value()
        self.assertTrue(r.is_done())
        self.assertTrue(MpackReader(b"").is_done())

    def test_exhaustion_raises(self):
        with self.assertRaises(ValueError):
            MpackReader(b"").read_value()
        with self.assertRaises(ValueError):
            MpackReader(bytes([0xCD, 0x00])).read_value()  # truncated u16
        with self.assertRaises(ValueError):
            MpackReader(b"\xa5ab").read_value()  # truncated str

    def test_unknown_marker_raises(self):
        with self.assertRaises(ValueError):
            MpackReader(bytes([0xC1])).read_value()


class TestSkipValue(unittest.TestCase):
    def test_skip_scalars(self):
        for blob in [b"\x01", b"\xff", b"\xc0", b"\xc2", b"\xcc\x01",
                     b"\xcd\x00\x01", b"\xca" + b"\x00" * 4, b"\xcf" + b"\x00" * 8]:
            with self.subTest(blob=blob.hex()):
                r = MpackReader(blob + b"\x2a")
                r.skip_value()
                self.assertEqual(r.read_value(), 42)
                self.assertTrue(r.is_done())

    def test_skip_nested_containers(self):
        # {1: [1, {2: "x", "k": [true, nil]}], "skip-me": (0xC4 bin)}
        w = MpackWriter()
        w.write_value({1: [1, {2: "x", "k": [True, None]}], "m": b"\x01\x02"})
        nested = w.to_bytes()
        r = MpackReader(nested + b"\x2a")
        r.skip_value()
        self.assertEqual(r.read_value(), 42)
        self.assertTrue(r.is_done())

    def test_skip_then_map_key(self):
        # string key with a nested value: read_map_key consumes the key only
        w = MpackWriter()
        w.write_value(["deep", {"a": [1, 2]}])
        r = MpackReader(b"\xa4note" + w.to_bytes())
        self.assertEqual(r.read_map_key(), 0)
        r.skip_value()  # the caller skips the associated value
        self.assertTrue(r.is_done())

    def test_skip_unskippable_marker(self):
        with self.assertRaises(ValueError):
            MpackReader(bytes([0xC1])).skip_value()
        with self.assertRaises(ValueError):
            MpackReader(bytes([0xC4, 0x05, 0x01])).skip_value()  # truncated bin


class TestWriteValueDispatch(unittest.TestCase):
    def test_unsupported_type(self):
        with self.assertRaises(TypeError):
            encode(object())

    def test_bool_beats_f32(self):
        # F32 is a float subclass, bool an int subclass; neither may be
        # confused with its parent in dispatch
        self.assertEqual(encode(F32(True)), b"\xca" + struct.pack(">f", 1.0))
        self.assertEqual(encode(True), b"\xc3")


if __name__ == "__main__":
    unittest.main()
