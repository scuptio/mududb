//! Parameter templates produced by template-mode binding.
//!
//! Template-mode binding walks the same code path as immediate binding but
//! does not read parameter values: every placeholder (`?`) is recorded as an
//! ordered [`ParamSlot`] and every value position in the bound statement
//! becomes a [`TemplateDatum`] (either a literal encoded at bind time or a
//! slot reference). The resulting [`BoundTemplate`] is schema-dependent but
//! parameter-independent, so it can be cached keyed by SQL text plus catalog
//! version and re-executed with different parameters by filling the slots
//! (see `mudu_conn::plan_cache`).
//!
//! Invariants:
//! - Slots are recorded in parameter order; `slots[i].param_index` counts
//!   placeholders in the same order immediate binding would consume them
//!   (SELECT: WHERE only; UPDATE: SET items then WHERE key; INSERT: rows x
//!   columns; delta assignment placeholders count once).
//! - A `Slot` datum always fills to a non-NULL binary: immediate binding
//!   never maps a placeholder to SQL NULL, so nullability handling for slot
//!   positions is identical (NULL literals are `Const(None)` and are fully
//!   resolved at template-bind time).

use crate::sql::bound_stmt::{
    BoundCommand, BoundDelete, BoundInsert, BoundInsertRow, BoundPredicate, BoundQuery,
    BoundResidual, BoundSelect, BoundSelectItem, BoundSetValue, BoundStmt, BoundUpdate,
};
use crate::sql::value_codec::ValueCodec;
use crate::x_engine::api::DeltaOp;
use mudu::common::buf::Buf;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::type_family::TypeFamily;
use sql_parser::ast::expr_item::ExprValue;
use sql_parser::ast::expr_operator::ValueCompare;
use std::ops::Bound;

/// One placeholder occurrence in a template, in parameter order.
#[derive(Clone, Debug)]
pub struct ParamSlot {
    /// Index into [`SQLParams`] this placeholder consumes.
    pub param_index: u64,
    /// Column type the placeholder binds against; fill-time encoding uses the
    /// same coercion rules as immediate binding.
    pub data_type: DataType,
    /// Whether the slot feeds a delta operand (`SET col = col +|- ?`), which
    /// restricts the accepted parameter type families at fill time (same
    /// check immediate binding performs at bind time).
    pub delta_operand: bool,
}

/// A value position in a template: either a literal encoded at bind time or
/// a reference to a [`ParamSlot`] filled from the parameters at execution
/// time.
#[derive(Clone, Debug)]
pub enum TemplateDatum {
    /// Value fully determined at bind time; `None` is a NULL literal.
    Const(Option<Buf>),
    /// Filled from `slots[index]` at execution time.
    Slot(u32),
}

/// Collects [`ParamSlot`]s while template binding walks a statement.
#[derive(Default)]
pub(crate) struct SlotRecorder {
    slots: Vec<ParamSlot>,
    next_param_index: usize,
}

impl SlotRecorder {
    /// Records one placeholder against `data_type` and returns its slot index.
    pub(crate) fn push(&mut self, data_type: DataType, delta_operand: bool) -> u32 {
        let index = self.slots.len() as u32;
        self.slots.push(ParamSlot {
            param_index: self.next_param_index as u64,
            data_type,
            delta_operand,
        });
        self.next_param_index += 1;
        index
    }

    pub(crate) fn into_slots(self) -> Vec<ParamSlot> {
        self.slots
    }
}

/// Encodes one expression position for template binding: literals are encoded
/// immediately, placeholders become slot references.
pub(crate) fn template_from_expr(
    expr: &ExprValue,
    data_type: &DataType,
    recorder: &mut SlotRecorder,
) -> RS<TemplateDatum> {
    match expr {
        ExprValue::ValueLiteral(literal) => Ok(TemplateDatum::Const(
            ValueCodec::binary_from_literal(literal, data_type)?,
        )),
        ExprValue::ValuePlaceholder => {
            Ok(TemplateDatum::Slot(recorder.push(data_type.clone(), false)))
        }
    }
}

/// A bound statement with parameter placeholders kept as slots.
#[derive(Clone, Debug)]
pub struct BoundTemplate {
    pub stmt: StmtTemplate,
    /// Ordered placeholder slots; `TemplateDatum::Slot(i)` indexes into this.
    pub slots: Vec<ParamSlot>,
}

