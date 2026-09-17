"""Unit tests for `mududb.codec.bridge`: the uni-data-value record envelope
<-> positional wire-value model conversions used by procedure adapters for
user-defined (mgen-generated) record types."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from mududb.codec.bridge import record_field_values, record_from_field_values
from mududb.codec.mpack import F32
from mududb.generated.uni_data_value import (
    UniDataValueArray,
    UniDataValueBinary,
    UniDataValueField,
    UniDataValueRecord,
    UniDataValueScalar,
)
from mududb.generated.uni_scalar_value import (
    UniScalarValueBlob,
    UniScalarValueBool,
    UniScalarValueF32,
    UniScalarValueF64,
    UniScalarValueI32,
    UniScalarValueI64,
    UniScalarValueNull,
    UniScalarValueString,
    UniScalarValueU128,
)


def _scalar(inner):
    return UniDataValueScalar(inner=inner)


def _field(value, name=""):
    return UniDataValueField(field_name=name, field_value=value)


class TestRecordFieldValues(unittest.TestCase):
    def test_unwraps_positional_field_map(self):
        uv = UniDataValueRecord(
            inner=[
                _field(_scalar(UniScalarValueString(inner="Ada"))),
                _field(_scalar(UniScalarValueI32(inner=7))),
                _field(_scalar(UniScalarValueBool(inner=True))),
            ]
        )
        self.assertEqual(record_field_values(uv), {1: "Ada", 2: 7, 3: True})

    def test_field_names_are_ignored(self):
        # The host drops field names (positional semantics); a guest that
        # sends names decodes identically.
        uv = UniDataValueRecord(
            inner=[_field(_scalar(UniScalarValueI64(inner=3)), name="left-over")]
        )
        self.assertEqual(record_field_values(uv), {1: 3})

    def test_nested_shapes_recurse(self):
        home = UniDataValueRecord(
            inner=[
                _field(_scalar(UniScalarValueString(inner="sh"))),
                _field(_scalar(UniScalarValueString(inner="200"))),
            ]
        )
        uv = UniDataValueRecord(
            inner=[
                _field(
                    UniDataValueArray(
                        inner=[
                            _scalar(UniScalarValueString(inner="a")),
                            _scalar(UniScalarValueString(inner="b")),
                        ]
                    )
                ),
                _field(home),
                _field(UniDataValueBinary(inner=b"\x01\x02")),
                _field(_scalar(UniScalarValueNull())),
            ]
        )
        self.assertEqual(
            record_field_values(uv),
            {1: ["a", "b"], 2: {1: "sh", 2: "200"}, 3: b"\x01\x02", 4: None},
        )

    def test_scalar_widths_and_128bit(self):
        uv = UniDataValueRecord(
            inner=[
                _field(_scalar(UniScalarValueF32(inner=1.5))),
                _field(_scalar(UniScalarValueF64(inner=-2.5))),
                _field(_scalar(UniScalarValueU128(inner=(300).to_bytes(16, "big")))),
                _field(_scalar(UniScalarValueBlob(inner=b"\xff"))),
            ]
        )
        self.assertEqual(
            record_field_values(uv), {1: 1.5, 2: -2.5, 3: 300, 4: b"\xff"}
        )

    def test_non_record_rejected(self):
        with self.assertRaises(TypeError):
            record_field_values(_scalar(UniScalarValueI32(inner=1)))
        with self.assertRaises(TypeError):
            record_field_values(UniDataValueArray(inner=[]))
        with self.assertRaises(TypeError):
            record_field_values({1: "not an envelope"})


class TestRecordFromFieldValues(unittest.TestCase):
    def test_wraps_positional_map(self):
        uv = record_from_field_values({1: "Ada", 2: 7, 3: True})
        self.assertIsInstance(uv, UniDataValueRecord)
        self.assertEqual(len(uv.inner), 3)
        for field in uv.inner:
            # the wire is positional: names are empty, like the host emits
            self.assertEqual(field.field_name, "")
        self.assertEqual(
            [f.field_value for f in uv.inner],
            [
                _scalar(UniScalarValueString(inner="Ada")),
                _scalar(UniScalarValueI64(inner=7)),
                _scalar(UniScalarValueI32(inner=1)),
            ],
        )

    def test_bool_crosses_as_i32(self):
        # The host's transactable vocabulary has no Bool: bool fields must
        # encode as the I32 0/1 form.
        uv = record_from_field_values({1: True, 2: False})
        self.assertEqual(uv.inner[0].field_value, _scalar(UniScalarValueI32(inner=1)))
        self.assertEqual(uv.inner[1].field_value, _scalar(UniScalarValueI32(inner=0)))

    def test_gap_fill_middle_omissions_with_null(self):
        uv = record_from_field_values({1: "a", 3: 5})
        self.assertEqual(
            [f.field_value for f in uv.inner],
            [
                _scalar(UniScalarValueString(inner="a")),
                _scalar(UniScalarValueNull()),
                _scalar(UniScalarValueI64(inner=5)),
            ],
        )

    def test_trailing_omission_shrinks(self):
        # An omitted trailing option slot cannot be recovered positionally.
        uv = record_from_field_values({1: "a", 2: 1})
        self.assertEqual(len(uv.inner), 2)

    def test_key_zero_skipped(self):
        uv = record_from_field_values({0: "bogus", 1: "a"})
        self.assertEqual(len(uv.inner), 1)
        self.assertEqual(uv.inner[0].field_value, _scalar(UniScalarValueString(inner="a")))

    def test_invalid_keys_rejected(self):
        with self.assertRaises(ValueError):
            record_from_field_values({-1: "a"})
        with self.assertRaises(ValueError):
            record_from_field_values({"1": "a"})
        with self.assertRaises(ValueError):
            record_from_field_values({True: "a"})

    def test_list_input_is_positional(self):
        uv = record_from_field_values(["a", None, 2])
        self.assertEqual(
            [f.field_value for f in uv.inner],
            [
                _scalar(UniScalarValueString(inner="a")),
                _scalar(UniScalarValueNull()),
                _scalar(UniScalarValueI64(inner=2)),
            ],
        )

    def test_nested_shapes_recurse(self):
        uv = record_from_field_values(
            {
                1: ["a", "b"],
                2: {1: "sh", 2: "200"},
                3: b"\x01",
                4: 1.5,
                5: F32(0.5),
            }
        )
        tags, home, blob, f64, f32 = [f.field_value for f in uv.inner]
        self.assertEqual(
            tags,
            UniDataValueArray(
                inner=[
                    _scalar(UniScalarValueString(inner="a")),
                    _scalar(UniScalarValueString(inner="b")),
                ]
            ),
        )
        self.assertIsInstance(home, UniDataValueRecord)
        self.assertEqual(
            [f.field_value for f in home.inner],
            [
                _scalar(UniScalarValueString(inner="sh")),
                _scalar(UniScalarValueString(inner="200")),
            ],
        )
        self.assertEqual(blob, _scalar(UniScalarValueBlob(inner=b"\x01")))
        self.assertEqual(f64, _scalar(UniScalarValueF64(inner=1.5)))
        self.assertEqual(f32, _scalar(UniScalarValueF32(inner=0.5)))

    def test_unsupported_values_rejected(self):
        with self.assertRaises(TypeError):
            record_from_field_values("not a map")
        with self.assertRaises(TypeError):
            record_from_field_values({1: object()})

    def test_roundtrip(self):
        values = {
            1: "Ada",
            2: 7,
            3: True,
            4: ["a", "b"],
            5: {1: "sh", 2: "200"},
            6: None,
        }
        self.assertEqual(
            record_field_values(record_from_field_values(values)), values
        )


if __name__ == "__main__":
    unittest.main()
