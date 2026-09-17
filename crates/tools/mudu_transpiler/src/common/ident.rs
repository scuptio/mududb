//! Identifier helpers shared by the guest front-ends.

/// Whether `input` is a valid ASCII identifier (`[A-Za-z_][A-Za-z0-9_]*`):
/// the shape procedure and parameter names must have to survive the
/// snake/kebab/Pascal case conversions into the generated artifacts.
pub fn is_valid_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Sanitize arbitrary text into a safe identifier for generated aliases:
/// every non-identifier character becomes `_`, a leading digit gets a `_`
/// prefix, and an empty result falls back to `"_"`.
pub fn sanitize_identifier(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for (index, ch) in input.chars().enumerate() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            if index == 0 && ch.is_ascii_digit() {
                out.push('_');
            }
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() { "_".to_string() } else { out }
}
