//! C MessagePack codec snippet generation.
//!
//! Produces explicit `mp_writer`/`mp_reader` statement lists (the runtime in
//! `crates/sdk/bindings/c/mududb/codec/mpack.h` exposes only explicit
//! per-type functions and C has no generics, so templates cannot use a
//! generic `write<T>`/`read<T>` style). Shared by the record/variant/enum/
//! func C templates; mirrors the AssemblyScript blueprint
//! (`lang_impl/assemblyscript/as_codec.rs`) statement for statement.
//!
//! The pinned dual semantics of `list<u8>` are controlled by
//! [`CCodecStyle::bytes_as_array`]: inside record/variant definitions a
//! `list<u8>` value is a plain MessagePack ARRAY of `u8` (matching the Rust
//! host's `Vec<u8>` serde output), while at func signature level it is a
//! MessagePack **bin** blob.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_snake_case, to_snake_case_upper};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Codec generation style selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CCodecStyle {
    /// Whether `list<u8>`/`blob` values encode as plain MessagePack arrays of
    /// `u8` (record/variant context) instead of MessagePack bin (func
    /// context).
    pub bytes_as_array: bool,
}

/// Record/variant definition context: `list<u8>` is a MessagePack array.
pub const C_STYLE_RECORD: CCodecStyle = CCodecStyle {
    bytes_as_array: true,
};

/// Func signature context: `list<u8>` is a MessagePack bin blob.
pub const C_STYLE_FUNC: CCodecStyle = CCodecStyle {
    bytes_as_array: false,
};

/// The kind of a generated composite holder type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CHolderKind {
    /// `list<T>` slice: `{ T *items; uint32_t len; }`.
    List,
    /// `option<T>` wrapper: `{ int has_value; T value; }`.
    Option,
    /// `tuple<T0, .., TN>` struct: `{ T0 f0; ..; TN fN; }`.
    Tuple,
}

/// A generated composite holder type (slice / option wrapper / tuple).
#[derive(Debug, Clone)]
pub struct CHolder {
    /// Holder type name.
    pub name: String,
    /// Holder shape.
    pub kind: CHolderKind,
    /// Element C types: exactly one for [`CHolderKind::List`] and
    /// [`CHolderKind::Option`], one per field (`f0..fN`) for
    /// [`CHolderKind::Tuple`].
    pub elem_types: Vec<String>,
}

impl CHolder {
    /// Render the holder as a C typedef.
    pub fn decl(&self) -> String {
        match self.kind {
            CHolderKind::List => format!(
                "typedef struct {{ {} *items; uint32_t len; }} {};",
                self.elem_types[0], self.name
            ),
            CHolderKind::Option => format!(
                "typedef struct {{ int has_value; {} value; }} {};",
                self.elem_types[0], self.name
            ),
            CHolderKind::Tuple => {
                let mut s = String::from("typedef struct {\n");
                for (i, ty) in self.elem_types.iter().enumerate() {
                    s.push_str(&format!("    {} f{};\n", ty, i));
                }
                s.push_str(&format!("}} {};", self.name));
                s
            }
        }
    }
}

/// Names composite holder types (`list` slices, `option` wrappers, tuples)
/// so that every value in codec position maps to exactly one named C type.
pub trait CTypeNamer {
    /// Return the slice type name for a `list` of `elem`.
    fn list_name(&mut self, elem: &UniDataType) -> RS<String>;
    /// Return the option-wrapper type name for an `option` of `inner`.
    fn option_name(&mut self, inner: &UniDataType) -> RS<String>;
    /// Return the struct type name for a tuple with the given element types.
    fn tuple_name(&mut self, elems: &[UniDataType]) -> RS<String>;
}

/// A [`CTypeNamer`] that names holders within one top-level signature
/// position (`begin` resets the position base) and accumulates the holder
/// declarations, deduplicated by name, for the template to emit. Names are
/// resolved by (position base, holder kind, tuple/list/option shape) so
/// repeated passes (type computation, encode, decode) over the same position
/// resolve identical names. Within one position the Nth holder of the same
/// kind gets a numeric suffix (`.._list`, `.._opt2`, `.._tuple3`).
pub struct CollectingCNamer {
    base: String,
    /// (base, shape debug key, name) triples in registration order.
    positions: Vec<(String, String, String)>,
    holders: Vec<CHolder>,
}

