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
    const TRANSACTIONS: &str = "transactions";

    const TRANS_ID: &str = "trans_id";

    const TRANS_TYPE: &str = "trans_type";

    const FROM_USER: &str = "from_user";

    const TO_USER: &str = "to_user";

    const AMOUNT: &str = "amount";

    const CREATED_AT: &str = "created_at";


    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Transactions {
        trans_id: AttrTransId,

        trans_type: AttrTransType,

        from_user: AttrFromUser,

        to_user: AttrToUser,

        amount: AttrAmount,

        created_at: AttrCreatedAt,

    }

    impl TupleDatumMarker for Transactions {}

    impl SQLParamMarker for Transactions {}

    impl Transactions {
        pub fn new(
            trans_id: Option<String>,
            trans_type: Option<String>,
            from_user: Option<i32>,
            to_user: Option<i32>,
            amount: Option<i32>,
            created_at: Option<i32>,
        ) -> Self {
            let s = Self {
                trans_id: AttrTransId::from(trans_id),

                trans_type: AttrTransType::from(trans_type),

                from_user: AttrFromUser::from(from_user),

                to_user: AttrToUser::from(to_user),

                amount: AttrAmount::from(amount),

                created_at: AttrCreatedAt::from(created_at),

            };
            s
        }

        pub fn new_empty() -> Self {
            Self::default()
        }


        pub fn set_trans_id(
            &mut self,
            trans_id: String,
        ) {
            self.trans_id.update(trans_id)
        }

        pub fn get_trans_id(
            &self,
        ) -> &Option<String> {
            self.trans_id.get()
        }

        pub fn set_trans_type(
            &mut self,
            trans_type: String,
        ) {
            self.trans_type.update(trans_type)
        }

        pub fn get_trans_type(
            &self,
        ) -> &Option<String> {
            self.trans_type.get()
        }

        pub fn set_from_user(
            &mut self,
            from_user: i32,
        ) {
            self.from_user.update(from_user)
        }

        pub fn get_from_user(
            &self,
        ) -> &Option<i32> {
            self.from_user.get()
        }

        pub fn set_to_user(
            &mut self,
            to_user: i32,
        ) {
            self.to_user.update(to_user)
        }

        pub fn get_to_user(
            &self,
        ) -> &Option<i32> {
            self.to_user.get()
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

    impl Datum for Transactions {
        fn dat_type() -> &'static DatType {
            lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Transactions>();
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

    impl DatumDyn for Transactions {
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

    impl Entity for Transactions {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrTransId::datum_desc().clone(),
                
                AttrTransType::datum_desc().clone(),
                
                AttrFromUser::datum_desc().clone(),
                
                AttrToUser::datum_desc().clone(),
                
                AttrAmount::datum_desc().clone(),
                
                AttrCreatedAt::datum_desc().clone(),
                
            ]);
        }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            TRANSACTIONS
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                TRANS_ID => {
                    attr_field_access::attr_get_binary::<_>(self.trans_id.get())
                }

                TRANS_TYPE => {
                    attr_field_access::attr_get_binary::<_>(self.trans_type.get())
            }

                FROM_USER => {
                    attr_field_access::attr_get_binary::<_>(self.from_user.get())
                }

                TO_USER => {
                    attr_field_access::attr_get_binary::<_>(self.to_user.get())
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
                TRANS_ID => {
                    attr_field_access::attr_set_binary::<_, _>(self.trans_id.get_mut(), binary.as_ref())?;
                }

                TRANS_TYPE => {
                    attr_field_access::attr_set_binary::<_, _>(self.trans_type.get_mut(), binary.as_ref())?;
                }

                FROM_USER => {
                    attr_field_access::attr_set_binary::<_, _>(self.from_user.get_mut(), binary.as_ref())?;
                }

                TO_USER => {
                    attr_field_access::attr_set_binary::<_, _>(self.to_user.get_mut(), binary.as_ref())?;
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
                TRANS_ID => {
                    attr_field_access::attr_get_value::<_>(self.trans_id.get())
                }

                TRANS_TYPE => {
                    attr_field_access::attr_get_value::<_>(self.trans_type.get())
            }

                FROM_USER => {
                    attr_field_access::attr_get_value::<_>(self.from_user.get())
                }

                TO_USER => {
                    attr_field_access::attr_get_value::<_>(self.to_user.get())
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
                TRANS_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.trans_id.get_mut(), value)?;
                }

                TRANS_TYPE => {
                    attr_field_access::attr_set_value::<_, _>(self.trans_type.get_mut(), value)?;
                }

                FROM_USER => {
                    attr_field_access::attr_set_value::<_, _>(self.from_user.get_mut(), value)?;
                }

                TO_USER => {
                    attr_field_access::attr_set_value::<_, _>(self.to_user.get_mut(), value)?;
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
    pub struct AttrTransId {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrTransId {
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

    impl AttrValue<String> for AttrTransId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TRANSACTIONS
        }

        fn attr_name() -> &'static str {
            TRANS_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrTransType {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrTransType {
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

    impl AttrValue<String> for AttrTransType {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TRANSACTIONS
        }

        fn attr_name() -> &'static str {
            TRANS_TYPE
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrFromUser {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrFromUser {
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

    impl AttrValue<i32> for AttrFromUser {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TRANSACTIONS
        }

        fn attr_name() -> &'static str {
            FROM_USER
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrToUser {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrToUser {
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

    impl AttrValue<i32> for AttrToUser {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TRANSACTIONS
        }

        fn attr_name() -> &'static str {
            TO_USER
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
            TRANSACTIONS
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
            TRANSACTIONS
        }

        fn attr_name() -> &'static str {
            CREATED_AT
        }
    }
}