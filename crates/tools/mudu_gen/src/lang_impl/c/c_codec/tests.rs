use super::holder::CHolderKind;
use super::{
    C_STYLE_FUNC, C_STYLE_RECORD, CollectingCNamer, c_decode_stmts, c_encode_stmts, c_ident, c_type,
};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

fn encode(ty: &UniDataType, style: super::CCodecStyle) -> Vec<String> {
    let mut out = Vec::new();
    c_encode_stmts(style, ty, "value->x", 4, 0, &mut out).unwrap();
    out
}

fn decode(ty: &UniDataType, style: super::CCodecStyle) -> Vec<String> {
    let mut out = Vec::new();
    let mut namer = CollectingCNamer::new();
    namer.begin("test_pos".to_string());
    c_decode_stmts(style, ty, "out->x", 4, 0, &mut out, &mut namer).unwrap();
    out
}

#[test]
fn record_context_binary_encodes_as_u8_array() {
    let lines = encode(&UniDataType::Binary, C_STYLE_RECORD);
    let joined = lines.join("\n");
    assert!(joined.contains("mpw_array_header(w, (value->x).len);"));
    assert!(joined.contains("mpw_u64(w, (value->x).data[i0]);"));
    assert!(!joined.contains("mpw_bin"));
}

#[test]
fn func_context_binary_encodes_as_bin() {
    let lines = encode(&UniDataType::Binary, C_STYLE_FUNC);
    assert_eq!(
        lines,
        vec!["    mpw_bin(w, (value->x).data, (value->x).len);".to_string()]
    );
}

#[test]
fn scalars_use_explicit_methods() {
    let lines = encode(&UniDataType::Scalar(UniScalar::U32), C_STYLE_RECORD);
    assert_eq!(
        lines,
        vec!["    mpw_u64(w, (uint64_t)(value->x));".to_string()]
    );
    let lines = encode(&UniDataType::Scalar(UniScalar::Bool), C_STYLE_RECORD);
    assert_eq!(lines, vec!["    mpw_bool(w, value->x);".to_string()]);
    let lines = encode(&UniDataType::Scalar(UniScalar::F32), C_STYLE_RECORD);
    assert_eq!(lines, vec!["    mpw_f32(w, value->x);".to_string()]);
}

#[test]
fn record_context_binary_decodes_from_u8_array_into_the_arena() {
    let joined = decode(&UniDataType::Binary, C_STYLE_RECORD).join("\n");
    assert!(joined.contains("uint32_t n0 = mpr_array_header(r);"));
    assert!(joined.contains("uint8_t *p0 = (uint8_t *)mpa_alloc(a, n0);"));
    assert!(joined.contains("p0[i0] = (uint8_t)mpr_u64(r);"));
    assert!(joined.contains("(out->x).data = p0;"));
}

#[test]
fn func_context_binary_decodes_from_bin_into_the_arena() {
    let joined = decode(&UniDataType::Binary, C_STYLE_FUNC).join("\n");
    assert!(joined.contains("const uint8_t *s0 = mpr_bin(r, &n0);"));
    assert!(joined.contains("memcpy(p0, s0, n0);"));
}

#[test]
fn string_decode_copies_into_the_arena_with_a_terminator() {
    let joined = decode(&UniDataType::Scalar(UniScalar::String), C_STYLE_RECORD).join("\n");
    assert!(joined.contains("const char *s0 = mpr_str(r, &n0);"));
    assert!(joined.contains("mpa_alloc(a, (size_t)n0 + 1u)"));
    assert!(joined.contains("p0[n0] = '\\0';"));
}

#[test]
fn array_of_tuple_uses_holder_types_and_unique_loop_vars() {
    let tuple = UniDataType::Tuple(vec![
        UniDataType::Scalar(UniScalar::U64),
        UniDataType::Binary,
    ]);
    let ty = UniDataType::Array(Box::new(tuple));
    let mut namer = CollectingCNamer::new();
    namer.begin("relation_key".to_string());
    let slice = c_type(&ty, &mut namer).unwrap();
    assert_eq!(slice, "relation_key_list");
    assert_eq!(namer.holders().len(), 2);
    // Holders accumulate innermost-first (C declaration order: a typedef
    // may only reference already-declared types).
    assert_eq!(namer.holders()[0].kind, CHolderKind::Tuple);
    assert_eq!(namer.holders()[0].name, "relation_key_tuple");
    assert_eq!(namer.holders()[1].kind, CHolderKind::List);
    assert_eq!(namer.holders()[1].name, "relation_key_list");
    // A repeated pass over the same position resolves the same names.
    namer.begin("relation_key".to_string());
    let again = c_type(&ty, &mut namer).unwrap();
    assert_eq!(again, slice);
    assert_eq!(namer.holders().len(), 2);

    let mut out = Vec::new();
    c_decode_stmts(C_STYLE_FUNC, &ty, "out->key", 4, 0, &mut out, &mut namer).unwrap();
    let joined = out.join("\n");
    assert!(joined.contains("relation_key_tuple *p0"));
    assert!(joined.contains("(p0[i0]).f0 = mpr_u64(r);"));
    // The tuple's `list<u8>` decodes with the FUNC style (bin) at depth 2
    // (array elements are depth 1, tuple fields depth 2).
    assert!(joined.contains("mpr_bin(r, &n2)"));
}

#[test]
fn nested_options_get_numbered_holder_names() {
    // option<list<option<list<u8>>>> (the relation-get result shape)
    let ty = UniDataType::Option(Box::new(UniDataType::Array(Box::new(UniDataType::Option(
        Box::new(UniDataType::Binary),
    )))));
    let mut namer = CollectingCNamer::new();
    namer.begin("relation_get_result".to_string());
    let top = c_type(&ty, &mut namer).unwrap();
    // Names are assigned outermost-first (`_opt` is the outer wrapper),
    // while the holder list accumulates innermost-first (C declaration
    // order).
    assert_eq!(top, "relation_get_result_opt");
    let names: Vec<&str> = namer.holders().iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "relation_get_result_opt2",
            "relation_get_result_list",
            "relation_get_result_opt"
        ]
    );
    assert_eq!(namer.holders()[0].elem_types[0], "mp_bin");
    assert_eq!(namer.holders()[1].elem_types[0], "relation_get_result_opt2");
    assert_eq!(namer.holders()[2].elem_types[0], "relation_get_result_list");
}

#[test]
fn holder_decls_render_typedefs() {
    let ty = UniDataType::Array(Box::new(UniDataType::Scalar(UniScalar::U64)));
    let mut namer = CollectingCNamer::new();
    namer.begin("select".to_string());
    c_type(&ty, &mut namer).unwrap();
    let decl = namer.holders()[0].decl();
    assert_eq!(
        decl,
        "typedef struct { uint64_t *items; uint32_t len; } select_list;"
    );
}

#[test]
fn keywords_are_sanitized() {
    assert_eq!(c_ident("bool"), "bool_");
    assert_eq!(c_ident("char"), "char_");
    assert_eq!(c_ident("new"), "new_");
    assert_eq!(c_ident("union"), "union_");
    assert_eq!(c_ident("oid"), "oid");
    assert_eq!(c_ident("null"), "null");
}

#[test]
fn wide_integers_are_rejected() {
    let mut namer = CollectingCNamer::new();
    assert!(c_type(&UniDataType::Scalar(UniScalar::U128), &mut namer).is_err());
    assert!(c_type(&UniDataType::Scalar(UniScalar::I128), &mut namer).is_err());
}
