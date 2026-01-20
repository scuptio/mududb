use libsql;

pub struct LibSQLParam {
    inner: Vec<libsql::Value>,
}

impl LibSQLParam {
    pub fn new(inner: Vec<libsql::Value>) -> Self {
        let mut inner = inner;
        inner.reverse();
        Self { inner }
    }
}

impl Iterator for LibSQLParam {
    type Item = libsql::Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.pop()
    }
}


