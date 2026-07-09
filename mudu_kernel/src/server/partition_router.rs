use std::ops::Bound;
use std::sync::Arc;

use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::build_tuple::build_tuple;
use mudu_contract::tuple::comparator::tuple_compare;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule::{PartitionBound, PartitionRuleDesc};
use crate::contract::table_desc::TableDesc;
use crate::x_engine::api::VecDatum;

pub const DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID: OID = 0;

pub struct PartitionRouter {
    meta_mgr: Arc<dyn MetaMgr>,
}

impl PartitionRouter {
    pub fn new(meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self { meta_mgr }
    }

    pub async fn route_exact_partition(
        &self,
        table_id: OID,
        table_desc: &TableDesc,
        key: &VecDatum,
    ) -> RS<Option<OID>> {
        let Some(binding) = self.meta_mgr.get_table_partition_binding(table_id).await? else {
            return Ok(Some(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID));
        };
        let rule = self
            .meta_mgr
            .get_partition_rule_by_id(binding.rule_id)
            .await?;
        let route_tuple = build_route_tuple(table_desc, &binding.ref_attr_indices, key)?;
        let route_desc = build_route_tuple_desc(table_desc, &binding.ref_attr_indices)?;

        for partition in &rule.partitions {
            let after_start = match &partition.start {
                PartitionBound::Unbounded => true,
                PartitionBound::Value(values) => {
                    let bound = build_partition_bound_tuple(&route_desc, values)?;
                    !tuple_compare(&route_desc, &route_tuple, &bound)?.is_lt()
                }
            };
            let before_end = match &partition.end {
                PartitionBound::Unbounded => true,
                PartitionBound::Value(values) => {
                    let bound = build_partition_bound_tuple(&route_desc, values)?;
                    tuple_compare(&route_desc, &route_tuple, &bound)?.is_lt()
                }
            };
            if after_start && before_end {
                return Ok(Some(partition.partition_id));
            }
        }

        Err(mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no partition matched table {} key", table_id)
        ))
    }

    pub async fn route_range_partitions(
        &self,
        table_id: OID,
        table_desc: &TableDesc,
        start: &Bound<Vec<(usize, Buf)>>,
        end: &Bound<Vec<(usize, Buf)>>,
    ) -> RS<Option<Vec<OID>>> {
        let Some(binding) = self.meta_mgr.get_table_partition_binding(table_id).await? else {
            return Ok(Some(vec![DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID]));
        };
        let rule = self
            .meta_mgr
            .get_partition_rule_by_id(binding.rule_id)
            .await?;
        let route_desc = build_route_tuple_desc(table_desc, &binding.ref_attr_indices)?;
        let start_tuple = build_route_bound_tuple(table_desc, &binding.ref_attr_indices, start)?;
        let end_tuple = build_route_bound_tuple(table_desc, &binding.ref_attr_indices, end)?;
        let mut partitions = Vec::new();
        for partition in &rule.partitions {
            if partition_overlaps(
                &rule,
                &route_desc,
                partition.partition_id,
                &start_tuple,
                &end_tuple,
            )? {
                partitions.push(partition.partition_id);
            }
        }
        Ok(Some(partitions))
    }

    pub fn route_rule_exact_partition(
        &self,
        rule: &PartitionRuleDesc,
        key_values: &[Vec<u8>],
    ) -> RS<OID> {
        let route_desc = build_rule_tuple_desc(&rule.key_types)?;
        let route_tuple = build_partition_bound_tuple(&route_desc, key_values)?;
        for partition in &rule.partitions {
            let after_start = match &partition.start {
                PartitionBound::Unbounded => true,
                PartitionBound::Value(values) => {
                    let bound = build_partition_bound_tuple(&route_desc, values)?;
                    !tuple_compare(&route_desc, &route_tuple, &bound)?.is_lt()
                }
            };
            let before_end = match &partition.end {
                PartitionBound::Unbounded => true,
                PartitionBound::Value(values) => {
                    let bound = build_partition_bound_tuple(&route_desc, values)?;
                    tuple_compare(&route_desc, &route_tuple, &bound)?.is_lt()
                }
            };
            if after_start && before_end {
                return Ok(partition.partition_id);
            }
        }
        Err(mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no partition matched rule {}", rule.name)
        ))
    }

    pub fn route_rule_range_partitions(
        &self,
        rule: &PartitionRuleDesc,
        start: &Bound<Vec<Vec<u8>>>,
        end: &Bound<Vec<Vec<u8>>>,
    ) -> RS<Vec<OID>> {
        let route_desc = build_rule_tuple_desc(&rule.key_types)?;
        let start_tuple = build_rule_bound_tuple(&route_desc, start)?;
        let end_tuple = build_rule_bound_tuple(&route_desc, end)?;
        let mut partitions = Vec::new();
        for partition in &rule.partitions {
            if partition_overlaps(
                rule,
                &route_desc,
                partition.partition_id,
                &start_tuple,
                &end_tuple,
            )? {
                partitions.push(partition.partition_id);
            }
        }
        Ok(partitions)
    }
}

