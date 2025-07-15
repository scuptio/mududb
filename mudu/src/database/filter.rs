use crate::tuple::datum::Datum;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum OpType {
    Equal,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
}

pub struct Filter {
    table_name:&'static str,
    column_name:&'static str,
    op_type: OpType,
    datum: Datum,
}

impl Filter {
    pub fn new(
        table_name:&'static str, 
        column_name:&'static str, 
        op_type: OpType, 
        datum:Datum
    ) -> Filter {
        Self {
            table_name,
            column_name,
            op_type,
            datum,
        }
    }
    pub fn op_type(&self) -> OpType {
        self.op_type
    }
    
    pub fn table_name(&self) -> &'static &str {
        &self.table_name
    }
    
    pub fn column_name(&self) ->  &'static &str {
        &self.column_name
    }
    
    pub fn datum(&self) -> &Datum {
        &self.datum
    }
}