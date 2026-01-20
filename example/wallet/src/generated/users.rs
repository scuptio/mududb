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
    const USERS: &str = "users";

    const USER_ID: &str = "user_id";

    const NAME: &str = "name";

    const PHONE: &str = "phone";

    const EMAIL: &str = "email";

    const PASSWORD: &str = "password";

    const CREATED_AT: &str = "created_at";

    const UPDATED_AT: &str = "updated_at";


    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Users {
        user_id: AttrUserId,

        name: AttrName,

        phone: AttrPhone,

        email: AttrEmail,

        password: AttrPassword,

        created_at: AttrCreatedAt,

        updated_at: AttrUpdatedAt,

    }

    impl TupleDatumMarker for Users {}

    impl SQLParamMarker for Users {}

    impl Users {
        pub fn new(
            user_id: Option<i32>,
            name: Option<String>,
            phone: Option<String>,
            email: Option<String>,
            password: Option<String>,
            created_at: Option<i32>,
            updated_at: Option<i32>,
        ) -> Self {
            let s = Self {
                user_id: AttrUserId::from(user_id),

                name: AttrName::from(name),

                phone: AttrPhone::from(phone),

                email: AttrEmail::from(email),

                password: AttrPassword::from(password),

                created_at: AttrCreatedAt::from(created_at),

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

        pub fn set_name(
            &mut self,
            name: String,
        ) {
            self.name.update(name)
        }

        pub fn get_name(
            &self,
        ) -> &Option<String> {
            self.name.get()
        }

        pub fn set_phone(
            &mut self,
            phone: String,
        ) {
            self.phone.update(phone)
        }

        pub fn get_phone(
            &self,
        ) -> &Option<String> {
            self.phone.get()
        }

        pub fn set_email(
            &mut self,
            email: String,
        ) {
            self.email.update(email)
        }

        pub fn get_email(
            &self,
        ) -> &Option<String> {
            self.email.get()
        }

        pub fn set_password(
            &mut self,
            password: String,
        ) {
            self.password.update(password)
        }

        pub fn get_password(
            &self,
        ) -> &Option<String> {
            self.password.get()
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

    impl Datum for Users {
        fn dat_type() -> &'static DatType {
            lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Users>();
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

    impl DatumDyn for Users {
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

    impl Entity for Users {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrUserId::datum_desc().clone(),
                
                AttrName::datum_desc().clone(),
                
                AttrPhone::datum_desc().clone(),
                
                AttrEmail::datum_desc().clone(),
                
                AttrPassword::datum_desc().clone(),
                
                AttrCreatedAt::datum_desc().clone(),
                
                AttrUpdatedAt::datum_desc().clone(),
                
            ]);
        }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            USERS
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                USER_ID => {
                    attr_field_access::attr_get_binary::<_>(self.user_id.get())
                }

                NAME => {
                    attr_field_access::attr_get_binary::<_>(self.name.get())
            }

                PHONE => {
                    attr_field_access::attr_get_binary::<_>(self.phone.get())
                }

                EMAIL => {
                    attr_field_access::attr_get_binary::<_>(self.email.get())
                }

                PASSWORD => {
                    attr_field_access::attr_get_binary::<_>(self.password.get())
                }

                CREATED_AT => {
                    attr_field_access::attr_get_binary::<_>(self.created_at.get())
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

                NAME => {
                    attr_field_access::attr_set_binary::<_, _>(self.name.get_mut(), binary.as_ref())?;
                }

                PHONE => {
                    attr_field_access::attr_set_binary::<_, _>(self.phone.get_mut(), binary.as_ref())?;
                }

                EMAIL => {
                    attr_field_access::attr_set_binary::<_, _>(self.email.get_mut(), binary.as_ref())?;
                }

                PASSWORD => {
                    attr_field_access::attr_set_binary::<_, _>(self.password.get_mut(), binary.as_ref())?;
                }

                CREATED_AT => {
                    attr_field_access::attr_set_binary::<_, _>(self.created_at.get_mut(), binary.as_ref())?;
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

                NAME => {
                    attr_field_access::attr_get_value::<_>(self.name.get())
                }

                PHONE => {
                    attr_field_access::attr_get_value::<_>(self.phone.get())
                }

                EMAIL => {
                    attr_field_access::attr_get_value::<_>(self.email.get())
                }

                PASSWORD => {
                    attr_field_access::attr_get_value::<_>(self.password.get())
                }

                CREATED_AT => {
                    attr_field_access::attr_get_value::<_>(self.created_at.get())
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

                NAME => {
                    attr_field_access::attr_set_value::<_, _>(self.name.get_mut(), value)?;
                }

                PHONE => {
                    attr_field_access::attr_set_value::<_, _>(self.phone.get_mut(), value)?;
                }

                EMAIL => {
                    attr_field_access::attr_set_value::<_, _>(self.email.get_mut(), value)?;
                }

                PASSWORD => {
                    attr_field_access::attr_set_value::<_, _>(self.password.get_mut(), value)?;
                }

                CREATED_AT => {
                    attr_field_access::attr_set_value::<_, _>(self.created_at.get_mut(), value)?;
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
            USERS
        }

        fn attr_name() -> &'static str {
            USER_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrName {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrName {
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

    impl AttrValue<String> for AttrName {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            USERS
        }

        fn attr_name() -> &'static str {
            NAME
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrPhone {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrPhone {
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

    impl AttrValue<String> for AttrPhone {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            USERS
        }

        fn attr_name() -> &'static str {
            PHONE
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrEmail {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrEmail {
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

    impl AttrValue<String> for AttrEmail {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            USERS
        }

        fn attr_name() -> &'static str {
            EMAIL
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrPassword {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrPassword {
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

    impl AttrValue<String> for AttrPassword {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            USERS
        }

        fn attr_name() -> &'static str {
            PASSWORD
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
            USERS
        }

        fn attr_name() -> &'static str {
            CREATED_AT
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
            USERS
        }

        fn attr_name() -> &'static str {
            UPDATED_AT
        }
    }
}