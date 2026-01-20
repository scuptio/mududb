pub mod object {
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu_contract::database::attr_field_access;
    use mudu_contract::database::attr_value::AttrValue;
    use mudu_contract::database::entity::Entity;
    use mudu_contract::database::entity_utils;
    use mudu_contract::database::sql_params::SQLParamMarker;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_datum::TupleDatumMarker;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_type::dat_binary::DatBinary;
    use mudu_type::dat_textual::DatTextual;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_value::DatValue;
    use mudu_type::datum::{Datum, DatumDyn};

    // constant definition
    const ORDERS: &str = "orders";

    const ORDER_ID: &str = "order_id";

    const USER_ID: &str = "user_id";

    const MERCH_ID: &str = "merch_id";

    const AMOUNT: &str = "amount";

    const CREATED_AT: &str = "created_at";


    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Orders {
        order_id: AttrOrderId,

        user_id: AttrUserId,

        merch_id: AttrMerchId,

        amount: AttrAmount,

        created_at: AttrCreatedAt,

    }

    impl TupleDatumMarker for Orders {}

    impl SQLParamMarker for Orders {}

    impl Orders {
        pub fn new(
            order_id: Option<i32>,
            user_id: Option<i32>,
            merch_id: Option<i32>,
            amount: Option<i32>,
            created_at: Option<i32>,
        ) -> Self {
            let s = Self {
                order_id: AttrOrderId::from(order_id),

                user_id: AttrUserId::from(user_id),

                merch_id: AttrMerchId::from(merch_id),

                amount: AttrAmount::from(amount),

                created_at: AttrCreatedAt::from(created_at),

            };
            s
        }

        pub fn new_empty() -> Self {
            Self::default()
        }


        pub fn set_order_id(
            &mut self,
            order_id: i32,
        ) {
            self.order_id.update(order_id)
        }

        pub fn get_order_id(
            &self,
        ) -> &Option<i32> {
            self.order_id.get()
        }

        pub fn set_user_id(
            &mut self,
            user_id: i32,
        ) {
            self.user_id.update(user_id)
        }

        pub fn get_user_id(
            &self,
        ) -> &Option<i32> {
            self.user_id.get()
        }

        pub fn set_merch_id(
            &mut self,
            merch_id: i32,
        ) {
            self.merch_id.update(merch_id)
        }

        pub fn get_merch_id(
            &self,
        ) -> &Option<i32> {
            self.merch_id.get()
        }

        pub fn set_amount(
            &mut self,
            amount: i32,
        ) {
            self.amount.update(amount)
        }

        pub fn get_amount(
            &self,
        ) -> &Option<i32> {
            self.amount.get()
        }

        pub fn set_created_at(
            &mut self,
            created_at: i32,
        ) {
            self.created_at.update(created_at)
        }

        pub fn get_created_at(
            &self,
        ) -> &Option<i32> {
            self.created_at.get()
        }
    }

    impl Datum for Orders {
        fn dat_type() -> &'static DatType {
            lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Orders>();
        }
            &DAT_TYPE
        }

        fn from_binary(binary: &[u8]) -> RS<Self> {
            entity_utils::entity_from_binary(binary)
        }

        fn from_value(value: &DatValue) -> RS<Self> {
            entity_utils::entity_from_value(value)
        }

        fn from_textual(textual: &str) -> RS<Self> {
            entity_utils::entity_from_textual(textual)
        }
    }

    impl DatumDyn for Orders {
        fn dat_type_id(&self) -> RS<DatTypeID> {
            entity_utils::entity_dat_type_id()
        }

        fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary> {
            entity_utils::entity_to_binary(self, dat_type)
        }

        fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual> {
            entity_utils::entity_to_textual(self, dat_type)
        }

        fn to_value(&self, dat_type: &DatType) -> RS<DatValue> {
            entity_utils::entity_to_value(self, dat_type)
        }

        fn clone_boxed(&self) -> Box<dyn DatumDyn> {
            entity_utils::entity_clone_boxed(self)
        }
    }

    impl Entity for Orders {
        fn new_empty() -> Self {
            Self::new_empty()
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

        fn object_name() -> &'static str {
            ORDERS
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                ORDER_ID => {
                    attr_field_access::attr_get_binary::<_>(self.order_id.get())
                }

                USER_ID => {
                    attr_field_access::attr_get_binary::<_>(self.user_id.get())
            }

                MERCH_ID => {
                    attr_field_access::attr_get_binary::<_>(self.merch_id.get())
                }

                AMOUNT => {
                    attr_field_access::attr_get_binary::<_>(self.amount.get())
                }

                CREATED_AT => {
                    attr_field_access::attr_get_binary::<_>(self.created_at.get())
                }

                _ => { panic!("unknown name"); }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                ORDER_ID => {
                    attr_field_access::attr_set_binary::<_, _>(self.order_id.get_mut(), binary.as_ref())?;
                }

                USER_ID => {
                    attr_field_access::attr_set_binary::<_, _>(self.user_id.get_mut(), binary.as_ref())?;
                }

                MERCH_ID => {
                    attr_field_access::attr_set_binary::<_, _>(self.merch_id.get_mut(), binary.as_ref())?;
                }

                AMOUNT => {
                    attr_field_access::attr_set_binary::<_, _>(self.amount.get_mut(), binary.as_ref())?;
                }

                CREATED_AT => {
                    attr_field_access::attr_set_binary::<_, _>(self.created_at.get_mut(), binary.as_ref())?;
                }

                _ => { panic!("unknown name"); }
            }
            Ok(())
        }

        fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
            match field {
                ORDER_ID => {
                    attr_field_access::attr_get_value::<_>(self.order_id.get())
                }

                USER_ID => {
                    attr_field_access::attr_get_value::<_>(self.user_id.get())
                }

                MERCH_ID => {
                    attr_field_access::attr_get_value::<_>(self.merch_id.get())
                }

                AMOUNT => {
                    attr_field_access::attr_get_value::<_>(self.amount.get())
            }

                CREATED_AT => {
                    attr_field_access::attr_get_value::<_>(self.created_at.get())
                }

                _ => { panic!("unknown name"); }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                ORDER_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.order_id.get_mut(), value)?;
                }

                USER_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.user_id.get_mut(), value)?;
                }

                MERCH_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.merch_id.get_mut(), value)?;
                }

                AMOUNT => {
                    attr_field_access::attr_set_value::<_, _>(self.amount.get_mut(), value)?;
                }

                CREATED_AT => {
                    attr_field_access::attr_set_value::<_, _>(self.created_at.get_mut(), value)?;
                }

                _ => { panic!("unknown name"); }
            }
            Ok(())
        }
    }


    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOrderId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOrderId {
        fn from(value: Option<i32>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<i32> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<i32> {
            &mut self.value
        }

        fn set(&mut self, value: Option<i32>) {
            self.value = value
        }

        fn update(&mut self, value: i32) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<i32> for AttrOrderId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            ORDER_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrUserId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrUserId {
        fn from(value: Option<i32>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<i32> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<i32> {
            &mut self.value
        }

        fn set(&mut self, value: Option<i32>) {
            self.value = value
        }

        fn update(&mut self, value: i32) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<i32> for AttrUserId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            USER_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrMerchId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrMerchId {
        fn from(value: Option<i32>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<i32> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<i32> {
            &mut self.value
        }

        fn set(&mut self, value: Option<i32>) {
            self.value = value
        }

        fn update(&mut self, value: i32) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<i32> for AttrMerchId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            MERCH_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrAmount {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrAmount {
        fn from(value: Option<i32>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<i32> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<i32> {
            &mut self.value
        }

        fn set(&mut self, value: Option<i32>) {
            self.value = value
        }

        fn update(&mut self, value: i32) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<i32> for AttrAmount {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            AMOUNT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCreatedAt {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCreatedAt {
        fn from(value: Option<i32>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<i32> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<i32> {
            &mut self.value
        }

        fn set(&mut self, value: Option<i32>) {
            self.value = value
        }

        fn update(&mut self, value: i32) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<i32> for AttrCreatedAt {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            CREATED_AT
        }
    }
}