use heck::ToSnakeCase;
use heck::{ToKebabCase, ToPascalCase};

pub fn to_snake_case(name: &String) -> String {
    name.to_snake_case()
}

pub fn to_pascal_case(name: &String) -> String {
    name.to_pascal_case()
}

pub fn to_kebab_case(name: &String) -> String {
    name.to_kebab_case()
}

pub fn to_snake_case_upper(name: &String) -> String {
    name.to_snake_case().to_uppercase()
}
