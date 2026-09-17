//! Registry of user-defined types declared in project `.wit` files.
//!
//! The byte-pipe front-ends (AssemblyScript, Python, C#, C, Go) resolve a
//! procedure parameter/return type in two stages: first the language scalar
//! table, then this registry, which aggregates every `record`, `variant` and
//! `enum` declared in the `.wit` files passed via `mtp --type-wit`.
//!
//! Conversion into the host type vocabulary ([`DataType`]) mirrors the
//! mapping the front-ends already use for their scalars:
//!
//! - WIT integers up to 32 bits map to the `I32` family, 64-bit integers to
//!   `I64`, 128-bit integers to `U128`/`I128`, floats to `F32`/`F64`,
//!   `string` to the `String` family, `blob` and `list<u8>` to the `Binary`
//!   family, and `bool` to `I32` (the front-end Boolean mapping).
//! - A WIT `record` maps to the `Record` family ([`DataTypeParamRecord`])
//!   named by its PascalCase type name (the name mgen generates in the guest
//!   languages), with the WIT field names kept verbatim; a nested record
//!   reference recurses.
//! - A WIT `enum` maps to `I32`: case ordinals travel as 32-bit integers.
//! - `list<T>` maps to the `Array` family ([`DataTypeParamArray`]).
//! - `option<T>` maps to the inner `DataType` of `T`. The host `DataType`
//!   cannot express nullability, so a front-end that converts a top-level
//!   procedure field must carry the flag itself to
//!   `mudu_contract::tuple::datum_desc::DatumDesc::new_nullable` — see
//!   [`TypeRegistry::to_data_type_nullable`]. Nullability *inside* arrays
//!   and record fields is not representable at all (`DataTypeParamRecord`
//!   fields are plain `(name, DataType)` pairs) and is dropped.
//!
//! Hard errors are returned for: `variant` types used as procedure
//! parameter/return types (the host type system has no Variant family),
//! `tuple`/`result`/`box` types, `char` (no host family), unknown type
//! names, duplicate type names across the input files, and recursive/cyclic
//! record references (detected eagerly when the registry is built, before
//! any conversion).

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::{UniEnumDef, UniRecordDef, UniVariantDef};
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_gen::src_gen::wit_parser::WitParser;
use mudu_type::data_type::DataType;
use mudu_type::data_type_param_array::DataTypeParamArray;
use mudu_type::data_type_param_record::DataTypeParamRecord;
use mudu_type::type_family::TypeFamily;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A user-defined type loaded from a project `.wit` file.
#[derive(Debug, Clone)]
pub enum TypeDefKind {
    /// A WIT `record` definition.
    Record(UniRecordDef),
    /// A WIT `variant` definition. Variants are registered so their names
    /// resolve, but converting one to a host [`DataType`] is rejected: the
    /// host type system has no Variant family.
    Variant(UniVariantDef),
    /// A WIT `enum` definition. Enums convert to the `I32` family (case
    /// ordinals travel as 32-bit integers).
    Enum(UniEnumDef),
}

impl TypeDefKind {
    /// Type name as declared in the WIT source (kebab-case by convention).
    pub fn declared_name(&self) -> &str {
        match self {
            Self::Record(def) => &def.record_name,
            Self::Variant(def) => &def.variant_name,
            Self::Enum(def) => &def.enum_name,
        }
    }

    /// Kind tag of the definition: `"record"`, `"variant"` or `"enum"`.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Record(_) => "record",
            Self::Variant(_) => "variant",
            Self::Enum(_) => "enum",
        }
    }

    /// Return the record definition, or `None` for variants and enums.
    pub fn as_record(&self) -> Option<&UniRecordDef> {
        match self {
            Self::Record(def) => Some(def),
            _ => None,
        }
    }
}

/// Registry of the user-defined types declared in a set of `.wit` files.
///
/// Names are keyed by their PascalCase form (the same keying `mgen message`
/// uses), so lookups accept the type name in any source-language case form
/// (`profile`, `Profile`, `PROFILE` all resolve the WIT record `profile`).
#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    types: HashMap<String, RegisteredType>,
}

