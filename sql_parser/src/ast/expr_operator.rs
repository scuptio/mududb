use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;

/// SQL operator enum.
#[derive(Copy, Clone)]
pub enum Operator {
    /// Value comparison operator (`=`, `<`, `>`, etc.).
    OValueCompare(ValueCompare),
    /// Logical connective operator (`AND`).
    OLogicalConnective(LogicalConnective),
    /// Arithmetic operator (`+`, `-`, `*`, `/`).
    OArithmetic(Arithmetic),
}

/// Arithmetic operators.
#[derive(Clone, Copy, Debug)]
pub enum Arithmetic {
    /// Addition (`+`).
    PLUS,
    /// Subtraction (`-`).
    MINUS,
    /// Multiplication (`*`).
    MULTIPLE,
    /// Division (`/`).
    DIVIDE,
}

/// Value comparison operators.
#[derive(Copy, Clone, Debug)]
pub enum ValueCompare {
    /// Equal (`=`).
    EQ,
    /// Less than or equal (`<=`).
    LE,
    /// Less than (`<`).
    LT,
    /// Greater than or equal (`>=`).
    GE,
    /// Greater than (`>`).
    GT,
    /// Not equal (`!=`).
    NE,
}

/// Logical connective operators.
#[derive(Copy, Clone, Debug)]
pub enum LogicalConnective {
    /// Logical AND.
    AND,
}

fn name2op(name: String) -> RS<Operator> {
    let array = [
        ("=", Operator::OValueCompare(ValueCompare::EQ)),
        ("<", Operator::OValueCompare(ValueCompare::LT)),
        ("<=", Operator::OValueCompare(ValueCompare::LE)),
        (">", Operator::OValueCompare(ValueCompare::GT)),
        (">=", Operator::OValueCompare(ValueCompare::GE)),
        ("!=", Operator::OValueCompare(ValueCompare::NE)),
        ("AND", Operator::OLogicalConnective(LogicalConnective::AND)),
        ("-", Operator::OArithmetic(Arithmetic::MINUS)),
        ("+", Operator::OArithmetic(Arithmetic::PLUS)),
        ("*", Operator::OArithmetic(Arithmetic::MULTIPLE)),
        ("/", Operator::OArithmetic(Arithmetic::DIVIDE)),
    ];
    let map = HashMap::from(array);
    let opt_op = map.get(name.as_str());
    let op = if let Some(op) = opt_op {
        *op
    } else {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("operator {} not found", name)
        ));
    };
    Ok(op)
}

impl Operator {
    /// Parse an operator from its SQL symbol or keyword name.
    pub fn from_name(name: String) -> RS<Self> {
        name2op(name)
    }

    /// Return the logical connective variant, if any.
    pub fn logical_connect(&self) -> Option<LogicalConnective> {
        match self {
            Operator::OValueCompare(_) => None,
            Operator::OLogicalConnective(c) => Some(*c),
            &Operator::OArithmetic(_) => None,
        }
    }

    /// Return `true` if this operator is a logical `AND`.
    pub fn is_logical_and(&self) -> bool {
        match self.logical_connect() {
            None => false,
            Some(c) => match c {
                LogicalConnective::AND => true,
            },
        }
    }
}

impl ValueCompare {
    /// Reverse the comparison direction.
    ///
    /// Used to normalize `literal OP field` into `field OP' literal`.
    pub fn revert_cmp_op(op: ValueCompare) -> ValueCompare {
        match op {
            ValueCompare::EQ => ValueCompare::EQ,
            ValueCompare::LE => ValueCompare::GT,
            ValueCompare::LT => ValueCompare::GE,
            ValueCompare::GE => ValueCompare::LT,
            ValueCompare::GT => ValueCompare::LE,
            ValueCompare::NE => ValueCompare::NE,
        }
    }
}
