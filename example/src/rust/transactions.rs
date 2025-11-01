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

    const TABLE_TRANSACTIONS: &str = "transactions";
    const COLUMN_TRANS_ID: &str = "trans_id";
    const COLUMN_TRANS_TYPE: &str = "trans_type";
    const COLUMN_FROM_USER: &str = "from_user";
    const COLUMN_TO_USER: &str = "to_user";
    const COLUMN_AMOUNT: &str = "amount";
    const COLUMN_CREATED_AT: &str = "created_at";

    pub struct Transactions {
        trans_id: Option<AttrTransId>,
        trans_type: Option<AttrTransType>,
        from_user: Option<AttrFromUser>,
        to_user: Option<AttrToUser>,
        amount: Option<AttrAmount>,
        created_at: Option<AttrCreatedAt>,
    }

    impl Transactions {
        pub fn new(
            trans_id: AttrTransId,
            trans_type: AttrTransType,
            from_user: AttrFromUser,
            to_user: AttrToUser,
            amount: AttrAmount,
            created_at: AttrCreatedAt,
        ) -> Self {
            let s = Self {
                trans_id: Some(trans_id),
                trans_type: Some(trans_type),
                from_user: Some(from_user),
                to_user: Some(to_user),
                amount: Some(amount),
                created_at: Some(created_at),
            };
            s
        }

        pub fn set_trans_id(&mut self, trans_id: AttrTransId) {
            self.trans_id = Some(trans_id);
        }

        pub fn get_trans_id(&self) -> &Option<AttrTransId> {
            &self.trans_id
        }

        pub fn set_trans_type(&mut self, trans_type: AttrTransType) {
            self.trans_type = Some(trans_type);
        }

        pub fn get_trans_type(&self) -> &Option<AttrTransType> {
            &self.trans_type
        }

        pub fn set_from_user(&mut self, from_user: AttrFromUser) {
            self.from_user = Some(from_user);
        }

        pub fn get_from_user(&self) -> &Option<AttrFromUser> {
            &self.from_user
        }

        pub fn set_to_user(&mut self, to_user: AttrToUser) {
            self.to_user = Some(to_user);
        }

        pub fn get_to_user(&self) -> &Option<AttrToUser> {
            &self.to_user
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

    impl Record for Transactions {
        fn new_empty() -> Self {
            let s = Self {
                trans_id: None,
                trans_type: None,
                from_user: None,
                to_user: None,
                amount: None,
                created_at: None,
            };
            s
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

        fn table_name() -> &'static str {
            TABLE_TRANSACTIONS
        }

        fn from_tuple<T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(row: T, desc: D) -> RS<Self> {
            record_from_tuple::<Self, T, D>(row, desc)
        }

        fn to_tuple<D: AsRef<TupleFieldDesc>>(&self, desc: D) -> RS<TupleField> {
            record_to_tuple(self, desc)
        }

        fn get_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_TRANS_ID => attr_get_binary(&self.trans_id),
                COLUMN_TRANS_TYPE => attr_get_binary(&self.trans_type),
                COLUMN_FROM_USER => attr_get_binary(&self.from_user),
                COLUMN_TO_USER => attr_get_binary(&self.to_user),
                COLUMN_AMOUNT => attr_get_binary(&self.amount),
                COLUMN_CREATED_AT => attr_get_binary(&self.created_at),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_TRANS_ID => {
                    attr_set_binary(&mut self.trans_id, binary.as_ref())?;
                }
                COLUMN_TRANS_TYPE => {
                    attr_set_binary(&mut self.trans_type, binary.as_ref())?;
                }
                COLUMN_FROM_USER => {
                    attr_set_binary(&mut self.from_user, binary.as_ref())?;
                }
                COLUMN_TO_USER => {
                    attr_set_binary(&mut self.to_user, binary.as_ref())?;
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

    pub struct AttrTransId {
        value: String,
    }

    impl AttrTransId {}

    impl AttrBinary for AttrTransId {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrTransId {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_TRANSACTIONS
        }

        fn column_name() -> &'static str {
            COLUMN_TRANS_ID
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrTransType {
        value: String,
    }

    impl AttrTransType {}

    impl AttrBinary for AttrTransType {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrTransType {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_TRANSACTIONS
        }

        fn column_name() -> &'static str {
            COLUMN_TRANS_TYPE
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrFromUser {
        value: i32,
    }

    impl AttrFromUser {}

    impl AttrBinary for AttrFromUser {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrFromUser {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_TRANSACTIONS
        }

        fn column_name() -> &'static str {
            COLUMN_FROM_USER
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrToUser {
        value: i32,
    }

    impl AttrToUser {}

    impl AttrBinary for AttrToUser {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrToUser {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_TRANSACTIONS
        }

        fn column_name() -> &'static str {
            COLUMN_TO_USER
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
            TABLE_TRANSACTIONS
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
            TABLE_TRANSACTIONS
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
