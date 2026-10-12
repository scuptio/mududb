//! Wire-to-typed conversion (`XxxFromValue`) statement generation — the
//! decode half of [`super::go_codec`].

use super::{go_type_name, next_tmp};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

pub(super) fn from_value_at(
    ty: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    ind: &str,
    err_ret: &str,
    lines: &mut Vec<String>,
) -> RS<()> {
    match ty {
        UniDataType::Scalar(scalar) => from_value_scalar(scalar, src, dst, ind, err_ret, lines),
        UniDataType::Binary => {
            lines.push(format!("{ind}{dst}, err = blobFromValue({src})"));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\t{err_ret}"));
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Identifier(name) => {
            lines.push(format!(
                "{ind}{dst}, err = {}FromValue({src})",
                to_pascal_case(name)
            ));
            lines.push(format!("{ind}if err != nil {{"));
            lines.push(format!("{ind}\t{err_ret}"));
            lines.push(format!("{ind}}}"));
        }
        UniDataType::Array(inner) => {
            from_value_array(inner, src, dst, tmp, ind, err_ret, lines)?;
        }
        UniDataType::Option(inner) => {
            from_value_option(inner, src, dst, tmp, ind, err_ret, lines)?;
        }
        UniDataType::Tuple(elems) => {
            from_value_tuple(elems, src, dst, tmp, ind, err_ret, lines)?;
        }
        UniDataType::Box(inner) => {
            from_value_at(inner, src, dst, tmp, ind, err_ret, lines)?;
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Go wire conversion for {ty:?} is not implemented")
            ));
        }
    }
    Ok(())
}

/// The checked-conversion helper decoding a wire value into this scalar type.
fn from_value_helper(scalar: &UniScalar) -> &'static str {
    match scalar {
        UniScalar::Bool => "boolChecked",
        UniScalar::U8 => "u8Checked",
        UniScalar::U16 => "u16Checked",
        UniScalar::U32 => "u32Checked",
        UniScalar::U64 => "u64Checked",
        UniScalar::U128 => "u128FromValue",
        UniScalar::I8 => "i8Checked",
        UniScalar::I16 => "i16Checked",
        UniScalar::I32 => "i32Checked",
        UniScalar::I64 => "i64Checked",
        UniScalar::I128 => "i128FromValue",
        UniScalar::F32 => "f32FromValue",
        UniScalar::F64 => "f64Checked",
        UniScalar::Char => "charChecked",
        UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => "strChecked",
        // Both the bin and the array-of-u8 form are accepted on
        // decode in either context (lenient).
        UniScalar::Blob => "blobFromValue",
    }
}

fn from_value_scalar(
    scalar: &UniScalar,
    src: &str,
    dst: &str,
    ind: &str,
    err_ret: &str,
    lines: &mut Vec<String>,
) {
    let helper = from_value_helper(scalar);
    lines.push(format!("{ind}{dst}, err = {helper}({src})"));
    lines.push(format!("{ind}if err != nil {{"));
    lines.push(format!("{ind}\t{err_ret}"));
    lines.push(format!("{ind}}}"));
}

fn from_value_array(
    inner: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    ind: &str,
    err_ret: &str,
    lines: &mut Vec<String>,
) -> RS<()> {
    let n = next_tmp(tmp);
    lines.push(format!("{ind}_l{n}, err := expectList({src})"));
    lines.push(format!("{ind}if err != nil {{"));
    lines.push(format!("{ind}\t{err_ret}"));
    lines.push(format!("{ind}}}"));
    lines.push(format!(
        "{ind}_v{n} := make([]{}, len(_l{n}))",
        go_type_name(inner)?
    ));
    lines.push(format!("{ind}for _i{n}, _e{n} := range _l{n} {{"));
    from_value_at(
        inner,
        &format!("_e{n}"),
        &format!("_v{n}[_i{n}]"),
        tmp,
        &format!("{ind}\t"),
        err_ret,
        lines,
    )?;
    lines.push(format!("{ind}}}"));
    lines.push(format!("{ind}{dst} = _v{n}"));
    Ok(())
}

fn from_value_option(
    inner: &UniDataType,
    src: &str,
    dst: &str,
    tmp: &mut usize,
    ind: &str,
    err_ret: &str,
    lines: &mut Vec<String>,
) -> RS<()> {
    let n = next_tmp(tmp);
    lines.push(format!("{ind}if {src} != nil {{"));
    lines.push(format!("{ind}\tvar _t{n} {}", go_type_name(inner)?));
    from_value_at(
        inner,
        src,
        &format!("_t{n}"),
        tmp,
        &format!("{ind}\t"),
        err_ret,
        lines,
    )?;
    lines.push(format!("{ind}\t{dst} = &_t{n}"));
    lines.push(format!("{ind}}}"));
    Ok(())
}

fn from_value_tuple(
    elems: &[UniDataType],
    src: &str,
    dst: &str,
    tmp: &mut usize,
    ind: &str,
    err_ret: &str,
    lines: &mut Vec<String>,
) -> RS<()> {
    let n = next_tmp(tmp);
    lines.push(format!(
        "{ind}_s{n}, err := expectSeq({src}, {})",
        elems.len()
    ));
    lines.push(format!("{ind}if err != nil {{"));
    lines.push(format!("{ind}\t{err_ret}"));
    lines.push(format!("{ind}}}"));
    let mut parts = Vec::with_capacity(elems.len());
    for (i, elem) in elems.iter().enumerate() {
        let var = format!("_t{n}_{i}");
        lines.push(format!("{ind}var {var} {}", go_type_name(elem)?));
        from_value_at(elem, &format!("_s{n}[{i}]"), &var, tmp, ind, err_ret, lines)?;
        parts.push(var);
    }
    lines.push(format!("{ind}{dst} = []any{{{}}}", parts.join(", ")));
    Ok(())
}
