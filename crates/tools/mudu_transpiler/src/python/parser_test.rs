use super::discover_procedures;
use crate::python::procedure::{PyParam, PyProcedure, PyValueType, normalize_hint};
use mudu::error::ErrorCode;
use std::error::Error;

#[test]
fn discovers_marked_procedure() -> Result<(), Box<dyn Error>> {
    let code = r#"
# mudu-proc
def transfer(account1: int, account2: int) -> int:
    return 0
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(
        procs,
        vec![PyProcedure {
            name: "transfer".to_string(),
            params: vec![
                PyParam {
                    name: "account1".to_string(),
                    ty: Some("int".to_string()),
                    value_type: PyValueType::Int64,
                },
                PyParam {
                    name: "account2".to_string(),
                    ty: Some("int".to_string()),
                    value_type: PyValueType::Int64,
                },
            ],
            session_arg: None,
            return_type: Some("int".to_string()),
            return_value_types: vec![PyValueType::Int64],
        }]
    );
    Ok(())
}

#[test]
fn detects_annotated_first_session_parameter() -> Result<(), Box<dyn Error>> {
    let code = r#"
# mudu-proc
def create_user(session: UniOid, user_id: int, name: str) -> int:
    return user_id
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(
        procs,
        vec![PyProcedure {
            name: "create_user".to_string(),
            params: vec![
                PyParam {
                    name: "session".to_string(),
                    ty: Some("UniOid".to_string()),
                    value_type: PyValueType::ObjectId,
                },
                PyParam {
                    name: "user_id".to_string(),
                    ty: Some("int".to_string()),
                    value_type: PyValueType::Int64,
                },
                PyParam {
                    name: "name".to_string(),
                    ty: Some("str".to_string()),
                    value_type: PyValueType::Text,
                },
            ],
            session_arg: Some("session".to_string()),
            return_type: Some("int".to_string()),
            return_value_types: vec![PyValueType::Int64],
        }]
    );
    Ok(())
}

#[test]
fn detects_unannotated_first_session_parameter() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef probe(session, x):\n    return x\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].session_arg, Some("session".to_string()));
    assert_eq!(procs[0].params.len(), 2);
    Ok(())
}

#[test]
fn session_in_non_first_position_stays_positional() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef f(x: int, session: UniOid):\n    pass\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].session_arg, None);
    Ok(())
}

#[test]
fn non_oid_annotated_first_session_stays_positional() -> Result<(), Box<dyn Error>> {
    // Mixed convention: the name says session but the hint says a value
    // type — treat it as an ordinary positional parameter.
    let code = "# mudu-proc\ndef f(session: int, x: int):\n    pass\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].session_arg, None);
    Ok(())
}

#[test]
fn marker_allows_leading_whitespace_and_blank_lines() -> Result<(), Box<dyn Error>> {
    let code = "    # mudu-proc\n\n\ndef add_one(x):\n    return x + 1\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "add_one");
    assert_eq!(
        procs[0].params,
        vec![PyParam {
            name: "x".to_string(),
            ty: None,
            value_type: PyValueType::Any,
        }]
    );
    assert_eq!(procs[0].return_type, None);
    assert_eq!(procs[0].return_value_types, vec![PyValueType::Any]);
    Ok(())
}

#[test]
fn ignores_unlabeled_function_after_labeled_function() -> Result<(), Box<dyn Error>> {
    let code = r#"
# mudu-proc
def transfer(amount: int) -> int:
    return amount


def helper(amount: int) -> int:
    return amount
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "transfer");
    Ok(())
}

#[test]
fn label_does_not_cross_a_statement() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\nx = 1\ndef not_a_proc():\n    pass\n";
    let procs = discover_procedures(code, None)?;
    assert!(procs.is_empty());
    Ok(())
}