impl Default for CollectingCNamer {
    fn default() -> Self {
        Self::new()
    }
}

impl CollectingCNamer {
    /// Create an empty namer.
    pub fn new() -> Self {
        Self {
            base: String::new(),
            positions: Vec::new(),
            holders: Vec::new(),
        }
    }

    /// Start a new top-level position (e.g. one func parameter or one record
    /// field) whose holders are named from `base`.
    pub fn begin(&mut self, base: String) {
        self.base = base;
    }

    /// The accumulated holder declarations.
    pub fn holders(&self) -> &[CHolder] {
        &self.holders
    }

    fn kind_suffix(kind: CHolderKind) -> &'static str {
        match kind {
            CHolderKind::List => "list",
            CHolderKind::Option => "opt",
            CHolderKind::Tuple => "tuple",
        }
    }

    fn kind_prefix(kind: CHolderKind) -> &'static str {
        match kind {
            CHolderKind::List => "list:",
            CHolderKind::Option => "opt:",
            CHolderKind::Tuple => "tuple:",
        }
    }

    /// Assign the next name for the holder kind at the current position
    /// (outermost-first: names are assigned before the element types are
    /// computed, so the outer holder of a nested shape gets the lower
    /// index). Caller must have checked [`Self::lookup`] first.
    fn assign_name(&mut self, kind: CHolderKind, shape_key: String) -> String {
        let suffix = Self::kind_suffix(kind);
        let index = self
            .positions
            .iter()
            .filter(|(base, shape, _)| {
                *base == self.base && shape.starts_with(Self::kind_prefix(kind))
            })
            .count()
            + 1;
        let name = if index == 1 {
            format!("{}_{}", self.base, suffix)
        } else {
            format!("{}_{}{}", self.base, suffix, index)
        };
        self.positions
            .push((self.base.clone(), shape_key, name.clone()));
        name
    }

    fn lookup(&self, shape_key: &str) -> Option<String> {
        self.positions
            .iter()
            .find(|(base, shape, _)| *base == self.base && *shape == shape_key)
            .map(|(_, _, name)| name.clone())
    }
}

impl CTypeNamer for CollectingCNamer {
    fn list_name(&mut self, elem: &UniDataType) -> RS<String> {
        let key = format!("list:{:?}", elem);
        if let Some(name) = self.lookup(&key) {
            return Ok(name);
        }
        let name = self.assign_name(CHolderKind::List, key);
        let elem_ty = c_type(elem, self)?;
        self.holders.push(CHolder {
            name: name.clone(),
            kind: CHolderKind::List,
            elem_types: vec![elem_ty],
        });
        Ok(name)
    }

    fn option_name(&mut self, inner: &UniDataType) -> RS<String> {
        let key = format!("opt:{:?}", inner);
        if let Some(name) = self.lookup(&key) {
            return Ok(name);
        }
        let name = self.assign_name(CHolderKind::Option, key);
        let inner_ty = c_type(inner, self)?;
        self.holders.push(CHolder {
            name: name.clone(),
            kind: CHolderKind::Option,
            elem_types: vec![inner_ty],
        });
        Ok(name)
    }

    fn tuple_name(&mut self, elems: &[UniDataType]) -> RS<String> {
        let key = format!("tuple:{:?}", elems);
        if let Some(name) = self.lookup(&key) {
            return Ok(name);
        }
        let name = self.assign_name(CHolderKind::Tuple, key);
        let mut field_types = Vec::with_capacity(elems.len());
        for elem in elems {
            field_types.push(c_type(elem, self)?);
        }
        self.holders.push(CHolder {
            name: name.clone(),
            kind: CHolderKind::Tuple,
            elem_types: field_types,
        });
        Ok(name)
    }
}

/// The C type name of a user-defined WIT type (`uni-oid` -> `uni_oid`).
pub fn c_type_name(wit_name: &str) -> String {
    to_snake_case(wit_name)
}

/// The C constant prefix of a type name (`uni-oid` -> `UNI_OID`).
pub fn c_const_name(wit_name: &str) -> String {
    to_snake_case_upper(wit_name)
}