fn build_rule_tuple_desc(key_types: &[TypeFamily]) -> RS<TupleBinaryDesc> {
    let types = key_types
        .iter()
        .map(|id| mudu_type::data_type::DataType::default_for(*id))
        .collect();
    TupleBinaryDesc::from(types)
}

fn build_rule_bound_tuple(
    route_desc: &TupleBinaryDesc,
    bound: &Bound<Vec<Vec<u8>>>,
) -> RS<Bound<Vec<u8>>> {
    match bound {
        Bound::Included(values) => Ok(Bound::Included(build_partition_bound_tuple(
            route_desc, values,
        )?)),
        Bound::Excluded(values) => Ok(Bound::Excluded(build_partition_bound_tuple(
            route_desc, values,
        )?)),
        Bound::Unbounded => Ok(Bound::Unbounded),
    }
}

fn partition_overlaps(
    rule: &PartitionRuleDesc,
    route_desc: &TupleBinaryDesc,
    partition_id: OID,
    start: &Bound<Vec<u8>>,
    end: &Bound<Vec<u8>>,
) -> RS<bool> {
    let partition = rule
        .partitions
        .iter()
        .find(|partition| partition.partition_id == partition_id)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such partition {}", partition_id)
            )
        })?;

    let start_ok = match (end, &partition.start) {
        (Bound::Unbounded, _) | (_, PartitionBound::Unbounded) => true,
        (Bound::Included(end), PartitionBound::Value(bound_start))
        | (Bound::Excluded(end), PartitionBound::Value(bound_start)) => {
            let start_tuple = build_partition_bound_tuple(route_desc, bound_start)?;
            !tuple_compare(route_desc, end, &start_tuple)?.is_le()
        }
    };
    let end_ok = match (start, &partition.end) {
        (Bound::Unbounded, _) | (_, PartitionBound::Unbounded) => true,
        (Bound::Included(start), PartitionBound::Value(bound_end))
        | (Bound::Excluded(start), PartitionBound::Value(bound_end)) => {
            let end_tuple = build_partition_bound_tuple(route_desc, bound_end)?;
            tuple_compare(route_desc, start, &end_tuple)?.is_lt()
        }
    };
    Ok(start_ok && end_ok)
}

fn build_route_tuple_desc(table_desc: &TableDesc, ref_attrs: &[usize]) -> RS<TupleBinaryDesc> {
    let mut fields = ref_attrs
        .iter()
        .map(|attr| {
            let field = table_desc.get_attr(*attr);
            (field.type_desc().clone(), field.datum_index())
        })
        .collect::<Vec<_>>();
    fields.sort_by_key(|(_, datum_index)| *datum_index);
    let types = fields.into_iter().map(|(data_type, _)| data_type).collect();
    TupleBinaryDesc::from(types)
}

fn build_route_tuple(table_desc: &TableDesc, ref_attrs: &[usize], key: &VecDatum) -> RS<Vec<u8>> {
    let mut values = Vec::with_capacity(ref_attrs.len());
    for attr in ref_attrs {
        let binary = key
            .data()
            .iter()
            .find_map(|(current_attr, binary)| (*current_attr == *attr).then(|| binary.clone()))
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("missing partition route attribute {}", attr)
                )
            })?;
        values.push(binary);
    }
    build_tuple(&values, &build_route_tuple_desc(table_desc, ref_attrs)?)
}

fn build_route_bound_tuple(
    table_desc: &TableDesc,
    ref_attrs: &[usize],
    bound: &Bound<Vec<(usize, Buf)>>,
) -> RS<Bound<Vec<u8>>> {
    match bound {
        Bound::Included(values) => Ok(Bound::Included(build_route_tuple(
            table_desc,
            ref_attrs,
            &VecDatum::new(values.clone()),
        )?)),
        Bound::Excluded(values) => Ok(Bound::Excluded(build_route_tuple(
            table_desc,
            ref_attrs,
            &VecDatum::new(values.clone()),
        )?)),
        Bound::Unbounded => Ok(Bound::Unbounded),
    }
}

