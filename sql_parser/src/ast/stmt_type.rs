use crate::ast::stmt_copy_from::StmtCopyFrom;
use crate::ast::stmt_copy_to::StmtCopyTo;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_delete::StmtDelete;
use crate::ast::stmt_drop_table::StmtDropTable;
use crate::ast::stmt_insert::StmtInsert;
use crate::ast::stmt_select::StmtSelect;
use crate::ast::stmt_update::StmtUpdate;

#[derive(Clone, Debug)]
pub enum StmtType {
    Select(StmtSelect),
    Command(StmtCommand),
}

#[derive(Clone, Debug)]
pub enum StmtCommand {
    Update(StmtUpdate),
    Delete(StmtDelete),
    Insert(StmtInsert),
    CreateTable(StmtCreateTable),
    DropTable(StmtDropTable),
    CopyTo(StmtCopyTo),
    CopyFrom(StmtCopyFrom),
}
