"""Unit tests for `mududb.sql.Params` bind rules and wire form."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from mududb.errors import MuduError
from mududb.generated.uni_scalar_value import (
    UniScalarValueBool,
    UniScalarValueF64,
    UniScalarValueI64,
    UniScalarValueNull,
    UniScalarValueString,
)
from mududb.sql import Params, to_uni_data_value


class TestToUniDataValue(unittest.TestCase):
    def test_plain_types(self):
        self.assertIsInstance(to_uni_data_value(True).inner, UniScalarValueBool)
        self.assertIsInstance(to_uni_data_value(42).inner, UniScalarValueI64)
        self.assertIsInstance(to_uni_data_value(1.5).inner, UniScalarValueF64)
        self.assertIsInstance(to_uni_data_value("x").inner, UniScalarValueString)
        self.assertIsInstance(to_uni_data_value(None).inner, UniScalarValueNull)

    def test_bool_is_not_int(self):
        # Python bool is an int subclass; it must map to Bool, not I64.
        value = to_uni_data_value(True)
        self.assertIsInstance(value.inner, UniScalarValueBool)


class TestParams(unittest.TestCase):
    def test_positional_contiguous(self):
        p = Params().bind(0, 1).bind(1, "a")
        param = p.to_uni_sql_param()
        self.assertEqual(len(param.params), 2)
        self.assertIsNone(param.param_names)

    def test_positional_gap_rejected(self):
        with self.assertRaises(MuduError):
            Params().bind(1, 1)

    def test_named(self):
        p = Params().bind_named("name", "carol").bind_named("id", 2)
        param = p.to_uni_sql_param()
        self.assertEqual(param.param_names, ["name", "id"])
        self.assertEqual(len(param.params), 2)

    def test_mixing_rejected_both_ways(self):
        with self.assertRaises(MuduError):
            Params().bind(0, 1).bind_named("a", 1)
        with self.assertRaises(MuduError):
            Params().bind_named("a", 1).bind(0, 1)


if __name__ == "__main__":
    unittest.main()