#[derive(Debug, Clone)]
struct RegisteredType {
    def: TypeDefKind,
    origin: PathBuf,
}

impl TypeRegistry {
    /// Build a registry from `--type-wit` paths.
    ///
    /// Each path may be a single `.wit` file or a directory; a directory
    /// contributes every `*.wit` file directly inside it, processed in
    /// sorted path order so aggregation is deterministic. Declaring the same
    /// type name (after PascalCase normalization) in two files is a hard
    /// error, as is a cyclic record reference; the record graph is checked
    /// for cycles before the registry is returned, so every record in a
    /// successfully built registry is convertible.
    pub fn from_wit_paths(paths: &[String]) -> RS<TypeRegistry> {
        let files = collect_wit_files(paths)?;
        let parser = WitParser::new();
        let mut registry = TypeRegistry {
            types: HashMap::new(),
        };
        for file in &files {
            let text = mudu_sys::fs::sync::sync_read_to_string(file)?;
            let wit_def = parser.parse_text(&text).map_err(|e| {
                mudu_error!(
                    ErrorCode::Parse,
                    format!("parse WIT file '{}' error: {}", file.display(), e)
                )
            })?;
            for record in &wit_def.records {
                registry.insert(TypeDefKind::Record(record.clone()), file)?;
            }
            for variant in &wit_def.variants {
                registry.insert(TypeDefKind::Variant(variant.clone()), file)?;
            }
            for enum_def in &wit_def.enums {
                registry.insert(TypeDefKind::Enum(enum_def.clone()), file)?;
            }
        }
        registry.check_acyclic()?;
        Ok(registry)
    }

    /// Resolve a type name as written in guest source (any case form) to its
    /// WIT definition.
    pub fn resolve(&self, name: &str) -> Option<&TypeDefKind> {
        self.types
            .get(&to_pascal_case(name))
            .map(|registered| &registered.def)
    }

    /// Number of registered types.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Whether the registry holds no types.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Convert a WIT type to the host [`DataType`], resolving `Identifier`
    /// references against the registry recursively.
    ///
    /// `Option` maps to the inner type: nullability is not part of
    /// `DataType`. For top-level procedure fields use
    /// [`Self::to_data_type_nullable`], which reports the flag so the caller
    /// can pass it to `DatumDesc::new_nullable`.
    pub fn to_data_type(&self, ty: &UniDataType) -> RS<DataType> {
        self.convert(ty)
    }

    /// Convert a WIT type like [`Self::to_data_type`], additionally
    /// reporting whether the value is nullable (top-level `option<T>`).
    ///
    /// The returned flag is `true` exactly when `ty` is `option<T>`; only
    /// one level is peeled. The caller is expected to build the procedure
    /// field descriptor with `DatumDesc::new_nullable(name, data_type,
    /// nullable)`.
    pub fn to_data_type_nullable(&self, ty: &UniDataType) -> RS<(DataType, bool)> {
        match ty {
            UniDataType::Option(inner) => Ok((self.convert(inner)?, true)),
            _ => Ok((self.convert(ty)?, false)),
        }
    }

