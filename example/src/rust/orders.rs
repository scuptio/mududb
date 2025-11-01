pub mod object {
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu::database::attr_binary::AttrBinary;
    use mudu::database::attr_set_get::{attr_get_binary, attr_set_binary};
    use mudu::database::attr_value::AttrValue;
    use mudu::database::record::Record;
    use mudu::database::record_convert_tuple::{record_from_tuple, record_to_tuple};
    use mudu::tuple::datum_convert::{datum_from_binary, datum_to_binary};
    use mudu::tuple::tuple_field::TupleField;
    use mudu::tuple::tuple_field_desc::TupleFieldDesc;

    const TABLE_ORDERS: &str = "orders";
    const COLUMN_ORDER_ID: &str = "order_id";
    const COLUMN_USER_ID: &str = "user_id";
    const COLUMN_MERCH_ID: &str = "merch_id";
    const COLUMN_AMOUNT: &str = "amount";
    const COLUMN_CREATED_AT: &str = "created_at";

    pub struct Orders {
        order_id: Option<AttrOrderId>,
        user_id: Option<AttrUserId>,
        merch_id: Option<AttrMerchId>,
        amount: Option<AttrAmount>,
        created_at: Option<AttrCreatedAt>,
    }

    impl Orders {
        pub fn new(
            order_id: AttrOrderId,
            user_id: AttrUserId,
            merch_id: AttrMerchId,
            amount: AttrAmount,
            created_at: AttrCreatedAt,
        ) -> Self {
            let s = Self {
                order_id: Some(order_id),
                user_id: Some(user_id),
                merch_id: Some(merch_id),
                amount: Some(amount),
                created_at: Some(created_at),
            };
            s
        }

        pub fn set_order_id(&mut self, order_id: AttrOrderId) {
            self.order_id = Some(order_id);
        }

        pub fn get_order_id(&self) -> &Option<AttrOrderId> {
            &self.order_id
        }

        pub fn set_user_id(&mut self, user_id: AttrUserId) {
            self.user_id = Some(user_id);
        }

        pub fn get_user_id(&self) -> &Option<AttrUserId> {
            &self.user_id
        }

        pub fn set_merch_id(&mut self, merch_id: AttrMerchId) {
            self.merch_id = Some(merch_id);
        }

        pub fn get_merch_id(&self) -> &Option<AttrMerchId> {
            &self.merch_id
        }

        pub fn set_amount(&mut self, amount: AttrAmount) {
            self.amount = Some(amount);
        }

        pub fn get_amount(&self) -> &Option<AttrAmount> {
            &self.amount
        }

        pub fn set_created_at(&mut self, created_at: AttrCreatedAt) {
            self.created_at = Some(created_at);
        }

        pub fn get_created_at(&self) -> &Option<AttrCreatedAt> {
            &self.created_at
        }
    }

    impl Record for Orders {
        fn new_empty() -> Self {
            let s = Self {
                order_id: None,
                user_id: None,
                merch_id: None,
                amount: None,
                created_at: None,
            };
            s
        }
        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrOrderId::datum_desc().clone(),
                    AttrUserId::datum_desc().clone(),
                    AttrMerchId::datum_desc().clone(),
                    AttrAmount::datum_desc().clone(),
                    AttrCreatedAt::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn from_tuple<T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(row: T, desc: D) -> RS<Self> {
            record_from_tuple::<Self, T, D>(row, desc)
        }

        fn to_tuple<D: AsRef<TupleFieldDesc>>(&self, desc: D) -> RS<TupleField> {
            record_to_tuple(self, desc)
        }

        fn get_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_ORDER_ID => attr_get_binary(&self.order_id),
                COLUMN_USER_ID => attr_get_binary(&self.user_id),
                COLUMN_MERCH_ID => attr_get_binary(&self.merch_id),
                COLUMN_AMOUNT => attr_get_binary(&self.amount),
                COLUMN_CREATED_AT => attr_get_binary(&self.created_at),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_ORDER_ID => {
                    attr_set_binary(&mut self.order_id, binary.as_ref())?;
                }
                COLUMN_USER_ID => {
                    attr_set_binary(&mut self.user_id, binary.as_ref())?;
                }
                COLUMN_MERCH_ID => {
                    attr_set_binary(&mut self.merch_id, binary.as_ref())?;
                }
                COLUMN_AMOUNT => {
                    attr_set_binary(&mut self.amount, binary.as_ref())?;
                }
                COLUMN_CREATED_AT => {
                    attr_set_binary(&mut self.created_at, binary.as_ref())?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    pub struct AttrOrderId {
        value: i32,
    }

    impl AttrOrderId {}

    impl AttrBinary for AttrOrderId {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrOrderId {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_ORDER_ID
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrUserId {
        value: i32,
    }

    impl AttrUserId {}

    impl AttrBinary for AttrUserId {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrUserId {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_USER_ID
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrMerchId {
        value: i32,
    }

    impl AttrMerchId {}

    impl AttrBinary for AttrMerchId {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrMerchId {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_MERCH_ID
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrAmount {
        value: i32,
    }

    impl AttrAmount {}

    impl AttrBinary for AttrAmount {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrAmount {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_AMOUNT
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrCreatedAt {
        value: i32,
    }

    impl AttrCreatedAt {}

    impl AttrBinary for AttrCreatedAt {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrCreatedAt {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_CREATED_AT
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }
} // end mod object
