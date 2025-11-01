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

    const TABLE_USERS: &str = "users";
    const COLUMN_USER_ID: &str = "user_id";
    const COLUMN_NAME: &str = "name";
    const COLUMN_PHONE: &str = "phone";
    const COLUMN_EMAIL: &str = "email";
    const COLUMN_PASSWORD: &str = "password";
    const COLUMN_CREATED_AT: &str = "created_at";
    const COLUMN_UPDATED_AT: &str = "updated_at";

    pub struct Users {
        user_id: Option<AttrUserId>,
        name: Option<AttrName>,
        phone: Option<AttrPhone>,
        email: Option<AttrEmail>,
        password: Option<AttrPassword>,
        created_at: Option<AttrCreatedAt>,
        updated_at: Option<AttrUpdatedAt>,
    }

    impl Users {
        pub fn new(
            user_id: AttrUserId,
            name: AttrName,
            phone: AttrPhone,
            email: AttrEmail,
            password: AttrPassword,
            created_at: AttrCreatedAt,
            updated_at: AttrUpdatedAt,
        ) -> Self {
            let s = Self {
                user_id: Some(user_id),
                name: Some(name),
                phone: Some(phone),
                email: Some(email),
                password: Some(password),
                created_at: Some(created_at),
                updated_at: Some(updated_at),
            };
            s
        }

        pub fn set_user_id(&mut self, user_id: AttrUserId) {
            self.user_id = Some(user_id);
        }

        pub fn get_user_id(&self) -> &Option<AttrUserId> {
            &self.user_id
        }

        pub fn set_name(&mut self, name: AttrName) {
            self.name = Some(name);
        }

        pub fn get_name(&self) -> &Option<AttrName> {
            &self.name
        }

        pub fn set_phone(&mut self, phone: AttrPhone) {
            self.phone = Some(phone);
        }

        pub fn get_phone(&self) -> &Option<AttrPhone> {
            &self.phone
        }

        pub fn set_email(&mut self, email: AttrEmail) {
            self.email = Some(email);
        }

        pub fn get_email(&self) -> &Option<AttrEmail> {
            &self.email
        }

        pub fn set_password(&mut self, password: AttrPassword) {
            self.password = Some(password);
        }

        pub fn get_password(&self) -> &Option<AttrPassword> {
            &self.password
        }

        pub fn set_created_at(&mut self, created_at: AttrCreatedAt) {
            self.created_at = Some(created_at);
        }

        pub fn get_created_at(&self) -> &Option<AttrCreatedAt> {
            &self.created_at
        }

        pub fn set_updated_at(&mut self, updated_at: AttrUpdatedAt) {
            self.updated_at = Some(updated_at);
        }

        pub fn get_updated_at(&self) -> &Option<AttrUpdatedAt> {
            &self.updated_at
        }
    }

    impl Record for Users {
        fn new_empty() -> Self {
            let s = Self {
                user_id: None,
                name: None,
                phone: None,
                email: None,
                password: None,
                created_at: None,
                updated_at: None,
            };
            s
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

        fn table_name() -> &'static str {
            TABLE_USERS
        }

        fn from_tuple<T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(row: T, desc: D) -> RS<Self> {
            record_from_tuple::<Self, T, D>(row, desc)
        }

        fn to_tuple<D: AsRef<TupleFieldDesc>>(&self, desc: D) -> RS<TupleField> {
            record_to_tuple(self, desc)
        }

        fn get_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_USER_ID => attr_get_binary(&self.user_id),
                COLUMN_NAME => attr_get_binary(&self.name),
                COLUMN_PHONE => attr_get_binary(&self.phone),
                COLUMN_EMAIL => attr_get_binary(&self.email),
                COLUMN_PASSWORD => attr_get_binary(&self.password),
                COLUMN_CREATED_AT => attr_get_binary(&self.created_at),
                COLUMN_UPDATED_AT => attr_get_binary(&self.updated_at),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_USER_ID => {
                    attr_set_binary(&mut self.user_id, binary.as_ref())?;
                }
                COLUMN_NAME => {
                    attr_set_binary(&mut self.name, binary.as_ref())?;
                }
                COLUMN_PHONE => {
                    attr_set_binary(&mut self.phone, binary.as_ref())?;
                }
                COLUMN_EMAIL => {
                    attr_set_binary(&mut self.email, binary.as_ref())?;
                }
                COLUMN_PASSWORD => {
                    attr_set_binary(&mut self.password, binary.as_ref())?;
                }
                COLUMN_CREATED_AT => {
                    attr_set_binary(&mut self.created_at, binary.as_ref())?;
                }
                COLUMN_UPDATED_AT => {
                    attr_set_binary(&mut self.updated_at, binary.as_ref())?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
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
            TABLE_USERS
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

    pub struct AttrName {
        value: String,
    }

    impl AttrName {}

    impl AttrBinary for AttrName {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrName {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_USERS
        }

        fn column_name() -> &'static str {
            COLUMN_NAME
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrPhone {
        value: String,
    }

    impl AttrPhone {}

    impl AttrBinary for AttrPhone {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrPhone {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_USERS
        }

        fn column_name() -> &'static str {
            COLUMN_PHONE
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrEmail {
        value: String,
    }

    impl AttrEmail {}

    impl AttrBinary for AttrEmail {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrEmail {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_USERS
        }

        fn column_name() -> &'static str {
            COLUMN_EMAIL
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrPassword {
        value: String,
    }

    impl AttrPassword {}

    impl AttrBinary for AttrPassword {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrPassword {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_USERS
        }

        fn column_name() -> &'static str {
            COLUMN_PASSWORD
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
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
            TABLE_USERS
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

    pub struct AttrUpdatedAt {
        value: i32,
    }

    impl AttrUpdatedAt {}

    impl AttrBinary for AttrUpdatedAt {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrUpdatedAt {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_USERS
        }

        fn column_name() -> &'static str {
            COLUMN_UPDATED_AT
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }
} // end mod object
