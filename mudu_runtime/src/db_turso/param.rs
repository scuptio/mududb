use turso;

pub struct TursoParam {
    inner: Vec<turso::Value>,
}

impl TursoParam {
    pub fn new(inner: Vec<turso::Value>) -> Self {
        let mut inner = inner;
        inner.reverse();
        Self { inner }
    }
}

impl Iterator for TursoParam {
    type Item = turso::Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.pop()
    }
}


