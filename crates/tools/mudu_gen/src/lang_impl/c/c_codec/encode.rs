use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

use super::{CCodecStyle, c_type_name, pad};

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
            c_encode_scalar(style, scalar, expr, indent, depth, out)?;
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
            c_encode_array(style, inner, expr, indent, depth, out)?;
        }
        UniDataType::Option(inner) => {
            c_encode_option(style, inner, expr, indent, depth, out)?;
        }
        UniDataType::Tuple(elems) => {
            c_encode_tuple(style, elems, expr, indent, depth, out)?;
        }
        UniDataType::Box(inner) => {
            c_encode_box(style, inner, expr, indent, depth, out)?;
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

fn c_encode_scalar(
    style: CCodecStyle,
    scalar: &UniScalar,
    expr: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
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
    Ok(())
}

fn c_encode_array(
    style: CCodecStyle,
    inner: &UniDataType,
    expr: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
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
    Ok(())
}

fn c_encode_option(
    style: CCodecStyle,
    inner: &UniDataType,
    expr: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
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
    Ok(())
}

fn c_encode_tuple(
    style: CCodecStyle,
    elems: &[UniDataType],
    expr: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
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
    Ok(())
}

fn c_encode_box(
    style: CCodecStyle,
    inner: &UniDataType,
    expr: &str,
    indent: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> RS<()> {
    let pad = pad(indent);
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