/// Template variants of the DML statements (DDL is never templated).
#[derive(Clone, Debug)]
pub enum StmtTemplate {
    Select(SelectTemplate),
    Insert(InsertTemplate),
    Update(UpdateTemplate),
    Delete(DeleteTemplate),
}

/// Template form of [`BoundSelect`].
#[derive(Clone, Debug)]
pub struct SelectTemplate {
    pub table_id: OID,
    pub select_items: Vec<BoundSelectItem>,
    pub tuple_desc: TupleFieldDesc,
    pub predicate: PredicateTemplate,
    pub residual: Vec<ResidualTemplate>,
    /// Whether the table has fs-bound columns (recorded for symmetry with the
    /// write templates; reads have no fs hook).
    pub has_fs_columns: bool,
}

/// Template form of [`BoundPredicate`].
#[derive(Clone, Debug)]
pub enum PredicateTemplate {
    True,
    KeyEq {
        key: Vec<(AttrIndex, TemplateDatum)>,
    },
    KeyPrefixEq {
        prefix: Vec<(AttrIndex, TemplateDatum)>,
    },
    KeyRange {
        start: Bound<Vec<(AttrIndex, TemplateDatum)>>,
        end: Bound<Vec<(AttrIndex, TemplateDatum)>>,
    },
}

/// Template form of [`BoundResidual`].
#[derive(Clone, Debug)]
pub struct ResidualTemplate {
    pub attr: AttrIndex,
    pub op: ValueCompare,
    pub literal: TemplateDatum,
}

/// Template form of [`BoundInsert`].
#[derive(Clone, Debug)]
pub struct InsertTemplate {
    pub table_id: OID,
    pub rows: Vec<InsertRowTemplate>,
    /// Tables with fs-bound columns keep the fs DML hook, so they are
    /// classified `Other` and executed through the regular planner path.
    pub has_fs_columns: bool,
}

/// Template form of [`BoundInsertRow`]; every datum is non-NULL by
/// construction (NULL literals are rejected or dropped at template-bind time).
#[derive(Clone, Debug)]
pub struct InsertRowTemplate {
    pub key: Vec<(AttrIndex, TemplateDatum)>,
    pub value: Vec<(AttrIndex, TemplateDatum)>,
}

/// Template form of [`BoundUpdate`].
#[derive(Clone, Debug)]
pub struct UpdateTemplate {
    pub table_id: OID,
    /// Complete primary key (enforced by template binding, same as immediate
    /// binding).
    pub key: Vec<(AttrIndex, TemplateDatum)>,
    pub value: Vec<(AttrIndex, SetValueTemplate)>,
    /// Tables with fs-bound columns keep the fs DML hook, so they are
    /// classified `Other` and executed through the regular planner path.
    pub has_fs_columns: bool,
}

/// Template form of [`BoundSetValue`].
#[derive(Clone, Debug)]
pub enum SetValueTemplate {
    /// Absolute value; always fills to a non-NULL binary (NULL assignments
    /// are resolved at template-bind time).
    Absolute(TemplateDatum),
    /// Restricted expression assignment `SET col = col <+|-> <literal or ?>`.
    Delta { op: DeltaOp, operand: TemplateDatum },
}

/// Template form of [`BoundDelete`].
#[derive(Clone, Debug)]
pub struct DeleteTemplate {
    pub table_id: OID,
    pub key: Vec<(AttrIndex, TemplateDatum)>,
}

/// Execution classification of a cached template.
#[derive(Clone, Debug)]
pub enum PlanClass {
    /// Point read: full primary-key equality, no residual filter, no
    /// aggregate, plain column projection with unique attributes. Executed as
    /// one `XContract::read_key` plus result materialization.
    PointRead { select: Vec<AttrIndex> },
    /// Point update on a table without fs-bound columns: executed as one
    /// `XContract::update` (absolute and delta assignments split like the
    /// planner does).
    PointUpdate,
    /// Point insert on a table without fs-bound columns: executed as one
    /// `XContract::insert` per row.
    PointInsert,
    /// Everything else: fill the template and feed the resulting
    /// [`BoundStmt`] to the regular planner (saves binding only).
    Other,
}