#[test]
fn discovers_multiple_procedures_in_order() -> Result<(), Box<dyn Error>> {
    let code = r#"
# mudu-proc
def first(a):
    return a


# mudu-proc
def second(b):
    return b


def third(c):
    return c
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(
        procs.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    Ok(())
}

#[test]
fn keeps_snake_case_names() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef transfer_funds(from_account, to_account):\n    pass\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "transfer_funds");
    assert_eq!(
        procs
            .iter()
            .flat_map(|p| p.params.iter().map(|param| param.name.as_str()))
            .collect::<Vec<_>>(),
        vec!["from_account", "to_account"]
    );
    Ok(())
}

#[test]
fn maps_recognized_type_hints() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef f(a: int, b: str, c: bool, d: float, e: bytes, f: Oid, g: list, h):\n    pass\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(
        procs
            .iter()
            .flat_map(|p| p.params.iter().map(|param| param.value_type.clone()))
            .collect::<Vec<_>>(),
        vec![
            PyValueType::Int64,
            PyValueType::Text,
            PyValueType::Boolean,
            PyValueType::Float64,
            PyValueType::Binary,
            PyValueType::ObjectId,
            PyValueType::Any,
            PyValueType::Any,
        ]
    );
    Ok(())
}

#[test]
fn typed_default_parameter_keeps_its_hint() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef f(name: str = \"x\", count=1):\n    pass\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(
        procs[0].params,
        vec![
            PyParam {
                name: "name".to_string(),
                ty: Some("str".to_string()),
                value_type: PyValueType::Text,
            },
            PyParam {
                name: "count".to_string(),
                ty: None,
                value_type: PyValueType::Any,
            },
        ]
    );
    Ok(())
}

#[test]
fn return_arity_follows_the_annotation() -> Result<(), Box<dyn Error>> {
    let cases: Vec<(&str, Vec<PyValueType>)> = vec![
        ("def f():\n    pass\n", vec![PyValueType::Any]),
        ("def f() -> None:\n    pass\n", vec![]),
        ("def f() -> int:\n    return 0\n", vec![PyValueType::Int64]),
        (
            "def f() -> tuple[int, str]:\n    pass\n",
            vec![PyValueType::Int64, PyValueType::Text],
        ),
        (
            "def f() -> tuple[int, list[str]]:\n    pass\n",
            vec![PyValueType::Int64, PyValueType::Any],
        ),
        (
            "def f() -> SomethingElse:\n    pass\n",
            vec![PyValueType::Any],
        ),
    ];
    for (body, expected) in cases {
        let code = format!("# mudu-proc\n{body}");
        let procs = discover_procedures(&code, None)?;
        assert_eq!(
            procs[0].return_value_types, expected,
            "return arity mismatch for {body:?}"
        );
    }
    Ok(())
}

#[test]
fn discovers_marked_decorated_definition() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\n@staticmethod\ndef decorated(x: int):\n    return x\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "decorated");
    Ok(())
}

#[test]
fn ignores_nested_functions() -> Result<(), Box<dyn Error>> {
    let code = r#"
def outer():
    # mudu-proc
    def inner(x):
        return x
    return inner
"#;
    let procs = discover_procedures(code, None)?;
    assert!(procs.is_empty());
    Ok(())
}

#[test]
fn rejects_syntax_error() -> Result<(), Box<dyn Error>> {
    let code = "def broken(:\n";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_duplicate_procedure_names() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef dup(x):\n    pass\n# mudu-proc\ndef dup(y):\n    pass\n";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_splat_parameters() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef f(x, *args):\n    pass\n";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    let code = "# mudu-proc\ndef f(x, **kwargs):\n    pass\n";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn normalize_hint_trims_and_lowercases() {
    assert_eq!(normalize_hint("  Str "), "str");
    assert_eq!(normalize_hint("Optional[int]"), "optional[int]");
}

// ---- user-defined types (`--type-wit`) ----

use crate::common::type_registry::TypeRegistry;
use std::time::UNIX_EPOCH;

