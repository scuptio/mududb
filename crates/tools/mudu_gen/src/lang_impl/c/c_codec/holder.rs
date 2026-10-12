use mudu::common::result::RS;
use mudu_binding::universal::uni_data_type::UniDataType;

use super::ctype::c_type;

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
