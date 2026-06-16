pub mod object {
    use lazy_static::lazy_static;
    use mududb::common::result::RS;
    use mududb::contract::database::attr_field_access;
    use mududb::contract::database::attr_value::AttrValue;
    use mududb::contract::database::entity::Entity;
    use mududb::contract::database::entity_utils;
    use mududb::contract::database::sql_params::SQLParamMarker;
    use mududb::contract::tuple::datum_desc::DatumDesc;
    use mududb::contract::tuple::tuple_datum::TupleDatumMarker;
    use mududb::contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mududb::types::dat_binary::DatBinary;
    use mududb::types::dat_textual::DatTextual;
    use mududb::types::dat_type::DatType;
    use mududb::types::dat_type_id::DatTypeID;
    use mududb::types::dat_value::DatValue;
    use mududb::types::datum::{Datum, DatumDyn};

    // constant definition
    const CUSTOMER: &str = "customer";

    const C_ID: &str = "c_id";

    const C_D_ID: &str = "c_d_id";

    const C_W_ID: &str = "c_w_id";

    const C_FIRST: &str = "c_first";

    const C_LAST: &str = "c_last";

    const C_DISCOUNT: &str = "c_discount";

    const C_CREDIT: &str = "c_credit";

    const C_BALANCE: &str = "c_balance";

    const C_YTD_PAYMENT: &str = "c_ytd_payment";

    const C_PAYMENT_CNT: &str = "c_payment_cnt";

    const C_DELIVERY_CNT: &str = "c_delivery_cnt";

    const C_LAST_ORDER_ID: &str = "c_last_order_id";

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Customer {
        c_id: AttrCId,

        c_d_id: AttrCDId,

        c_w_id: AttrCWId,

        c_first: AttrCFirst,

        c_last: AttrCLast,

        c_discount: AttrCDiscount,

        c_credit: AttrCCredit,

        c_balance: AttrCBalance,

        c_ytd_payment: AttrCYtdPayment,

        c_payment_cnt: AttrCPaymentCnt,

        c_delivery_cnt: AttrCDeliveryCnt,

        c_last_order_id: AttrCLastOrderId,
    }

    pub struct CustomerParams {
        pub c_id: Option<i32>,
        pub c_d_id: Option<i32>,
        pub c_w_id: Option<i32>,
        pub c_first: Option<String>,
        pub c_last: Option<String>,
        pub c_discount: Option<i32>,
        pub c_credit: Option<String>,
        pub c_balance: Option<i32>,
        pub c_ytd_payment: Option<i32>,
        pub c_payment_cnt: Option<i32>,
        pub c_delivery_cnt: Option<i32>,
        pub c_last_order_id: Option<i32>,
    }

    impl TupleDatumMarker for Customer {}

    impl SQLParamMarker for Customer {}

    impl Customer {
        pub fn new(params: CustomerParams) -> Self {
            Self {
                c_id: AttrCId::from(params.c_id),

                c_d_id: AttrCDId::from(params.c_d_id),

                c_w_id: AttrCWId::from(params.c_w_id),

                c_first: AttrCFirst::from(params.c_first),

                c_last: AttrCLast::from(params.c_last),

                c_discount: AttrCDiscount::from(params.c_discount),

                c_credit: AttrCCredit::from(params.c_credit),

                c_balance: AttrCBalance::from(params.c_balance),

                c_ytd_payment: AttrCYtdPayment::from(params.c_ytd_payment),

                c_payment_cnt: AttrCPaymentCnt::from(params.c_payment_cnt),

                c_delivery_cnt: AttrCDeliveryCnt::from(params.c_delivery_cnt),

                c_last_order_id: AttrCLastOrderId::from(params.c_last_order_id),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_c_id(&mut self, c_id: i32) {
            self.c_id.update(c_id)
        }

        pub fn get_c_id(&self) -> &Option<i32> {
            self.c_id.get()
        }

        pub fn set_c_d_id(&mut self, c_d_id: i32) {
            self.c_d_id.update(c_d_id)
        }

        pub fn get_c_d_id(&self) -> &Option<i32> {
            self.c_d_id.get()
        }

        pub fn set_c_w_id(&mut self, c_w_id: i32) {
            self.c_w_id.update(c_w_id)
        }

        pub fn get_c_w_id(&self) -> &Option<i32> {
            self.c_w_id.get()
        }

        pub fn set_c_first(&mut self, c_first: String) {
            self.c_first.update(c_first)
        }

        pub fn get_c_first(&self) -> &Option<String> {
            self.c_first.get()
        }

        pub fn set_c_last(&mut self, c_last: String) {
            self.c_last.update(c_last)
        }

        pub fn get_c_last(&self) -> &Option<String> {
            self.c_last.get()
        }

        pub fn set_c_discount(&mut self, c_discount: i32) {
            self.c_discount.update(c_discount)
        }

        pub fn get_c_discount(&self) -> &Option<i32> {
            self.c_discount.get()
        }

        pub fn set_c_credit(&mut self, c_credit: String) {
            self.c_credit.update(c_credit)
        }

        pub fn get_c_credit(&self) -> &Option<String> {
            self.c_credit.get()
        }

        pub fn set_c_balance(&mut self, c_balance: i32) {
            self.c_balance.update(c_balance)
        }

        pub fn get_c_balance(&self) -> &Option<i32> {
            self.c_balance.get()
        }

        pub fn set_c_ytd_payment(&mut self, c_ytd_payment: i32) {
            self.c_ytd_payment.update(c_ytd_payment)
        }

        pub fn get_c_ytd_payment(&self) -> &Option<i32> {
            self.c_ytd_payment.get()
        }

        pub fn set_c_payment_cnt(&mut self, c_payment_cnt: i32) {
            self.c_payment_cnt.update(c_payment_cnt)
        }

        pub fn get_c_payment_cnt(&self) -> &Option<i32> {
            self.c_payment_cnt.get()
        }

        pub fn set_c_delivery_cnt(&mut self, c_delivery_cnt: i32) {
            self.c_delivery_cnt.update(c_delivery_cnt)
        }

        pub fn get_c_delivery_cnt(&self) -> &Option<i32> {
            self.c_delivery_cnt.get()
        }

        pub fn set_c_last_order_id(&mut self, c_last_order_id: i32) {
            self.c_last_order_id.update(c_last_order_id)
        }

        pub fn get_c_last_order_id(&self) -> &Option<i32> {
            self.c_last_order_id.get()
        }
    }

    impl Datum for Customer {
        fn dat_type() -> &'static DatType {
            lazy_static! {
                static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Customer>();
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

    impl DatumDyn for Customer {
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

    impl Entity for Customer {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrCId::datum_desc().clone(),
                    AttrCDId::datum_desc().clone(),
                    AttrCWId::datum_desc().clone(),
                    AttrCFirst::datum_desc().clone(),
                    AttrCLast::datum_desc().clone(),
                    AttrCDiscount::datum_desc().clone(),
                    AttrCCredit::datum_desc().clone(),
                    AttrCBalance::datum_desc().clone(),
                    AttrCYtdPayment::datum_desc().clone(),
                    AttrCPaymentCnt::datum_desc().clone(),
                    AttrCDeliveryCnt::datum_desc().clone(),
                    AttrCLastOrderId::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                C_ID => attr_field_access::attr_get_binary::<_>(self.c_id.get()),

                C_D_ID => attr_field_access::attr_get_binary::<_>(self.c_d_id.get()),

                C_W_ID => attr_field_access::attr_get_binary::<_>(self.c_w_id.get()),

                C_FIRST => attr_field_access::attr_get_binary::<_>(self.c_first.get()),

                C_LAST => attr_field_access::attr_get_binary::<_>(self.c_last.get()),

                C_DISCOUNT => attr_field_access::attr_get_binary::<_>(self.c_discount.get()),

                C_CREDIT => attr_field_access::attr_get_binary::<_>(self.c_credit.get()),

                C_BALANCE => attr_field_access::attr_get_binary::<_>(self.c_balance.get()),

                C_YTD_PAYMENT => attr_field_access::attr_get_binary::<_>(self.c_ytd_payment.get()),

                C_PAYMENT_CNT => attr_field_access::attr_get_binary::<_>(self.c_payment_cnt.get()),

                C_DELIVERY_CNT => {
                    attr_field_access::attr_get_binary::<_>(self.c_delivery_cnt.get())
                }

                C_LAST_ORDER_ID => {
                    attr_field_access::attr_get_binary::<_>(self.c_last_order_id.get())
                }

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                C_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_D_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_d_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_w_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_FIRST => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_first.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_LAST => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_last.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_DISCOUNT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_discount.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_CREDIT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_credit.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_BALANCE => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_balance.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_YTD_PAYMENT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_ytd_payment.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_PAYMENT_CNT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_payment_cnt.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_DELIVERY_CNT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_delivery_cnt.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                C_LAST_ORDER_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.c_last_order_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }

        fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
            match field {
                C_ID => attr_field_access::attr_get_value::<_>(self.c_id.get()),

                C_D_ID => attr_field_access::attr_get_value::<_>(self.c_d_id.get()),

                C_W_ID => attr_field_access::attr_get_value::<_>(self.c_w_id.get()),

                C_FIRST => attr_field_access::attr_get_value::<_>(self.c_first.get()),

                C_LAST => attr_field_access::attr_get_value::<_>(self.c_last.get()),

                C_DISCOUNT => attr_field_access::attr_get_value::<_>(self.c_discount.get()),

                C_CREDIT => attr_field_access::attr_get_value::<_>(self.c_credit.get()),

                C_BALANCE => attr_field_access::attr_get_value::<_>(self.c_balance.get()),

                C_YTD_PAYMENT => attr_field_access::attr_get_value::<_>(self.c_ytd_payment.get()),

                C_PAYMENT_CNT => attr_field_access::attr_get_value::<_>(self.c_payment_cnt.get()),

                C_DELIVERY_CNT => attr_field_access::attr_get_value::<_>(self.c_delivery_cnt.get()),

                C_LAST_ORDER_ID => {
                    attr_field_access::attr_get_value::<_>(self.c_last_order_id.get())
                }

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                C_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.c_id.get_mut(), value)?;
                }

                C_D_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.c_d_id.get_mut(), value)?;
                }

                C_W_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.c_w_id.get_mut(), value)?;
                }

                C_FIRST => {
                    attr_field_access::attr_set_value::<_, _>(self.c_first.get_mut(), value)?;
                }

                C_LAST => {
                    attr_field_access::attr_set_value::<_, _>(self.c_last.get_mut(), value)?;
                }

                C_DISCOUNT => {
                    attr_field_access::attr_set_value::<_, _>(self.c_discount.get_mut(), value)?;
                }

                C_CREDIT => {
                    attr_field_access::attr_set_value::<_, _>(self.c_credit.get_mut(), value)?;
                }

                C_BALANCE => {
                    attr_field_access::attr_set_value::<_, _>(self.c_balance.get_mut(), value)?;
                }

                C_YTD_PAYMENT => {
                    attr_field_access::attr_set_value::<_, _>(self.c_ytd_payment.get_mut(), value)?;
                }

                C_PAYMENT_CNT => {
                    attr_field_access::attr_set_value::<_, _>(self.c_payment_cnt.get_mut(), value)?;
                }

                C_DELIVERY_CNT => {
                    attr_field_access::attr_set_value::<_, _>(
                        self.c_delivery_cnt.get_mut(),
                        value,
                    )?;
                }

                C_LAST_ORDER_ID => {
                    attr_field_access::attr_set_value::<_, _>(
                        self.c_last_order_id.get_mut(),
                        value,
                    )?;
                }

                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCId {
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

    impl AttrValue<i32> for AttrCId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCDId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCDId {
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

    impl AttrValue<i32> for AttrCDId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_D_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCWId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCWId {
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

    impl AttrValue<i32> for AttrCWId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_W_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCFirst {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrCFirst {
        fn from(value: Option<String>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<String> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<String> {
            &mut self.value
        }

        fn set(&mut self, value: Option<String>) {
            self.value = value
        }

        fn update(&mut self, value: String) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<String> for AttrCFirst {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_FIRST
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCLast {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrCLast {
        fn from(value: Option<String>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<String> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<String> {
            &mut self.value
        }

        fn set(&mut self, value: Option<String>) {
            self.value = value
        }

        fn update(&mut self, value: String) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<String> for AttrCLast {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_LAST
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCDiscount {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCDiscount {
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

    impl AttrValue<i32> for AttrCDiscount {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_DISCOUNT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCCredit {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrCCredit {
        fn from(value: Option<String>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<String> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<String> {
            &mut self.value
        }

        fn set(&mut self, value: Option<String>) {
            self.value = value
        }

        fn update(&mut self, value: String) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<String> for AttrCCredit {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_CREDIT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCBalance {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCBalance {
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

    impl AttrValue<i32> for AttrCBalance {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_BALANCE
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCYtdPayment {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCYtdPayment {
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

    impl AttrValue<i32> for AttrCYtdPayment {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_YTD_PAYMENT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCPaymentCnt {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCPaymentCnt {
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

    impl AttrValue<i32> for AttrCPaymentCnt {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_PAYMENT_CNT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCDeliveryCnt {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCDeliveryCnt {
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

    impl AttrValue<i32> for AttrCDeliveryCnt {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_DELIVERY_CNT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrCLastOrderId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrCLastOrderId {
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

    impl AttrValue<i32> for AttrCLastOrderId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            CUSTOMER
        }

        fn attr_name() -> &'static str {
            C_LAST_ORDER_ID
        }
    }
}