/// Sanitize a snake-case identifier that collides with a C or C++ keyword
/// (or a `stdbool.h`/`stddef.h` macro) by appending an underscore.
pub fn c_ident(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        // C keywords
        "auto",
        "break",
        "case",
        "char",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extern",
        "float",
        "for",
        "goto",
        "if",
        "inline",
        "int",
        "long",
        "register",
        "restrict",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "struct",
        "switch",
        "typedef",
        "union",
        "unsigned",
        "void",
        "volatile",
        "while",
        "_Alignas",
        "_Alignof",
        "_Atomic",
        "_Bool",
        "_Complex",
        "_Generic",
        "_Imaginary",
        "_Noreturn",
        "_Static_assert",
        "_Thread_local",
        // stdbool.h / stddef.h macros
        "bool",
        "true",
        "false",
        "NULL",
        // C++ keywords (the headers must also compile as C++)
        "alignas",
        "alignof",
        "and",
        "and_eq",
        "asm",
        "bitand",
        "bitor",
        "catch",
        "char16_t",
        "char32_t",
        "class",
        "compl",
        "constexpr",
        "const_cast",
        "decltype",
        "delete",
        "dynamic_cast",
        "explicit",
        "export",
        "friend",
        "mutable",
        "namespace",
        "new",
        "noexcept",
        "not",
        "not_eq",
        "nullptr",
        "operator",
        "or",
        "or_eq",
        "private",
        "protected",
        "public",
        "reinterpret_cast",
        "static_assert",
        "static_cast",
        "template",
        "this",
        "thread_local",
        "throw",
        "try",
        "typeid",
        "typename",
        "using",
        "virtual",
        "wchar_t",
        "xor",
        "xor_eq",
        "override",
        "final",
    ];
    if KEYWORDS.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// The C type of a value in codec position.
///
/// Composites are named through `namer`: `list<T>` maps to a generated slice
/// struct, `option<T>` to a generated wrapper struct, `box<T>` to `T *` and
/// tuples to generated `f0..fN` structs.
pub fn c_type(ty: &UniDataType, namer: &mut dyn CTypeNamer) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => "bool".to_string(),
            UniScalar::U8 => "uint8_t".to_string(),
            UniScalar::U16 => "uint16_t".to_string(),
            UniScalar::U32 => "uint32_t".to_string(),
            UniScalar::U64 => "uint64_t".to_string(),
            UniScalar::I8 => "int8_t".to_string(),
            UniScalar::I16 => "int16_t".to_string(),
            UniScalar::I32 => "int32_t".to_string(),
            UniScalar::I64 => "int64_t".to_string(),
            UniScalar::F32 => "float".to_string(),
            UniScalar::F64 => "double".to_string(),
            UniScalar::Char => "char".to_string(),
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => "mp_str".to_string(),
            UniScalar::Blob => "mp_bin".to_string(),
            UniScalar::U128 | UniScalar::I128 => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "C codec does not support 128-bit integers (the uni surface carries them as list<u8>)"
                ));
            }
        },
        UniDataType::Binary => "mp_bin".to_string(),
        UniDataType::Array(inner) => namer.list_name(inner)?,
        UniDataType::Option(inner) => namer.option_name(inner)?,
        UniDataType::Tuple(elems) => namer.tuple_name(elems)?,
        UniDataType::Box(inner) => format!("{} *", c_type(inner, namer)?),
        UniDataType::Identifier(name) => c_type_name(name),
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("C codec does not support {:?} in this position", ty)
            ));
        }
    };
    Ok(s)
}

fn pad(indent: usize) -> String {
    " ".repeat(indent)
}