fn build_partition_bound_tuple(route_desc: &TupleBinaryDesc, values: &[Vec<u8>]) -> RS<Vec<u8>> {
    if route_desc.field_count() != values.len() {
        return Err(mudu_error!(
            ErrorCode::InvalidTuple,
            "partition bound width mismatch"
        ));
    }
    let mut binaries = Vec::with_capacity(values.len());
    for (index, textual) in values.iter().enumerate() {
        let field_desc = route_desc.get_field_desc(index);
        let data_type = field_desc.type_obj();
        binaries.push(textual_to_binary(
            data_type.type_family(),
            data_type,
            textual,
        )?);
    }
    build_tuple(&binaries, route_desc)
}

fn textual_to_binary(
    data_type_id: TypeFamily,
    data_type: &mudu_type::data_type::DataType,
    raw: &[u8],
) -> RS<Vec<u8>> {
    let text = String::from_utf8(raw.to_vec())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "partition bound text is not utf8", e))?;
    let normalized = strip_text_literal_quotes(text.trim());
    let datum: Box<dyn DatumDyn> = match data_type_id {
        TypeFamily::I32 => Box::new(<i32 as mudu_type::datum::Datum>::from_textual(&normalized)?),
        TypeFamily::I64 => Box::new(<i64 as mudu_type::datum::Datum>::from_textual(&normalized)?),
        TypeFamily::I128 => Box::new(<i128 as mudu_type::datum::Datum>::from_textual(
            &normalized,
        )?),
        TypeFamily::U128 => Box::new(<u128 as mudu_type::datum::Datum>::from_textual(
            &normalized,
        )?),
        TypeFamily::F32 => Box::new(<f32 as mudu_type::datum::Datum>::from_textual(&normalized)?),
        TypeFamily::F64 => Box::new(<f64 as mudu_type::datum::Datum>::from_textual(&normalized)?),
        TypeFamily::String => Box::new(<String as mudu_type::datum::Datum>::from_textual(
            &normalized,
        )?),
        _ => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("partition bound type {:?} is not supported", data_type_id)
            ));
        }
    };
    datum.to_binary(data_type).map(|binary| binary.into())
}

