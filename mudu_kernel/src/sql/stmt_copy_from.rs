use crate::command::load_from_file::LoadFromFile;
use crate::contract::cmd_exec::CmdExec;
use crate::contract::ssn_ctx::SsnCtx;
use crate::sql::stmt_cmd::StmtCmd;
use crate::sync::s_mutex::SMutex;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::tuple::tuple_field_desc::TupleFieldDesc as TupleDesc;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct StmtCopyFrom {
    from_file_path: String,
    table: String,
    columns: Vec<String>,
    opt_load_param: Arc<SMutex<Option<LoadParam>>>,
}

#[derive(Clone, Debug)]
struct LoadParam {
    csv_file: String,
    table_id: OID,
    key_index: Vec<usize>,
    value_index: Vec<usize>,
    key_desc: TupleDesc,
    value_desc: TupleDesc,
}


#[async_trait]
impl StmtCmd for StmtCopyFrom {
    async fn realize(&self, ctx: &dyn SsnCtx) -> RS<()> {
        self.build_param(ctx).await
    }

    async fn build(&self, ctx: &dyn SsnCtx) -> RS<Arc<dyn CmdExec>> {
        self.build_cmd(ctx).await
    }
}

impl StmtCopyFrom {
    pub fn new(from_file_path: String, table: String, columns: Vec<String>) -> Self {
        Self {
            from_file_path,
            table,
            columns,
            opt_load_param: Arc::new(Default::default()),
        }
    }

    fn build_copy_to_cmd(p: LoadParam, thd_ctx: &dyn SsnCtx) -> LoadFromFile {
        todo!()
        /*
        LoadFromFile::new(
            p.csv_file,
            p.table_id,
            p.key_index,
            p.value_index,
            p.key_desc,
            p.value_desc,
            thd_ctx.thd_ctx().clone(),
        )

         */
    }
    async fn build_cmd(&self, ctx: &dyn SsnCtx) -> RS<Arc<dyn CmdExec>> {
        let param = self
            .opt_load_param
            .lock()?
            .as_ref()
            .expect("must invoke release first")
            .clone();
        let cmd = Self::build_copy_to_cmd(param, ctx);
        Ok(Arc::new(cmd))
    }

    async fn build_param(&self, ctx: &dyn SsnCtx) -> RS<()> {
        todo!()
        /*
        let opt_table = ctx
            .thd_ctx()
            .meta_mgr()
            .get_table_by_name(&self.table)
            .await?;
        let table_desc = match opt_table {
            Some(table) => table,
            None => return Err(ER::NoSuchTable(format!("cannot find table {}", self.table))),
        };
        let table_id = table_desc.id();

        let columns = if self.columns.is_empty() {
            let mut col_name_and_index: Vec<(String, usize)> = Vec::from_iter(
                table_desc
                    .oid2col()
                    .iter()
                    .map(|(_k, v)| (v.name().clone(), v.column_index())),
            );
            col_name_and_index.sort_by(|a, b| a.1.cmp(&b.1));
            col_name_and_index.into_iter().map(|(n, _i)| n).collect()
        } else if self.columns.len() == table_desc.oid2col().len() {
            self.columns.clone()
        } else {
            return Err(ER::TableInfoError(format!(
                "the columns of table {} is not equal to the size specified  {}",
                self.columns.len(),
                table_desc.oid2col().len()
            )));
        };

        let mut map = HashMap::new();
        for (i, name) in columns.iter().enumerate() {
            map.insert(name.to_string(), i);
        }

        let mut key_index = vec![];
        let mut value_index = vec![];
        for (vec_index, vec_oid) in [
            (&mut key_index, table_desc.key_field_oid()),
            (&mut value_index, table_desc.value_field_oid()),
        ] {
            for id in vec_oid {
                let opt = table_desc.oid2col().get(id);
                let info = match opt {
                    Some(i) => i,
                    None => {
                        panic!("cannot find column oid {}", id);
                    }
                };
                let opt = map.get(info.name());
                let index = match opt {
                    Some(i) => *i,
                    None => {
                        panic!("cannot find column name {}", id);
                    }
                };
                vec_index.push(index);
            }
        }

        let key_desc = table_desc.key_desc().clone();
        let value_desc = table_desc.value_desc().clone();
        let param = LoadParam {
            csv_file: self.from_file_path.clone(),
            table_id,
            key_index,
            value_index,
            key_desc,
            value_desc,
        };
        let mut guard = self.opt_load_param.lock()?;
        *guard = Some(param);
        Ok(())
         */
    }
}