/// Append the statements encoding `expr` (a value of type `ty`) to `out`,
/// one line per statement, indented by `indent` spaces. `depth` uniquifies
/// the loop variables of nested composites (`i0`, `i1`, ..).
pub fn c_encode_stmts(
    style: CCodecStyle,
    ty: &UniDataType,
    expr: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
    match ty {
        UniDataType::Scalar(scalar) => {
            let stmt = match scalar {
                UniScalar::Bool => format!("mpw_bool(w, {});", expr),
                UniScalar::U8 | UniScalar::U16 | UniScalar::U32 | UniScalar::U64 => {
                    format!("mpw_u64(w, (uint64_t)({}));", expr)
                }
                UniScalar::I8 | UniScalar::I16 | UniScalar::I32 | UniScalar::I64 => {
                    format!("mpw_i64(w, (int64_t)({}));", expr)
                }
                UniScalar::F32 => format!("mpw_f32(w, {});", expr),
                UniScalar::F64 => format!("mpw_f64(w, {});", expr),
                UniScalar::Char => format!("mpw_str(w, &({}), 1u);", expr),
                UniScalar::String
                | UniScalar::Numeric
                | UniScalar::Date
                | UniScalar::Time
                | UniScalar::Timestamp
                | UniScalar::TimestampTz => {
                    format!("mpw_str(w, ({}).data, ({}).len);", expr, expr)
                }
                UniScalar::Blob => {
                    return c_encode_bin(style, expr, indent, depth, out);
                }
                UniScalar::U128 | UniScalar::I128 => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "C codec does not support 128-bit integers"
                    ));
                }
            };
            out.push(format!("{}{}", pad, stmt));
        }
        UniDataType::Binary => {
            return c_encode_bin(style, expr, indent, depth, out);
        }
        UniDataType::Identifier(name) => {
            out.push(format!(
                "{}{}_encode(w, &({}));",
                pad,
                c_type_name(name),
                expr
            ));
        }
        UniDataType::Array(inner) => {
            let i = format!("i{}", depth);
            out.push(format!("{}mpw_array_header(w, ({}).len);", pad, expr));
            out.push(format!(
                "{}for (uint32_t {} = 0; {} < ({}).len; {}++) {{",
                pad, i, i, expr, i
            ));
            c_encode_stmts(
                style,
                inner,
                &format!("({}).items[{}]", expr, i),
                indent + 4,
                depth + 1,
                out,
            )?;
            out.push(format!("{}}}", pad));
        }
        UniDataType::Option(inner) => {
            out.push(format!("{}if (({}).has_value) {{", pad, expr));
            c_encode_stmts(
                style,
                inner,
                &format!("({}).value", expr),
                indent + 4,
                depth + 1,
                out,
            )?;
            out.push(format!("{}}} else {{", pad));
            out.push(format!("{}    mpw_nil(w);", pad));
            out.push(format!("{}}}", pad));
        }
        UniDataType::Tuple(elems) => {
            out.push(format!("{}mpw_array_header(w, {}u);", pad, elems.len()));
            for (k, elem) in elems.iter().enumerate() {
                c_encode_stmts(
                    style,
                    elem,
                    &format!("({}).f{}", expr, k),
                    indent,
                    depth + 1,
                    out,
                )?;
            }
        }
        UniDataType::Box(inner) => {
            out.push(format!("{}if (({}) == 0) {{", pad, expr));
            out.push(format!("{}    mpw_fail(w);", pad));
            out.push(format!("{}}} else {{", pad));
            c_encode_stmts(
                style,
                inner,
                &format!("(*{})", expr),
                indent + 4,
                depth + 1,
                out,
            )?;
            out.push(format!("{}}}", pad));
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("C codec does not support {:?} in this position", ty)
            ));
        }
    }
    Ok(())
}

fn c_encode_bin(
    style: CCodecStyle,
    expr: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
    if style.bytes_as_array {
        let i = format!("i{}", depth);
        out.push(format!("{}mpw_array_header(w, ({}).len);", pad, expr));
        out.push(format!(
            "{}for (uint32_t {} = 0; {} < ({}).len; {}++) {{",
            pad, i, i, expr, i
        ));
        out.push(format!("{}    mpw_u64(w, ({}).data[{}]);", pad, expr, i));
        out.push(format!("{}}}", pad));
    } else {
        out.push(format!(
            "{}mpw_bin(w, ({}).data, ({}).len);",
            pad, expr, expr
        ));
    }
    Ok(())
}