fn strip_text_literal_quotes(input: &str) -> String {
    if input.len() >= 2 && input.starts_with('\'') && input.ends_with('\'') {
        input[1..input.len() - 1].to_string()
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::contract::partition_rule::RangePartitionDef;
    use crate::contract::partition_rule_binding::TablePartitionBinding;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_info::TableInfo;
    use async_trait::async_trait;
    use mudu::common::id::AttrIndex;
    use mudu::error::ErrorCode;
    use mudu_type::data_type_info::DataTypeInfo;

    struct TestMetaMgr {
        table_desc: Arc<TableDesc>,
        binding: Option<TablePartitionBinding>,
        rule: Option<PartitionRuleDesc>,
    }

    #[async_trait]
    impl MetaMgr for TestMetaMgr {
        async fn initialize(&self) -> RS<()> {
            Ok(())
        }
        async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
            if self.table_desc.id() == oid {
                Ok(self.table_desc.clone())
            } else {
                Err(mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such table {}", oid)
                ))
            }
        }

        async fn get_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>> {
            Ok((self.table_desc.name() == name).then(|| self.table_desc.clone()))
        }

        async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
            Ok(())
        }

        async fn drop_table(&self, _table_id: OID) -> RS<()> {
            Ok(())
        }

        async fn get_table_partition_binding(
            &self,
            _table_id: OID,
        ) -> RS<Option<TablePartitionBinding>> {
            Ok(self.binding.clone())
        }

        async fn get_partition_rule_by_id(&self, oid: OID) -> RS<PartitionRuleDesc> {
            match &self.rule {
                Some(rule) if rule.oid == oid => Ok(rule.clone()),
                _ => Err(mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such partition rule {}", oid)
                )),
            }
        }
    }

    impl Default for TestMetaMgr {
        fn default() -> Self {
            Self {
                table_desc: test_table_desc(),
                binding: None,
                rule: None,
            }
        }
    }

    fn test_table_desc() -> Arc<TableDesc> {
        TableInfo::new(SchemaTable::new(
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        ))
        .unwrap()
        .table_desc()
        .unwrap()
    }

    fn test_table_desc_two_key_cols() -> Arc<TableDesc> {
        TableInfo::new(SchemaTable::new(
            "t2".to_string(),
            vec![
                SchemaColumn::new(
                    "a".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
                SchemaColumn::new(
                    "b".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
            ],
            vec![0, 1],
            vec![],
        ))
        .unwrap()
        .table_desc()
        .unwrap()
    }

    fn v(text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }

    fn i32_value(n: i32) -> Vec<u8> {
        n.to_be_bytes().to_vec()
    }

    fn single_col_rule() -> PartitionRuleDesc {
        PartitionRuleDesc::new_range(
            "r".to_string(),
            vec![TypeFamily::I32],
            vec![
                RangePartitionDef::new(
                    "p1".to_string(),
                    PartitionBound::Value(vec![v("10")]),
                    PartitionBound::Value(vec![v("20")]),
                ),
                RangePartitionDef::new(
                    "p2".to_string(),
                    PartitionBound::Value(vec![v("20")]),
                    PartitionBound::Value(vec![v("30")]),
                ),
                RangePartitionDef::new(
                    "p3".to_string(),
                    PartitionBound::Value(vec![v("40")]),
                    PartitionBound::Value(vec![v("50")]),
                ),
            ],
        )
    }

    fn multi_col_rule() -> PartitionRuleDesc {
        PartitionRuleDesc::new_range(
            "r2".to_string(),
            vec![TypeFamily::I32, TypeFamily::I32],
            vec![
                RangePartitionDef::new(
                    "p1".to_string(),
                    PartitionBound::Value(vec![v("0"), v("0")]),
                    PartitionBound::Value(vec![v("10"), v("10")]),
                ),
                RangePartitionDef::new(
                    "p2".to_string(),
                    PartitionBound::Value(vec![v("10"), v("10")]),
                    PartitionBound::Value(vec![v("20"), v("20")]),
                ),
            ],
        )
    }

    fn partitioned_meta_mgr(
        table_desc: Arc<TableDesc>,
        rule: PartitionRuleDesc,
        ref_attrs: Vec<AttrIndex>,
    ) -> TestMetaMgr {
        let table_id = table_desc.id();
        TestMetaMgr {
            table_desc,
            binding: Some(TablePartitionBinding {
                table_id,
                rule_id: rule.oid,
                ref_attr_indices: ref_attrs,
            }),
            rule: Some(rule),
        }
    }

    #[test]
    fn route_exact_partition_unpartitioned_table_returns_default() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let table_desc = test_table_desc();
            let router = PartitionRouter::new(Arc::new(TestMetaMgr {
                table_desc: table_desc.clone(),
                binding: None,
                rule: None,
            }));
            let point = router
                .route_exact_partition(
                    table_desc.id(),
                    table_desc.as_ref(),
                    &VecDatum::new(vec![(0, i32_value(1))]),
                )
                .await
                .unwrap();
            assert_eq!(point, Some(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID));
        })
        .unwrap()
    }

    #[test]
    fn route_exact_partition_single_col_range() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let table_desc = test_table_desc();
            let rule = single_col_rule();
            let p1 = rule.partitions[0].partition_id;
            let router = PartitionRouter::new(Arc::new(partitioned_meta_mgr(
                table_desc.clone(),
                rule,
                vec![0],
            )));

            let matched = router
                .route_exact_partition(
                    table_desc.id(),
                    table_desc.as_ref(),
                    &VecDatum::new(vec![(0, i32_value(15))]),
                )
                .await
                .unwrap();
            assert_eq!(matched, Some(p1));

            for key in [5, 35, 55] {
                let err = router
                    .route_exact_partition(
                        table_desc.id(),
                        table_desc.as_ref(),
                        &VecDatum::new(vec![(0, i32_value(key))]),
                    )
                    .await
                    .unwrap_err();
                assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            }
        })
        .unwrap()
    }

    #[test]
    fn route_exact_partition_multi_col_range() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let table_desc = test_table_desc_two_key_cols();
            let rule = multi_col_rule();
            let p1 = rule.partitions[0].partition_id;
            let p2 = rule.partitions[1].partition_id;
            let router = PartitionRouter::new(Arc::new(partitioned_meta_mgr(
                table_desc.clone(),
                rule,
                vec![0, 1],
            )));

            let key = VecDatum::new(vec![(0, i32_value(5)), (1, i32_value(5))]);
            assert_eq!(
                router
                    .route_exact_partition(table_desc.id(), table_desc.as_ref(), &key)
                    .await
                    .unwrap(),
                Some(p1)
            );

            let key = VecDatum::new(vec![(0, i32_value(15)), (1, i32_value(15))]);
            assert_eq!(
                router
                    .route_exact_partition(table_desc.id(), table_desc.as_ref(), &key)
                    .await
                    .unwrap(),
                Some(p2)
            );
        })
        .unwrap()
    }

    #[test]
    fn route_range_partitions_overlaps_and_unbounded_and_unpartitioned() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let table_desc = test_table_desc();
            let rule = single_col_rule();
            let ids: Vec<OID> = rule.partitions.iter().map(|p| p.partition_id).collect();
            let router = PartitionRouter::new(Arc::new(partitioned_meta_mgr(
                table_desc.clone(),
                rule,
                vec![0],
            )));

            let overlap = router
                .route_range_partitions(
                    table_desc.id(),
                    table_desc.as_ref(),
                    &Bound::Included(vec![(0, i32_value(15))]),
                    &Bound::Excluded(vec![(0, i32_value(25))]),
                )
                .await
                .unwrap()
                .unwrap();
            assert_eq!(overlap, vec![ids[0], ids[1]]);

            let all = router
                .route_range_partitions(
                    table_desc.id(),
                    table_desc.as_ref(),
                    &Bound::Unbounded,
                    &Bound::Unbounded,
                )
                .await
                .unwrap()
                .unwrap();
            assert_eq!(all, ids);

            let unpartitioned = PartitionRouter::new(Arc::new(TestMetaMgr {
                table_desc: table_desc.clone(),
                binding: None,
                rule: None,
            }));
            let default = unpartitioned
                .route_range_partitions(
                    table_desc.id(),
                    table_desc.as_ref(),
                    &Bound::Included(vec![(0, i32_value(1))]),
                    &Bound::Excluded(vec![(0, i32_value(2))]),
                )
                .await
                .unwrap();
            assert_eq!(
                default,
                Some(vec![DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID])
            );
        })
        .unwrap()
    }

    #[test]
    fn route_rule_exact_partition_direct_rule() {
        let rule = single_col_rule();
        let router = PartitionRouter::new(Arc::new(TestMetaMgr::default()));
        let oid = router
            .route_rule_exact_partition(&rule, &[v("15")])
            .unwrap();
        assert_eq!(oid, rule.partitions[0].partition_id);
    }

    #[test]
    fn route_rule_range_partitions_direct_rule() {
        let rule = single_col_rule();
        let ids: Vec<OID> = rule.partitions.iter().map(|p| p.partition_id).collect();
        let router = PartitionRouter::new(Arc::new(TestMetaMgr::default()));

        let matched = router
            .route_rule_range_partitions(
                &rule,
                &Bound::Included(vec![v("15")]),
                &Bound::Excluded(vec![v("25")]),
            )
            .unwrap();
        assert_eq!(matched, vec![ids[0], ids[1]]);

        let all = router
            .route_rule_range_partitions(&rule, &Bound::Unbounded, &Bound::Unbounded)
            .unwrap();
        assert_eq!(all, ids);
    }

    #[test]
    fn route_rule_exact_partition_bound_count_mismatch_returns_invalid_tuple() {
        let rule = multi_col_rule();
        let router = PartitionRouter::new(Arc::new(TestMetaMgr::default()));
        let err = router
            .route_rule_exact_partition(&rule, &[v("5")])
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidTuple);
    }

    #[test]
    fn textual_to_binary_strips_quotes() {
        let rule = single_col_rule();
        let router = PartitionRouter::new(Arc::new(TestMetaMgr::default()));
        let oid = router
            .route_rule_exact_partition(&rule, &[b"'15'".to_vec()])
            .unwrap();
        assert_eq!(oid, rule.partitions[0].partition_id);
    }

    #[test]
    fn textual_to_binary_unsupported_type_returns_not_implemented() {
        let rule = PartitionRuleDesc::new_range(
            "date_rule".to_string(),
            vec![TypeFamily::Date],
            vec![RangePartitionDef::new(
                "p1".to_string(),
                PartitionBound::Value(vec![v("2024-01-01")]),
                PartitionBound::Unbounded,
            )],
        );
        let router = PartitionRouter::new(Arc::new(TestMetaMgr::default()));
        let err = router
            .route_rule_exact_partition(&rule, &[v("2024-01-01")])
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }
}