impl BoundTemplate {
    pub fn new(stmt: StmtTemplate, slots: Vec<ParamSlot>) -> Self {
        Self { stmt, slots }
    }

    /// Classifies the template for the plan-cache fast paths.
    pub(crate) fn classify(&self) -> PlanClass {
        match &self.stmt {
            StmtTemplate::Select(select) => {
                if !matches!(select.predicate, PredicateTemplate::KeyEq { .. })
                    || !select.residual.is_empty()
                {
                    return PlanClass::Other;
                }
                // `read_key` projects the select list verbatim, so only plain
                // column projections with unique attributes are equivalent to
                // the executor path; duplicates and aggregates go through the
                // planner instead.
                let mut attrs: Vec<AttrIndex> = Vec::with_capacity(select.select_items.len());
                for item in &select.select_items {
                    match item {
                        BoundSelectItem::Column(column) => {
                            if attrs.contains(&column.attr) {
                                return PlanClass::Other;
                            }
                            attrs.push(column.attr);
                        }
                        BoundSelectItem::Aggregate(_) => return PlanClass::Other,
                    }
                }
                PlanClass::PointRead { select: attrs }
            }
            StmtTemplate::Update(update) if !update.has_fs_columns => PlanClass::PointUpdate,
            StmtTemplate::Insert(insert) if !insert.has_fs_columns => PlanClass::PointInsert,
            _ => PlanClass::Other,
        }
    }

    /// Fills every slot from `params` and produces the ordinary bound
    /// statement immediate binding would have produced.
    pub fn fill(&self, params: &dyn SQLParams) -> RS<BoundStmt> {
        match &self.stmt {
            StmtTemplate::Select(select) => Ok(BoundStmt::Query(BoundQuery::Select(
                select.fill(&self.slots, params)?,
            ))),
            StmtTemplate::Insert(insert) => Ok(BoundStmt::Command(BoundCommand::Insert(
                insert.fill(&self.slots, params)?,
            ))),
            StmtTemplate::Update(update) => Ok(BoundStmt::Command(BoundCommand::Update(
                update.fill(&self.slots, params)?,
            ))),
            StmtTemplate::Delete(delete) => Ok(BoundStmt::Command(BoundCommand::Delete(
                delete.fill(&self.slots, params)?,
            ))),
        }
    }
}

/// Fills one slot from the parameters, applying the same coercion rules as
/// immediate binding (`ValueCodec::binary_from_param`, which covers the
/// String -> NUMERIC wire form). Delta operand slots re-check the accepted
/// parameter families exactly as immediate binding does at bind time.
fn fill_slot(slot: &ParamSlot, params: &dyn SQLParams) -> RS<Buf> {
    let datum = params.get_idx(slot.param_index).ok_or_else(|| {
        mudu_error!(
            ER::IndexOutOfRange,
            format!("missing parameter {}", slot.param_index)
        )
    })?;
    if slot.delta_operand {
        match datum.type_family()? {
            TypeFamily::I32 | TypeFamily::I64 | TypeFamily::I128 | TypeFamily::U128 => {}
            // NUMERIC params arrive type-erased as strings over the wire.
            TypeFamily::String if slot.data_type.type_family() == TypeFamily::Numeric => {}
            _ => {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    "expression updates are not implemented \
                     (only `SET col = col +|- <integer literal or ?>` is supported)"
                ))
            }
        }
    }
    ValueCodec::binary_from_param(datum, &slot.data_type)
}

impl TemplateDatum {
    /// Fills the datum; `None` is a NULL literal (slots never fill to NULL).
    fn fill(&self, slots: &[ParamSlot], params: &dyn SQLParams) -> RS<Option<Buf>> {
        match self {
            TemplateDatum::Const(value) => Ok(value.clone()),
            TemplateDatum::Slot(index) => {
                let slot = slots.get(*index as usize).ok_or_else(|| {
                    mudu_error!(ER::Internal, format!("template slot {index} out of range"))
                })?;
                Ok(Some(fill_slot(slot, params)?))
            }
        }
    }

    /// Fills a datum position that is non-NULL by construction (keys,
    /// absolute update values, delta operands, insert values).
    pub(crate) fn fill_some(&self, slots: &[ParamSlot], params: &dyn SQLParams) -> RS<Buf> {
        self.fill(slots, params)?
            .ok_or_else(|| mudu_error!(ER::Internal, "template datum unexpectedly NULL"))
    }
}