/// Append the statements decoding a value of type `ty` into `target` (an
/// assignable l-value expression) to `out`, one line per statement, indented
/// by `indent` spaces. `depth` uniquifies the temporaries of nested
/// composites (`n0`/`s0`/`p0`/`i0`, `n1`/`s1`/`p1`/`i1`, ..).
pub fn c_decode_stmts(
    style: CCodecStyle,
    ty: &UniDataType,
    target: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
    namer: &mut dyn CTypeNamer,
) -> RS<()> {
    let pad = pad(indent);
    match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => out.push(format!("{}{} = mpr_bool(r);", pad, target)),
            UniScalar::U8 => out.push(format!("{}{} = (uint8_t)mpr_u64(r);", pad, target)),
            UniScalar::U16 => out.push(format!("{}{} = (uint16_t)mpr_u64(r);", pad, target)),
            UniScalar::U32 => out.push(format!("{}{} = (uint32_t)mpr_u64(r);", pad, target)),
            UniScalar::U64 => out.push(format!("{}{} = mpr_u64(r);", pad, target)),
            UniScalar::I8 => out.push(format!("{}{} = (int8_t)mpr_i64(r);", pad, target)),
            UniScalar::I16 => out.push(format!("{}{} = (int16_t)mpr_i64(r);", pad, target)),
            UniScalar::I32 => out.push(format!("{}{} = (int32_t)mpr_i64(r);", pad, target)),
            UniScalar::I64 => out.push(format!("{}{} = mpr_i64(r);", pad, target)),
            UniScalar::F32 => out.push(format!("{}{} = mpr_f32(r);", pad, target)),
            UniScalar::F64 => out.push(format!("{}{} = mpr_f64(r);", pad, target)),
            UniScalar::Char => {
                let (n, s) = tmp_names(depth);
                out.push(format!("{}{{", pad));
                out.push(format!("{}    uint32_t {} = 0;", pad, n));
                out.push(format!(
                    "{}    const char *{} = mpr_str(r, &{});",
                    pad, s, n
                ));
                out.push(format!(
                    "{}    {} = {} > 0 ? {}[0] : '\\0';",
                    pad, target, n, s
                ));
                out.push(format!("{}}}", pad));
            }
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => {
                c_decode_copy(
                    &pad,
                    depth,
                    &CCopySpec {
                        borrow_ty: "const char *",
                        read_fn: "mpr_str",
                        alloc_ty: "char *",
                        nul_terminate: true,
                    },
                    target,
                    out,
                );
            }
            UniScalar::Blob => {
                return c_decode_bin(style, target, indent, depth, out);
            }
            UniScalar::U128 | UniScalar::I128 => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "C codec does not support 128-bit integers"
                ));
            }
        },
        UniDataType::Binary => {
            return c_decode_bin(style, target, indent, depth, out);
        }
        UniDataType::Identifier(name) => {
            out.push(format!(
                "{}{}_decode(r, a, &({}));",
                pad,
                c_type_name(name),
                target
            ));
        }
        UniDataType::Array(inner) => {
            let elem_ty = c_type(inner, namer)?;
            let (n, _, p, i) = tmp_names4(depth);
            out.push(format!("{}{{", pad));
            out.push(format!("{}    uint32_t {} = mpr_array_header(r);", pad, n));
            out.push(format!(
                "{}    {} *{} = ({} *)mpa_alloc(a, sizeof({}) * (size_t){});",
                pad, elem_ty, p, elem_ty, elem_ty, n
            ));
            out.push(format!("{}    if ({} == 0) {{", pad, p));
            out.push(format!("{}        mpr_fail(r);", pad));
            out.push(format!("{}    }} else {{", pad));
            out.push(format!(
                "{}        for (uint32_t {} = 0; {} < {}; {}++) {{",
                pad, i, i, n, i
            ));
            c_decode_stmts(
                style,
                inner,
                &format!("{}[{}]", p, i),
                indent + 12,
                depth + 1,
                out,
                namer,
            )?;
            out.push(format!("{}        }}", pad));
            out.push(format!("{}        ({}).items = {};", pad, target, p));
            out.push(format!("{}        ({}).len = {};", pad, target, n));
            out.push(format!("{}    }}", pad));
            out.push(format!("{}}}", pad));
        }
        UniDataType::Option(inner) => {
            out.push(format!(
                "{}memset(&({}), 0, sizeof({}));",
                pad, target, target
            ));
            out.push(format!("{}if (mpr_try_nil(r)) {{", pad));
            out.push(format!("{}    ({}).has_value = 0;", pad, target));
            out.push(format!("{}}} else {{", pad));
            out.push(format!("{}    ({}).has_value = 1;", pad, target));
            c_decode_stmts(
                style,
                inner,
                &format!("({}).value", target),
                indent + 4,
                depth + 1,
                out,
                namer,
            )?;
            out.push(format!("{}}}", pad));
        }
        UniDataType::Tuple(elems) => {
            out.push(format!(
                "{}memset(&({}), 0, sizeof({}));",
                pad, target, target
            ));
            out.push(format!(
                "{}if (mpr_array_header(r) != {}u) {{",
                pad,
                elems.len()
            ));
            out.push(format!("{}    mpr_fail(r);", pad));
            out.push(format!("{}}} else {{", pad));
            for (k, elem) in elems.iter().enumerate() {
                c_decode_stmts(
                    style,
                    elem,
                    &format!("({}).f{}", target, k),
                    indent + 4,
                    depth + 1,
                    out,
                    namer,
                )?;
            }
            out.push(format!("{}}}", pad));
        }
        UniDataType::Box(inner) => {
            // A box of a named type decodes through the type's generated
            // `<T>_decode_new` (arena-allocating) function: only its
            // prototype is needed, so boxes never force the pointee's full
            // definition into the including header (this is what keeps the
            // cross-file include graph acyclic). Boxes of anonymous shapes
            // (e.g. `box<box<T>>`) allocate inline; their size is a pointer
            // size and needs no completeness.
            if let UniDataType::Identifier(name) = inner.as_ref() {
                let snake = c_type_name(name);
                let (_, _, p, _) = tmp_names4(depth);
                out.push(format!("{}{{", pad));
                out.push(format!(
                    "{}    {} *{} = {}_decode_new(r, a);",
                    pad, snake, p, snake
                ));
                out.push(format!("{}    if ({} == 0) {{", pad, p));
                out.push(format!("{}        mpr_fail(r);", pad));
                out.push(format!("{}    }} else {{", pad));
                out.push(format!("{}        {} = {};", pad, target, p));
                out.push(format!("{}    }}", pad));
                out.push(format!("{}}}", pad));
                return Ok(());
            }
            let inner_ty = c_type(inner, namer)?;
            let (_, _, p, _) = tmp_names4(depth);
            out.push(format!("{}{{", pad));
            out.push(format!(
                "{}    {} *{} = ({} *)mpa_alloc(a, sizeof({}));",
                pad, inner_ty, p, inner_ty, inner_ty
            ));
            out.push(format!("{}    if ({} == 0) {{", pad, p));
            out.push(format!("{}        mpr_fail(r);", pad));
            out.push(format!("{}    }} else {{", pad));
            out.push(format!("{}        memset({}, 0, sizeof(*{}));", pad, p, p));
            c_decode_stmts(
                style,
                inner,
                &format!("(*{})", p),
                indent + 8,
                depth + 1,
                out,
                namer,
            )?;
            out.push(format!("{}        {} = {};", pad, target, p));
            out.push(format!("{}    }}", pad));
            out.push(format!("{}}}", pad));
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("C codec does not support {:?} in this position", ty)
            ));
        }
    }
    Ok(())
}

