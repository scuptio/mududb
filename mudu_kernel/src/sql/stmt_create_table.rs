use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;

use crate::contract::cmd_exec::CmdExec;
use crate::contract::schema_table::SchemaTable;
use crate::contract::ssn_ctx::SsnCtx;
use crate::meta::meta_mgr::MetaMgrImpl;
use mudu::common::result::RS;

use crate::sql::stmt_cmd::StmtCmd;
use crate::sync::s_mutex::SMutex;
use crate::x_engine::x_param::PCreateTable;
use sql_parser::ast::column_def::ColumnDef;

#[derive(Clone, Debug)]
pub struct StmtCreateTable {
    table_name: String,
    column_def: Vec<ColumnDef>,
    primary_key_column_def: Vec<ColumnDef>,
    non_primary_key_column_def: Vec<ColumnDef>,
    opt_create: Arc<SMutex<Option<PCreateTable>>>,
}

impl StmtCreateTable {
    pub fn new(table_name: String) -> StmtCreateTable {
        Self {
            table_name,
            column_def: vec![],
            primary_key_column_def: vec![],
            non_primary_key_column_def: vec![],
            opt_create: Arc::new(Default::default()),
        }
    }

    pub fn table_name(&self) -> &String {
        &self.table_name
    }

    pub fn add_column_def(&mut self, def: ColumnDef) {
        self.column_def.push(def)
    }

    pub fn mutable_column_def(&mut self) -> &mut Vec<ColumnDef> {
        &mut self.column_def
    }

    pub fn non_primary_columns(&self) -> &Vec<ColumnDef> {
        &self.non_primary_key_column_def
    }

    pub fn primary_columns(&self) -> &Vec<ColumnDef> {
        &self.primary_key_column_def
    }

    pub fn assign_index_for_columns(&mut self) {
        let mut index_non_primary = 0;
        let mut column_def_list = vec![];
        std::mem::swap(&mut self.column_def, &mut column_def_list);
        for mut c in column_def_list {
            if c.is_primary_key() {
                self.primary_key_column_def.push(c);
            } else {
                c.set_index(index_non_primary);
                index_non_primary += 1;
                self.non_primary_key_column_def.push(c);
            }
        }
        self.primary_key_column_def.sort_by(|x, y| {
            return x.column_index().cmp(&y.column_index());
        })
    }

    fn to_table_schema(&self) -> SchemaTable {
        let table_name = self.table_name.clone();
        let mut vec_keys = vec![];
        let mut vec_values = vec![];
        for def in self.primary_columns() {
            let sc = todo!();
            vec_keys.push(sc);
        }

        for def in self.non_primary_columns() {
            let sc = todo!();
            vec_values.push(sc);
        }
        SchemaTable::new(table_name, vec_keys, vec_values)
    }

    pub async fn execute(&self, schema_manager: &MetaMgrImpl) -> RS<()> {
        let schema_table = self.to_table_schema();
        schema_manager._create_table(&schema_table)?;
        Ok(())
    }
}


#[async_trait]
impl StmtCmd for StmtCreateTable {
    async fn realize(&self, ctx: &dyn SsnCtx) -> RS<()> {
        let schema = self.to_table_schema();
        let table_name = schema.table_name().clone();
        todo!();
        /*
        let opt = ctx
            .thd_ctx()
            .meta_mgr()
            .get_table_by_name(&table_name)
            .await?;
        if opt.is_some() {
            return Err(m_error!(ER::ExistingSuchElement, format!(
                "existing a table name {}, cannot create",
                table_name
            )));
        }
        let p = PCreateTable { xid: 0, schema };
        let mut guard = self.opt_create.lock()?;
        *guard = Some(p);
        Ok(())

         */
    }

    async fn build(&self, ctx: &dyn SsnCtx) -> RS<Arc<dyn CmdExec>> {
        let opt = self.opt_create.lock()?;
        let param = opt.as_ref().expect("must be invoked realize");
        todo!();
        /*
        let cmd = CreateTable::new(param.clone(), ctx.thd_ctx().clone());
        Ok(Arc::new(cmd))
         */
    }
}
