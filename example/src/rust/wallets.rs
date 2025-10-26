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

    const TABLE_WALLETS: &str = "wallets";
    const COLUMN_USER_ID: &str = "user_id";
    const COLUMN_BALANCE: &str = "balance";

    pub struct Wallets {
        user_id: Option<AttrUserId>,
        balance: Option<AttrBalance>,
    }

    impl Wallets {
        pub fn new(user_id: AttrUserId, balance: AttrBalance) -> Self {
            let s = Self {
                user_id: Some(user_id),
                balance: Some(balance),
            };
            s
        }

        pub fn set_user_id(&mut self, user_id: AttrUserId) {
            self.user_id = Some(user_id);
        }

        pub fn get_user_id(&self) -> &Option<AttrUserId> {
            &self.user_id
        }

        pub fn set_balance(&mut self, balance: AttrBalance) {
            self.balance = Some(balance);
        }

        pub fn get_balance(&self) -> &Option<AttrBalance> {
            &self.balance
        }
    }

    impl Record for Wallets {
        fn new_empty() -> Self {
            let s = Self {
                user_id: None,
                balance: None,
            };
            s
        }
        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrUserId::datum_desc().clone(),
                    AttrBalance::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn table_name() -> &'static str {
            TABLE_WALLETS
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
                COLUMN_BALANCE => attr_get_binary(&self.balance),
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
                COLUMN_BALANCE => {
                    attr_set_binary(&mut self.balance, binary.as_ref())?;
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
            TABLE_WALLETS
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

    pub struct AttrBalance {
        value: i32,
    }

    impl AttrBalance {}

    impl AttrBinary for AttrBalance {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrBalance {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WALLETS
        }

        fn column_name() -> &'static str {
            COLUMN_BALANCE
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }
} // end mod object
