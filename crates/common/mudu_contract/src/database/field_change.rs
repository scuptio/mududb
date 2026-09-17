//! `database::field_change` module.
//!
//! [`FieldChange`] is the partial-update change type used by the entity
//! changesets that `mgen` generates for each table. A changeset struct holds
//! one `FieldChange<T>` per non-key column:
//!
//! - [`FieldChange::Unchanged`] — the column is not touched by the update;
//!   it is omitted from the generated `UPDATE ... SET` clause.
//! - [`FieldChange::Set(v)`] — the column is updated to `v`.
//!
//! Nullable columns use `FieldChange<Option<T>>`, so `Set(None)` writes a
//! SQL `NULL` while `Unchanged` leaves the column untouched.

/// Per-column change of a partial entity update; see the module documentation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FieldChange<T> {
    /// The column is not touched by the update.
    #[default]
    Unchanged,
    /// The column is updated to the contained value.
    Set(T),
}

impl<T> FieldChange<T> {
    /// Returns `true` when the column is not touched by the update.
    pub fn is_unchanged(&self) -> bool {
        matches!(self, FieldChange::Unchanged)
    }

    /// Returns `true` when the column is updated to a new value.
    pub fn is_set(&self) -> bool {
        matches!(self, FieldChange::Set(_))
    }
}

impl<T> From<T> for FieldChange<T> {
    fn from(value: T) -> Self {
        FieldChange::Set(value)
    }
}
