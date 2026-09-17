pub(crate) struct ParseContext {
    text: String,
}

impl ParseContext {
    pub(crate) fn new(text: String) -> Self {
        Self { text }
    }

    pub(crate) fn parse_str(&self) -> &str {
        self.text.as_str()
    }
}