/// Temp names (length, borrow, alloc) for one nesting depth.
fn tmp_names(depth: usize) -> (String, String) {
    (format!("n{}", depth), format!("s{}", depth))
}

/// Temp names (length, borrow, alloc, loop index) for one nesting depth.
fn tmp_names4(depth: usize) -> (String, String, String, String) {
    (
        format!("n{}", depth),
        format!("s{}", depth),
        format!("p{}", depth),
        format!("i{}", depth),
    )
}

/// The borrow-then-arena-copy decode plan for a `str`/`bin` slice.
struct CCopySpec {
    /// Borrowed pointer type produced by the reader (`const char *`).
    borrow_ty: &'static str,
    /// Reader function producing the borrowed slice (`mpr_str`).
    read_fn: &'static str,
    /// Arena allocation pointer type (`char *`).
    alloc_ty: &'static str,
    /// Whether a `'\0'` is appended after the copied bytes.
    nul_terminate: bool,
}

/// Emit the borrow-then-arena-copy decode of a `str`/`bin` slice into
/// `target` (`{target}.data` / `{target}.len`).
fn c_decode_copy(pad: &str, depth: usize, spec: &CCopySpec, target: &str, out: &mut Vec<String>) {
    let (n, s, p, _) = tmp_names4(depth);
    out.push(format!("{}{{", pad));
    out.push(format!("{}    uint32_t {} = 0;", pad, n));
    out.push(format!(
        "{}    {}{} = {}(r, &{});",
        pad, spec.borrow_ty, s, spec.read_fn, n
    ));
    out.push(format!(
        "{}    {}{} = ({})mpa_alloc(a, (size_t){}{});",
        pad,
        spec.alloc_ty,
        p,
        spec.alloc_ty,
        n,
        if spec.nul_terminate { " + 1u" } else { "" }
    ));
    out.push(format!("{}    if ({} == 0) {{", pad, p));
    out.push(format!("{}        mpr_fail(r);", pad));
    out.push(format!("{}    }} else {{", pad));
    out.push(format!("{}        memcpy({}, {}, {});", pad, p, s, n));
    if spec.nul_terminate {
        out.push(format!("{}        {}[{}] = '\\0';", pad, p, n));
    }
    out.push(format!("{}        ({}).data = {};", pad, target, p));
    out.push(format!("{}        ({}).len = {};", pad, target, n));
    out.push(format!("{}    }}", pad));
    out.push(format!("{}}}", pad));
}