/// Fills a `(attr, datum)` pair list (keys, insert values).
pub(crate) fn fill_pairs(
    pairs: &[(AttrIndex, TemplateDatum)],
    slots: &[ParamSlot],
    params: &dyn SQLParams,
) -> RS<Vec<(AttrIndex, Buf)>> {
    pairs
        .iter()
        .map(|(attr, datum)| Ok((*attr, datum.fill_some(slots, params)?)))
        .collect()
}

impl SelectTemplate {
    fn fill(&self, slots: &[ParamSlot], params: &dyn SQLParams) -> RS<BoundSelect> {
        Ok(BoundSelect {
            table_id: self.table_id,
            select_items: self.select_items.clone(),
            tuple_desc: self.tuple_desc.clone(),
            predicate: self.predicate.fill(slots, params)?,
            residual: self
                .residual
                .iter()
                .map(|residual| {
                    Ok(BoundResidual {
                        attr: residual.attr,
                        op: residual.op,
                        literal: residual.literal.fill(slots, params)?,
                    })
                })
                .collect::<RS<Vec<_>>>()?,
        })
    }
}

impl PredicateTemplate {
    fn fill(&self, slots: &[ParamSlot], params: &dyn SQLParams) -> RS<BoundPredicate> {
        match self {
            PredicateTemplate::True => Ok(BoundPredicate::True),
            PredicateTemplate::KeyEq { key } => Ok(BoundPredicate::KeyEq {
                key: fill_pairs(key, slots, params)?,
            }),
            PredicateTemplate::KeyPrefixEq { prefix } => Ok(BoundPredicate::KeyPrefixEq {
                prefix: fill_pairs(prefix, slots, params)?,
            }),
            PredicateTemplate::KeyRange { start, end } => Ok(BoundPredicate::KeyRange {
                start: fill_bound(start, slots, params)?,
                end: fill_bound(end, slots, params)?,
            }),
        }
    }
}

fn fill_bound(
    bound: &Bound<Vec<(AttrIndex, TemplateDatum)>>,
    slots: &[ParamSlot],
    params: &dyn SQLParams,
) -> RS<Bound<Vec<(AttrIndex, Buf)>>> {
    Ok(match bound {
        Bound::Included(pairs) => Bound::Included(fill_pairs(pairs, slots, params)?),
        Bound::Excluded(pairs) => Bound::Excluded(fill_pairs(pairs, slots, params)?),
        Bound::Unbounded => Bound::Unbounded,
    })
}

impl InsertTemplate {
    fn fill(&self, slots: &[ParamSlot], params: &dyn SQLParams) -> RS<BoundInsert> {
        Ok(BoundInsert {
            table_id: self.table_id,
            rows: self
                .rows
                .iter()
                .map(|row| {
                    Ok(BoundInsertRow {
                        key: fill_pairs(&row.key, slots, params)?,
                        value: fill_pairs(&row.value, slots, params)?,
                    })
                })
                .collect::<RS<Vec<_>>>()?,
        })
    }
}

impl UpdateTemplate {
    fn fill(&self, slots: &[ParamSlot], params: &dyn SQLParams) -> RS<BoundUpdate> {
        Ok(BoundUpdate {
            table_id: self.table_id,
            key: fill_pairs(&self.key, slots, params)?,
            value: self
                .value
                .iter()
                .map(|(attr, set_value)| {
                    let set_value = match set_value {
                        SetValueTemplate::Absolute(datum) => {
                            BoundSetValue::Absolute(datum.fill_some(slots, params)?)
                        }
                        SetValueTemplate::Delta { op, operand } => BoundSetValue::Delta {
                            op: *op,
                            literal: operand.fill_some(slots, params)?,
                        },
                    };
                    Ok((*attr, set_value))
                })
                .collect::<RS<Vec<_>>>()?,
        })
    }
}

impl DeleteTemplate {
    fn fill(&self, slots: &[ParamSlot], params: &dyn SQLParams) -> RS<BoundDelete> {
        Ok(BoundDelete {
            table_id: self.table_id,
            key: fill_pairs(&self.key, slots, params)?,
        })
    }
}
