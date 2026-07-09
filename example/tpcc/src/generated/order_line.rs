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
    use mududb::types::data_binary::DataBinary;
    use mududb::types::data_textual::DataTextual;
    use mududb::types::data_type::DataType;
    use mududb::types::data_value::DataValue;
    use mududb::types::datum::{Datum, DatumDyn};
    use mududb::types::type_family::TypeFamily;

    // constant definition
    const ORDER_LINE: &str = "order_line";

    const OL_O_ID: &str = "ol_o_id";

    const OL_D_ID: &str = "ol_d_id";

    const OL_W_ID: &str = "ol_w_id";

    const OL_NUMBER: &str = "ol_number";

    const OL_I_ID: &str = "ol_i_id";

    const OL_SUPPLY_W_ID: &str = "ol_supply_w_id";

    const OL_DELIVERY_D: &str = "ol_delivery_d";

    const OL_QUANTITY: &str = "ol_quantity";

    const OL_AMOUNT: &str = "ol_amount";

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct OrderLine {
        ol_o_id: AttrOlOId,

        ol_d_id: AttrOlDId,

        ol_w_id: AttrOlWId,

        ol_number: AttrOlNumber,

        ol_i_id: AttrOlIId,

        ol_supply_w_id: AttrOlSupplyWId,

        ol_delivery_d: AttrOlDeliveryD,

        ol_quantity: AttrOlQuantity,

        ol_amount: AttrOlAmount,
    }

    impl TupleDatumMarker for OrderLine {}

    impl SQLParamMarker for OrderLine {}

    impl OrderLine {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            ol_o_id: Option<i32>,
            ol_d_id: Option<i32>,
            ol_w_id: Option<i32>,
            ol_number: Option<i32>,
            ol_i_id: Option<i32>,
            ol_supply_w_id: Option<i32>,
            ol_delivery_d: Option<String>,
            ol_quantity: Option<i32>,
            ol_amount: Option<i32>,
        ) -> Self {
            Self {
                ol_o_id: AttrOlOId::from(ol_o_id),

                ol_d_id: AttrOlDId::from(ol_d_id),

                ol_w_id: AttrOlWId::from(ol_w_id),

                ol_number: AttrOlNumber::from(ol_number),

                ol_i_id: AttrOlIId::from(ol_i_id),

                ol_supply_w_id: AttrOlSupplyWId::from(ol_supply_w_id),

                ol_delivery_d: AttrOlDeliveryD::from(ol_delivery_d),

                ol_quantity: AttrOlQuantity::from(ol_quantity),

                ol_amount: AttrOlAmount::from(ol_amount),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_ol_o_id(&mut self, ol_o_id: i32) {
            self.ol_o_id.update(ol_o_id)
        }

        pub fn get_ol_o_id(&self) -> &Option<i32> {
            self.ol_o_id.get()
        }

        pub fn set_ol_d_id(&mut self, ol_d_id: i32) {
            self.ol_d_id.update(ol_d_id)
        }

        pub fn get_ol_d_id(&self) -> &Option<i32> {
            self.ol_d_id.get()
        }

        pub fn set_ol_w_id(&mut self, ol_w_id: i32) {
            self.ol_w_id.update(ol_w_id)
        }

        pub fn get_ol_w_id(&self) -> &Option<i32> {
            self.ol_w_id.get()
        }

        pub fn set_ol_number(&mut self, ol_number: i32) {
            self.ol_number.update(ol_number)
        }

        pub fn get_ol_number(&self) -> &Option<i32> {
            self.ol_number.get()
        }

        pub fn set_ol_i_id(&mut self, ol_i_id: i32) {
            self.ol_i_id.update(ol_i_id)
        }

        pub fn get_ol_i_id(&self) -> &Option<i32> {
            self.ol_i_id.get()
        }

        pub fn set_ol_supply_w_id(&mut self, ol_supply_w_id: i32) {
            self.ol_supply_w_id.update(ol_supply_w_id)
        }

        pub fn get_ol_supply_w_id(&self) -> &Option<i32> {
            self.ol_supply_w_id.get()
        }

        pub fn set_ol_delivery_d(&mut self, ol_delivery_d: String) {
            self.ol_delivery_d.update(ol_delivery_d)
        }

        pub fn get_ol_delivery_d(&self) -> &Option<String> {
            self.ol_delivery_d.get()
        }

        pub fn set_ol_quantity(&mut self, ol_quantity: i32) {
            self.ol_quantity.update(ol_quantity)
        }

        pub fn get_ol_quantity(&self) -> &Option<i32> {
            self.ol_quantity.get()
        }

        pub fn set_ol_amount(&mut self, ol_amount: i32) {
            self.ol_amount.update(ol_amount)
        }

        pub fn get_ol_amount(&self) -> &Option<i32> {
            self.ol_amount.get()
        }
    }

    impl Datum for OrderLine {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_data_type::<OrderLine>)
                .clone()
        }

        fn from_binary(binary: &[u8]) -> RS<Self> {
            entity_utils::entity_from_binary(binary)
        }

        fn from_value(value: &DataValue) -> RS<Self> {
            entity_utils::entity_from_value(value)
        }

        fn from_textual(textual: &str) -> RS<Self> {
            entity_utils::entity_from_textual(textual)
        }
    }

    impl DatumDyn for OrderLine {
        fn type_family(&self) -> RS<TypeFamily> {
            entity_utils::entity_type_family()
        }

        fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
            entity_utils::entity_to_binary(self, data_type)
        }

        fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
            entity_utils::entity_to_textual(self, data_type)
        }

        fn to_value(&self, data_type: &DataType) -> RS<DataValue> {
            entity_utils::entity_to_value(self, data_type)
        }

        fn clone_boxed(&self) -> Box<dyn DatumDyn> {
            entity_utils::entity_clone_boxed(self)
        }
    }

    impl Entity for OrderLine {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrOlOId::datum_desc().clone(),
                    AttrOlDId::datum_desc().clone(),
                    AttrOlWId::datum_desc().clone(),
                    AttrOlNumber::datum_desc().clone(),
                    AttrOlIId::datum_desc().clone(),
                    AttrOlSupplyWId::datum_desc().clone(),
                    AttrOlDeliveryD::datum_desc().clone(),
                    AttrOlQuantity::datum_desc().clone(),
                    AttrOlAmount::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                OL_O_ID => attr_field_access::attr_get_binary::<_>(self.ol_o_id.get()),

                OL_D_ID => attr_field_access::attr_get_binary::<_>(self.ol_d_id.get()),

                OL_W_ID => attr_field_access::attr_get_binary::<_>(self.ol_w_id.get()),

                OL_NUMBER => attr_field_access::attr_get_binary::<_>(self.ol_number.get()),

                OL_I_ID => attr_field_access::attr_get_binary::<_>(self.ol_i_id.get()),

                OL_SUPPLY_W_ID => {
                    attr_field_access::attr_get_binary::<_>(self.ol_supply_w_id.get())
                }

                OL_DELIVERY_D => attr_field_access::attr_get_binary::<_>(self.ol_delivery_d.get()),

                OL_QUANTITY => attr_field_access::attr_get_binary::<_>(self.ol_quantity.get()),

                OL_AMOUNT => attr_field_access::attr_get_binary::<_>(self.ol_amount.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                OL_O_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_o_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_D_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_d_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_w_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_NUMBER => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_number.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_I_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_i_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_SUPPLY_W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_supply_w_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_DELIVERY_D => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_delivery_d.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_QUANTITY => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_quantity.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OL_AMOUNT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.ol_amount.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }

        fn get_field_value(&self, field: &str) -> RS<Option<DataValue>> {
            match field {
                OL_O_ID => attr_field_access::attr_get_value::<_>(self.ol_o_id.get()),

                OL_D_ID => attr_field_access::attr_get_value::<_>(self.ol_d_id.get()),

                OL_W_ID => attr_field_access::attr_get_value::<_>(self.ol_w_id.get()),

                OL_NUMBER => attr_field_access::attr_get_value::<_>(self.ol_number.get()),

                OL_I_ID => attr_field_access::attr_get_value::<_>(self.ol_i_id.get()),

                OL_SUPPLY_W_ID => attr_field_access::attr_get_value::<_>(self.ol_supply_w_id.get()),

                OL_DELIVERY_D => attr_field_access::attr_get_value::<_>(self.ol_delivery_d.get()),

                OL_QUANTITY => attr_field_access::attr_get_value::<_>(self.ol_quantity.get()),

                OL_AMOUNT => attr_field_access::attr_get_value::<_>(self.ol_amount.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DataValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                OL_O_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_o_id.get_mut(), value)?;
                }

                OL_D_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_d_id.get_mut(), value)?;
                }

                OL_W_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_w_id.get_mut(), value)?;
                }

                OL_NUMBER => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_number.get_mut(), value)?;
                }

                OL_I_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_i_id.get_mut(), value)?;
                }

                OL_SUPPLY_W_ID => {
                    attr_field_access::attr_set_value::<_, _>(
                        self.ol_supply_w_id.get_mut(),
                        value,
                    )?;
                }

                OL_DELIVERY_D => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_delivery_d.get_mut(), value)?;
                }

                OL_QUANTITY => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_quantity.get_mut(), value)?;
                }

                OL_AMOUNT => {
                    attr_field_access::attr_set_value::<_, _>(self.ol_amount.get_mut(), value)?;
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
    pub struct AttrOlOId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlOId {
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

    impl AttrValue<i32> for AttrOlOId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_O_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlDId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlDId {
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

    impl AttrValue<i32> for AttrOlDId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_D_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlWId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlWId {
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

    impl AttrValue<i32> for AttrOlWId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_W_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlNumber {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlNumber {
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

    impl AttrValue<i32> for AttrOlNumber {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_NUMBER
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlIId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlIId {
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

    impl AttrValue<i32> for AttrOlIId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_I_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlSupplyWId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlSupplyWId {
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

    impl AttrValue<i32> for AttrOlSupplyWId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_SUPPLY_W_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlDeliveryD {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrOlDeliveryD {
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

    impl AttrValue<String> for AttrOlDeliveryD {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_DELIVERY_D
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlQuantity {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlQuantity {
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

    impl AttrValue<i32> for AttrOlQuantity {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_QUANTITY
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOlAmount {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOlAmount {
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

    impl AttrValue<i32> for AttrOlAmount {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDER_LINE
        }

        fn attr_name() -> &'static str {
            OL_AMOUNT
        }
    }
}
