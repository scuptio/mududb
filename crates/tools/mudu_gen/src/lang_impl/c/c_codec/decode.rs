use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

use super::ctype::c_type;
use super::holder::CTypeNamer;
use super::{CCodecStyle, c_type_name, pad};

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
        UniDataType::Scalar(scalar) => {
            c_decode_scalar(style, scalar, target, indent, depth, out)?;
        }
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
            c_decode_array(style, inner, target, indent, depth, out, namer)?;
        }
        UniDataType::Option(inner) => {
            c_decode_option(style, inner, target, indent, depth, out, namer)?;
        }
        UniDataType::Tuple(elems) => {
            c_decode_tuple(style, elems, target, indent, depth, out, namer)?;
        }
        UniDataType::Box(inner) => {
            c_decode_box(style, inner, target, indent, depth, out, namer)?;
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

fn c_decode_scalar(
    style: CCodecStyle,
    scalar: &UniScalar,
    target: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
    match scalar {
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
    }
    Ok(())
}

fn c_decode_array(
    style: CCodecStyle,
    inner: &UniDataType,
    target: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
    namer: &mut dyn CTypeNamer,
) -> RS<()> {
    let pad = pad(indent);
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
    Ok(())
}

fn c_decode_option(
    style: CCodecStyle,
    inner: &UniDataType,
    target: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
    namer: &mut dyn CTypeNamer,
) -> RS<()> {
    let pad = pad(indent);
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
    Ok(())
}

fn c_decode_tuple(
    style: CCodecStyle,
    elems: &[UniDataType],
    target: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
    namer: &mut dyn CTypeNamer,
) -> RS<()> {
    let pad = pad(indent);
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
    Ok(())
}

fn c_decode_box(
    style: CCodecStyle,
    inner: &UniDataType,
    target: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
    namer: &mut dyn CTypeNamer,
) -> RS<()> {
    let pad = pad(indent);
    // A box of a named type decodes through the type's generated
    // `<T>_decode_new` (arena-allocating) function: only its
    // prototype is needed, so boxes never force the pointee's full
    // definition into the including header (this is what keeps the
    // cross-file include graph acyclic). Boxes of anonymous shapes
    // (e.g. `box<box<T>>`) allocate inline; their size is a pointer
    // size and needs no completeness.
    if let UniDataType::Identifier(name) = inner {
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
