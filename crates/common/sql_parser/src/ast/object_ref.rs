//! Possibly schema-qualified object reference AST node (`schema.name` or `name`).

use std::fmt::Debug;

/// A table reference as written in a DML/DROP/COPY statement: a bare object
/// name, optionally qualified by a schema name (`schema.table`). The grammar
/// parses the qualifier; name resolution (search path) is a binder concern.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectRef {
    schema: Option<String>,
    name: String,
}

impl ObjectRef {
    /// Create a reference from an optional schema qualifier and the object name.
    pub fn new(schema: Option<String>, name: String) -> Self {
        Self { schema, name }
    }

    /// Create an unqualified reference from the bare object name.
    pub fn unqualified(name: String) -> Self {
        Self { schema: None, name }
    }

    /// Return the schema qualifier, when the reference was written qualified.
    pub fn schema(&self) -> Option<&String> {
        self.schema.as_ref()
    }

    /// Return the bare object name (without the schema qualifier).
    pub fn name(&self) -> &String {
        &self.name
    }
}
