use std::collections::HashMap;

#[derive(Copy, Clone)]
pub enum Operator {
    OValueCompare(ValueCompare),
    OLogicalConnective(LogicalConnective),
    OArithmetic(Arithmetic),
}


#[derive(Clone, Copy, Debug)]
pub enum Arithmetic {
    PLUS,
    MINUS,
    MULTIPLE,
    DIVIDE,
}

#[derive(Copy, Clone, Debug)]
pub enum ValueCompare {
    EQ,
    LE,
    LT,
    GE,
    GT,
    NE,
}

#[derive(Copy, Clone, Debug)]
pub enum LogicalConnective {
    AND,
}

fn name2op(name: String) -> Operator {
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
        op
    } else {
        panic!("Operator {} not found", name);
    };
    op.clone()
}

impl Operator {
    pub fn from_str(name: String) -> Self {
        name2op(name)
    }

    pub fn logical_connect(&self) -> Option<LogicalConnective> {
        match self {
            Operator::OValueCompare(_) => None,
            Operator::OLogicalConnective(c) => Some(c.clone()),
            &Operator::OArithmetic(_) => { panic!("Arithmetic not supported"); }
        }
    }

    pub fn is_logical_and(&self) -> bool {
        match self.logical_connect() {
            None => false,
            Some(c) => match c {
                LogicalConnective::AND => true,
            },
        }
    }
}