fn c_decode_bin(
    style: CCodecStyle,
    target: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
    if style.bytes_as_array {
        let (n, _, p, i) = tmp_names4(depth);
        out.push(format!("{}{{", pad));
        out.push(format!("{}    uint32_t {} = mpr_array_header(r);", pad, n));
        out.push(format!(
            "{}    uint8_t *{} = (uint8_t *)mpa_alloc(a, {});",
            pad, p, n
        ));
        out.push(format!("{}    if ({} == 0) {{", pad, p));
        out.push(format!("{}        mpr_fail(r);", pad));
        out.push(format!("{}    }} else {{", pad));
        out.push(format!(
            "{}        for (uint32_t {} = 0; {} < {}; {}++) {{",
            pad, i, i, n, i
        ));
        out.push(format!(
            "{}            {}[{}] = (uint8_t)mpr_u64(r);",
            pad, p, i
        ));
        out.push(format!("{}        }}", pad));
        out.push(format!("{}        ({}).data = {};", pad, target, p));
        out.push(format!("{}        ({}).len = {};", pad, target, n));
        out.push(format!("{}    }}", pad));
        out.push(format!("{}}}", pad));
    } else {
        c_decode_copy(
            &pad,
            depth,
            &CCopySpec {
                borrow_ty: "const uint8_t *",
                read_fn: "mpr_bin",
                alloc_ty: "uint8_t *",
                nul_terminate: false,
            },
            target,
            out,
        );
    }
    Ok(())
}

/// One branch of a lenient integer-keyed map decode (a record field or a
/// request parameter).
pub struct CMapEntry {
    /// 1-based field/parameter number (the wire map key).
    pub number: u32,
    /// Decode statements for the value, indented for the branch body
    /// (`indent + 16` spaces, joined).
    pub decode_stmts: String,
}

/// Render the lenient map-decode loop shared by record and request decoders:
/// integer keys of any width, unknown keys skipped, missing keys defaulted
/// (the caller `memset`s the target beforehand).
pub fn c_map_decode_loop(entries: &[CMapEntry], indent: usize) -> String {
    let pad = pad(indent);
    let mut lines = Vec::new();
    lines.push(format!("{}{{", pad));
    lines.push(format!("{}    uint32_t count = mpr_map_header(r);", pad));
    lines.push(format!(
        "{}    for (uint32_t i = 0; i < count && mpr_ok(r); i++) {{",
        pad
    ));
    lines.push(format!("{}        if (mpr_next_is_integer(r)) {{", pad));
    lines.push(format!("{}            uint64_t key = mpr_u64(r);", pad));
    if entries.is_empty() {
        lines.push(format!("{}            (void)mpr_u64(r);", pad));
        lines.push(format!("{}            mpr_skip(r);", pad));
    } else {
        for (index, entry) in entries.iter().enumerate() {
            if index == 0 {
                lines.push(format!(
                    "{}            if (key == {}u) {{",
                    pad, entry.number
                ));
            } else {
                lines.push(format!(
                    "{}            }} else if (key == {}u) {{",
                    pad, entry.number
                ));
            }
            lines.push(entry.decode_stmts.clone());
        }
        lines.push(format!("{}            }} else {{", pad));
        lines.push(format!("{}                mpr_skip(r);", pad));
        lines.push(format!("{}            }}", pad));
    }
    lines.push(format!("{}        }} else {{", pad));
    lines.push(format!("{}            mpr_skip(r);", pad));
    lines.push(format!("{}            mpr_skip(r);", pad));
    lines.push(format!("{}        }}", pad));
    lines.push(format!("{}    }}", pad));
    lines.push(format!("{}}}", pad));
    lines.join("\n")
}

/// Join snippet lines into a single template-ready string.
pub fn c_join(lines: &[String]) -> String {
    lines.join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{
        C_STYLE_FUNC, C_STYLE_RECORD, CHolderKind, CollectingCNamer, c_decode_stmts,
        c_encode_stmts, c_ident, c_type,
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
}