    fn insert(&mut self, def: TypeDefKind, origin: &Path) -> RS<()> {
        let key = to_pascal_case(def.declared_name());
        if let Some(existing) = self.types.get(&key) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "duplicate WIT {} name '{}': declared in both '{}' and '{}'",
                    def.kind_name(),
                    def.declared_name(),
                    existing.origin.display(),
                    origin.display()
                )
            ));
        }
        self.types.insert(
            key,
            RegisteredType {
                def,
                origin: origin.to_path_buf(),
            },
        );
        Ok(())
    }

    /// Depth-first cycle detection over the record reference graph. Every
    /// record reachable through `Identifier` field references (including
    /// references nested inside `list`/`option`/`box`/`tuple`/`result` and
    /// inline records) must be acyclic; a cycle is reported as
    /// `Name -> ... -> Name` using the PascalCase registry keys.
    fn check_acyclic(&self) -> RS<()> {
        let mut color: HashMap<&str, u8> = HashMap::new();
        let mut keys: Vec<&str> = self.types.keys().map(String::as_str).collect();
        keys.sort();
        let mut stack: Vec<String> = Vec::new();
        for key in keys {
            if !color.contains_key(key) {
                self.visit_type(key, &mut color, &mut stack)?;
            }
        }
        Ok(())
    }

    fn visit_type<'a>(
        &'a self,
        key: &'a str,
        color: &mut HashMap<&'a str, u8>,
        stack: &mut Vec<String>,
    ) -> RS<()> {
        const IN_STACK: u8 = 1;
        const DONE: u8 = 2;
        color.insert(key, IN_STACK);
        stack.push(key.to_string());
        if let Some(registered) = self.types.get(key)
            && let TypeDefKind::Record(record) = &registered.def
        {
            let mut references = Vec::new();
            for field in &record.record_fields {
                collect_identifiers(&field.rf_type, &mut references);
            }
            for name in references {
                let ref_key_string = to_pascal_case(&name);
                let Some((ref_key, _)) = self.types.get_key_value(ref_key_string.as_str()) else {
                    // Unknown names are reported at conversion time.
                    continue;
                };
                match color.get(ref_key.as_str()) {
                    Some(&IN_STACK) => {
                        let start = stack
                            .iter()
                            .position(|stacked| stacked == ref_key)
                            .unwrap_or(0);
                        let mut cycle = stack[start..].to_vec();
                        cycle.push(ref_key.clone());
                        return Err(mudu_error!(
                            ErrorCode::InvalidArgument,
                            format!(
                                "cyclic record type reference detected: {}; recursive records are not supported",
                                cycle.join(" -> ")
                            )
                        ));
                    }
                    Some(&DONE) => {}
                    _ => self.visit_type(ref_key.as_str(), color, stack)?,
                }
            }
        }
        stack.pop();
        color.insert(key, DONE);
        Ok(())
    }

    fn convert(&self, ty: &UniDataType) -> RS<DataType> {
        match ty {
            UniDataType::Scalar(scalar) => Self::scalar_to_data_type(scalar),
            UniDataType::Array(inner) => {
                let element = self.convert(inner)?;
                Ok(DataType::from_array(DataTypeParamArray::new(element)))
            }
            UniDataType::Record(inline) => {
                let mut fields = Vec::with_capacity(inline.record_fields.len());
                for field in &inline.record_fields {
                    fields.push((field.field_name.clone(), self.convert(&field.field_type)?));
                }
                Ok(DataType::from_record(DataTypeParamRecord::new(
                    inline.record_name.clone(),
                    fields,
                )))
            }
            UniDataType::Option(inner) => {
                // Nullability is not expressible in DataType; it is carried
                // by DatumDesc::new_nullable at the procedure field level
                // (see to_data_type_nullable). Nested optionality is dropped.
                self.convert(inner)
            }
            UniDataType::Tuple(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "WIT tuple types are not supported in procedure types or record fields: the host type system has no Tuple family"
            )),
            UniDataType::Result(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "WIT result types are not supported in procedure types or record fields: the host type system has no Result family"
            )),
            UniDataType::Identifier(name) => self.identifier_to_data_type(name),
            UniDataType::Binary => Ok(DataType::new_no_param(TypeFamily::Binary)),
            UniDataType::Box(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "WIT box types are not supported in procedure types or record fields"
            )),
        }
    }

    fn identifier_to_data_type(&self, name: &str) -> RS<DataType> {
        let def = self.resolve(name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "unknown WIT type '{}': not declared in any --type-wit file",
                    name
                )
            )
        })?;
        match def {
            TypeDefKind::Record(record) => self.record_to_data_type(record),
            TypeDefKind::Variant(variant) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "WIT variant type '{}' cannot be used as a procedure parameter or return type: the host type system has no Variant family",
                    variant.variant_name
                )
            )),
            TypeDefKind::Enum(_) => {
                // Enum case ordinals travel as u32 values, carried by the I32
                // family (the same family the front-ends use for Boolean).
                Ok(DataType::new_no_param(TypeFamily::I32))
            }
        }
    }

    fn record_to_data_type(&self, record: &UniRecordDef) -> RS<DataType> {
        let mut fields = Vec::with_capacity(record.record_fields.len());
        for field in &record.record_fields {
            fields.push((field.rf_name.clone(), self.convert(&field.rf_type)?));
        }
        Ok(DataType::from_record(DataTypeParamRecord::new(
            to_pascal_case(&record.record_name),
            fields,
        )))
    }

    /// Map a WIT scalar to the host family, mirroring the front-end scalar
    /// tables (`Boolean` -> `I32`, `Int64` -> `I64`, `Float64` -> `F64`,
    /// `Text` -> `String`, `Binary` -> `Binary`, `ObjectId` -> `U128`).
    fn scalar_to_data_type(scalar: &UniScalar) -> RS<DataType> {
        let data_type = match scalar {
            UniScalar::Bool
            | UniScalar::U8
            | UniScalar::I8
            | UniScalar::U16
            | UniScalar::I16
            | UniScalar::U32
            | UniScalar::I32 => DataType::new_no_param(TypeFamily::I32),
            UniScalar::U64 | UniScalar::I64 => DataType::new_no_param(TypeFamily::I64),
            UniScalar::U128 => DataType::new_no_param(TypeFamily::U128),
            UniScalar::I128 => DataType::new_no_param(TypeFamily::I128),
            UniScalar::F32 => DataType::new_no_param(TypeFamily::F32),
            UniScalar::F64 => DataType::new_no_param(TypeFamily::F64),
            UniScalar::Char => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "WIT char is not supported in procedure types or record fields: the host type system has no Char family, declare the field as string instead"
                ));
            }
            UniScalar::String => DataType::default_for(TypeFamily::String),
            UniScalar::Blob => DataType::new_no_param(TypeFamily::Binary),
            UniScalar::Numeric => DataType::default_for(TypeFamily::Numeric),
            UniScalar::Date => DataType::default_for(TypeFamily::Date),
            UniScalar::Time => DataType::default_for(TypeFamily::Time),
            UniScalar::Timestamp => DataType::default_for(TypeFamily::Timestamp),
            UniScalar::TimestampTz => DataType::default_for(TypeFamily::TimestampTz),
        };
        Ok(data_type)
    }
}

