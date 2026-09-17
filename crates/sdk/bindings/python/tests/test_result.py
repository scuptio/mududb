"""Unit tests for `mududb.result` cursor semantics."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from mududb.errors import MuduError
from mududb.generated.uni_data_value import UniDataValueScalar
from mududb.generated.uni_query_result import UniQueryResult
from mududb.generated.uni_record_type import UniRecordField, UniRecordType
from mududb.generated.uni_result_set import UniResultSet
from mududb.generated.uni_scalar_value import UniScalarValueI64, UniScalarValueNull, UniScalarValueString
from mududb.generated.uni_tuple_row import UniTupleRow
from mududb.result import ResultSet, as_i64, as_string


def _i64(v):
    return UniDataValueScalar(inner=UniScalarValueI64(inner=v))


def _text(v):
    return UniDataValueScalar(inner=UniScalarValueString(inner=v))


def _sample():
    return UniQueryResult(
        tuple_desc=UniRecordType(
            record_fields=[UniRecordField(field_name="id"), UniRecordField(field_name="name")]
        ),
        result_set=UniResultSet(
            eof=True,
            row_set=[
                UniTupleRow(fields=[_i64(1), _text("alice")]),
                UniTupleRow(fields=[_i64(2), _text("bob")]),
            ],
        ),
    )


class TestResultSet(unittest.TestCase):
    def test_cursor_semantics(self):
        rs = ResultSet(_sample())
        self.assertEqual(rs.column_count(), 2)
        self.assertEqual(rs.column_name(0), "id")
        self.assertEqual(rs.find_column("name"), 1)
        with self.assertRaises(MuduError):
            rs.current_row()

        self.assertTrue(rs.next())
        row = rs.current_row()
        self.assertEqual(row.value(0).inner.inner, 1)
        self.assertEqual(row.value_by_name("name").inner.inner, "alice")
        self.assertFalse(row.is_null(0))

        self.assertTrue(rs.next())
        # AS semantics: eof() is already true once the cursor reached the
        # last row (cursor >= len), even though current_row() is still valid.
        self.assertTrue(rs.eof())
        row = rs.current_row()
        self.assertEqual(row.value_by_name("id").inner.inner, 2)

        self.assertFalse(rs.next())
        self.assertTrue(rs.eof())
        # A failed next() leaves the cursor in place, so the last row is
        # still current (AS semantics).
        row = rs.current_row()
        self.assertEqual(row.value_by_name("id").inner.inner, 2)

    def test_out_of_range_and_unknown_name(self):
        rs = ResultSet(_sample())
        with self.assertRaises(MuduError):
            rs.column_name(5)
        with self.assertRaises(MuduError):
            rs.find_column("nope")
        rs.next()
        row = rs.current_row()
        with self.assertRaises(MuduError):
            row.value(9)
        with self.assertRaises(MuduError):
            row.value_by_name("nope")

    def test_null_detection(self):
        sample = _sample()
        sample.result_set.row_set[0].fields[1] = UniDataValueScalar(inner=UniScalarValueNull())
        rs = ResultSet(sample)
        rs.next()
        row = rs.current_row()
        self.assertTrue(row.is_null(1))
        self.assertTrue(row.is_null_by_name("name"))
        self.assertFalse(row.is_null(0))

    def test_typed_accessors(self):
        rs = ResultSet(_sample())
        rs.next()
        row = rs.current_row()
        self.assertEqual(as_i64(row.value(0)), 1)
        self.assertEqual(as_string(row.value(1)), "alice")
        with self.assertRaises(MuduError):
            as_string(row.value(0))
        sample = _sample()
        sample.result_set.row_set[0].fields[0] = UniDataValueScalar(inner=UniScalarValueNull())
        rs = ResultSet(sample)
        rs.next()
        with self.assertRaises(MuduError):
            as_i64(rs.current_row().value(0))


if __name__ == "__main__":
    unittest.main()
