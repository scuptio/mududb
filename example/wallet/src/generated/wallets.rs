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
    const WALLETS: &str = "wallets";

    const USER_ID: &str = "user_id";

    const BALANCE: &str = "balance";

    const UPDATED_AT: &str = "updated_at";


    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Wallets {
        user_id: AttrUserId,

        balance: AttrBalance,

        updated_at: AttrUpdatedAt,

    }

    impl TupleDatumMarker for Wallets {}

    impl SQLParamMarker for Wallets {}

    impl Wallets {
        pub fn new(
        user_id: Option<i32>,
        balance: Option<i32>,
        updated_at: Option<i32>,
        ) -> Self {
            let s = Self {
                user_id: AttrUserId::from(user_id),

                balance: AttrBalance::from(balance),

                updated_at: AttrUpdatedAt::from(updated_at),

            };
            s
        }

        pub fn new_empty() -> Self {
            Self::default()
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

        pub fn set_balance(
            &mut self,
            balance: i32,
        ) {
            self.balance.update(balance)
        }

        pub fn get_balance(
            &self,
        ) -> &Option<i32> {
            self.balance.get()
        }

        pub fn set_updated_at(
            &mut self,
            updated_at: i32,
        ) {
            self.updated_at.update(updated_at)
        }

        pub fn get_updated_at(
            &self,
        ) -> &Option<i32> {
            self.updated_at.get()
        }
    }

    impl Datum for Wallets {
        fn dat_type() -> &'static DatType {
            lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Wallets>();
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

    impl DatumDyn for Wallets {
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

    impl Entity for Wallets {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrUserId::datum_desc().clone(),
                
                AttrBalance::datum_desc().clone(),
                
                AttrUpdatedAt::datum_desc().clone(),
                
            ]);
        }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            WALLETS
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                USER_ID => {
                    attr_field_access::attr_get_binary::<_>(self.user_id.get())
                }

                BALANCE => {
                    attr_field_access::attr_get_binary::<_>(self.balance.get())
            }

                UPDATED_AT => {
                    attr_field_access::attr_get_binary::<_>(self.updated_at.get())
                }

                _ => { panic!("unknown name"); }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                USER_ID => {
                    attr_field_access::attr_set_binary::<_, _>(self.user_id.get_mut(), binary.as_ref())?;
                }

                BALANCE => {
                    attr_field_access::attr_set_binary::<_, _>(self.balance.get_mut(), binary.as_ref())?;
                }

                UPDATED_AT => {
                    attr_field_access::attr_set_binary::<_, _>(self.updated_at.get_mut(), binary.as_ref())?;
                }

                _ => { panic!("unknown name"); }
            }
            Ok(())
        }

        fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
            match field {
                USER_ID => {
                    attr_field_access::attr_get_value::<_>(self.user_id.get())
                }

                BALANCE => {
                    attr_field_access::attr_get_value::<_>(self.balance.get())
            }

                UPDATED_AT => {
                    attr_field_access::attr_get_value::<_>(self.updated_at.get())
                }

                _ => { panic!("unknown name"); }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                USER_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.user_id.get_mut(), value)?;
                }

                BALANCE => {
                    attr_field_access::attr_set_value::<_, _>(self.balance.get_mut(), value)?;
                }

                UPDATED_AT => {
                    attr_field_access::attr_set_value::<_, _>(self.updated_at.get_mut(), value)?;
                }

                _ => { panic!("unknown name"); }
            }
            Ok(())
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
            WALLETS
        }

        fn attr_name() -> &'static str {
            USER_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrBalance {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrBalance {
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

    impl AttrValue<i32> for AttrBalance {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            WALLETS
        }

        fn attr_name() -> &'static str {
            BALANCE
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrUpdatedAt {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrUpdatedAt {
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

    impl AttrValue<i32> for AttrUpdatedAt {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            WALLETS
        }

        fn attr_name() -> &'static str {
            UPDATED_AT
        }
    }
}