const SHOP_TYPES_WIT: &str = r#"
interface shop-types {
    record address {
        city: string,
        zip: string,
    }
    record profile {
        display-name: string,
        level: u32,
        vip: bool,
        tags: list<string>,
        home: option<address>,
    }
    enum mode {
        basic,
        pro,
    }
    variant shape {
        circle(f32),
        point,
    }
}
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_py_types_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    let path = dir.join("types.wit");
    mudu_sys::fs::sync::sync_write(&path, SHOP_TYPES_WIT)?;
    Ok(TypeRegistry::from_wit_paths(&[path
        .to_str()
        .ok_or("invalid UTF-8 in path")?
        .to_string()])?)
}

#[test]
fn resolves_record_enum_and_option_hints() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = r#"
# mudu-proc
def update_profile(session: UniOid, user_id: int, profile: Profile, note: Optional[str], mode: Mode, home: Address | None) -> Profile:
    return profile
"#;
    let procs = discover_procedures(code, Some(&registry))?;
    assert_eq!(procs.len(), 1);
    let params = &procs[0].params;
    assert_eq!(params[1].value_type, PyValueType::Int64);
    match &params[2].value_type {
        PyValueType::Record(custom) => {
            assert_eq!(custom.name, "Profile");
            assert_eq!(custom.name_snake, "profile");
        }
        other => return Err(format!("expected a record type, got {other:?}").into()),
    }
    assert_eq!(
        params[3].value_type,
        PyValueType::Option(Box::new(PyValueType::Text))
    );
    assert!(params[3].value_type.is_nullable());
    match &params[4].value_type {
        PyValueType::Enum(custom) => {
            assert_eq!(custom.name, "Mode");
            assert_eq!(custom.name_snake, "mode");
        }
        other => return Err(format!("expected an enum type, got {other:?}").into()),
    }
    match &params[5].value_type {
        PyValueType::Option(inner) => match inner.as_ref() {
            PyValueType::Record(custom) => {
                assert_eq!(custom.name, "Address");
                assert_eq!(custom.name_snake, "address");
            }
            other => return Err(format!("expected an option of record, got {other:?}").into()),
        },
        other => return Err(format!("expected an option type, got {other:?}").into()),
    }
    assert!(params[5].value_type.is_nullable());
    match &procs[0].return_value_types[0] {
        PyValueType::Record(custom) => assert_eq!(custom.name, "Profile"),
        other => return Err(format!("expected a record return type, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn resolves_typing_optional_and_none_left_union() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = r#"
# mudu-proc
def f(a: typing.Optional[int], b: None | Profile, c: Optional[Optional[int]], d: Optional[Whatever]) -> None:
    pass
"#;
    let procs = discover_procedures(code, Some(&registry))?;
    let params = &procs[0].params;
    assert_eq!(
        params[0].value_type,
        PyValueType::Option(Box::new(PyValueType::Int64))
    );
    match &params[1].value_type {
        PyValueType::Option(inner) => {
            assert!(matches!(inner.as_ref(), PyValueType::Record(_)));
        }
        other => return Err(format!("expected an option of record, got {other:?}").into()),
    }
    // A nested option and an option of an unresolvable hint keep the
    // permissive default.
    assert_eq!(params[2].value_type, PyValueType::Any);
    assert_eq!(params[3].value_type, PyValueType::Any);
    assert_eq!(procs[0].return_value_types, vec![]);
    Ok(())
}

#[test]
fn unknown_hint_stays_permissive_without_a_registry() -> Result<(), Box<dyn Error>> {
    let code = "# mudu-proc\ndef f(profile: Profile) -> Profile:\n    return profile\n";
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs[0].params[0].value_type, PyValueType::Any);
    assert_eq!(procs[0].return_value_types, vec![PyValueType::Any]);
    Ok(())
}

#[test]
fn variant_hint_is_a_hard_error() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = "# mudu-proc\ndef f(shape: Shape):\n    pass\n";
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a variant error")?;
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    Ok(())
}

#[test]
fn enum_and_option_returns_are_rejected() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = "# mudu-proc\ndef f() -> Mode:\n    return 0\n";
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected an enum return error")?;
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    let code = "# mudu-proc\ndef f() -> Optional[int]:\n    return 0\n";
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected an option return error")?;
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    Ok(())
}
