#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::utils::case_convert::{
        to_kebab_case, to_pascal_case, to_snake_case, to_snake_case_upper,
    };

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("helloWorld"), "hello_world");
        assert_eq!(to_snake_case("hello_world"), "hello_world");
        assert_eq!(to_snake_case("Hello-World"), "hello_world");
        assert_eq!(to_snake_case("hello"), "hello");
        assert_eq!(to_snake_case(""), "");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("hello-world"), "HelloWorld");
        assert_eq!(to_pascal_case("helloWorld"), "HelloWorld");
        assert_eq!(to_pascal_case("HelloWorld"), "HelloWorld");
        assert_eq!(to_pascal_case("hello"), "Hello");
        assert_eq!(to_pascal_case(""), "");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("HelloWorld"), "hello-world");
        assert_eq!(to_kebab_case("hello_world"), "hello-world");
        assert_eq!(to_kebab_case("helloWorld"), "hello-world");
        assert_eq!(to_kebab_case("hello-world"), "hello-world");
        assert_eq!(to_kebab_case("hello"), "hello");
        assert_eq!(to_kebab_case(""), "");
    }

    #[test]
    fn test_to_snake_case_upper() {
        assert_eq!(to_snake_case_upper("HelloWorld"), "HELLO_WORLD");
        assert_eq!(to_snake_case_upper("hello_world"), "HELLO_WORLD");
        assert_eq!(to_snake_case_upper("hello"), "HELLO");
        assert_eq!(to_snake_case_upper(""), "");
    }
}
