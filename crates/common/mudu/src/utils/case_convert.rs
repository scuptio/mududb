//! Case-conversion helpers built on top of `heck`.

use heck::ToSnakeCase;
use heck::{ToKebabCase, ToPascalCase};

/// Converts `name` to `snake_case`.
pub fn to_snake_case(name: &str) -> String {
    name.to_snake_case()
}

/// Converts `name` to `PascalCase`.
pub fn to_pascal_case(name: &str) -> String {
    name.to_pascal_case()
}

/// Converts `name` to `kebab-case`.
pub fn to_kebab_case(name: &str) -> String {
    name.to_kebab_case()
}

/// Converts `name` to `SCREAMING_SNAKE_CASE`.
pub fn to_snake_case_upper(name: &str) -> String {
    name.to_snake_case().to_uppercase()
}