/// Collect every `Identifier` reference reachable from a field type,
/// descending into `list`/`option`/`box` inners, `tuple` items, `result`
/// ok/err types and inline record fields.
fn collect_identifiers(ty: &UniDataType, out: &mut Vec<String>) {
    match ty {
        UniDataType::Identifier(name) => out.push(name.clone()),
        UniDataType::Array(inner) | UniDataType::Option(inner) | UniDataType::Box(inner) => {
            collect_identifiers(inner, out);
        }
        UniDataType::Tuple(items) => {
            for item in items {
                collect_identifiers(item, out);
            }
        }
        UniDataType::Result(result) => {
            if let Some(ok) = &result.ok {
                collect_identifiers(ok, out);
            }
            if let Some(err) = &result.err {
                collect_identifiers(err, out);
            }
        }
        UniDataType::Record(inline) => {
            for field in &inline.record_fields {
                collect_identifiers(&field.field_type, out);
            }
        }
        UniDataType::Scalar(_) | UniDataType::Binary => {}
    }
}

/// Expand `--type-wit` paths into a deterministic list of `.wit` files.
fn collect_wit_files(paths: &[String]) -> RS<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path_string in paths {
        let path = PathBuf::from(path_string);
        let metadata = mudu_sys::fs::sync::sync_metadata(&path)?;
        if metadata.is_dir() {
            let mut dir_files = Vec::new();
            for entry in mudu_sys::fs::sync::sync_read_dir_entries(&path)? {
                if entry.file_type()?.is_file()
                    && entry.path().extension() == Some(OsStr::new("wit"))
                {
                    dir_files.push(entry.path());
                }
            }
            dir_files.sort();
            files.extend(dir_files);
        } else {
            if path.extension() != Some(OsStr::new("wit")) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "--type-wit path '{}' is not a .wit file or a directory",
                        path_string
                    )
                ));
            }
            files.push(path);
        }
    }
    Ok(files)
}
