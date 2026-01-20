use crate::wasm::proc2::object::Wallets;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_contract::{sql_params, sql_stmt};
use mudu_macro::mudu_proc;
use mudu_type::datum::{Datum, DatumDyn};
use sys_interface::api::{mudu_command, mudu_query};

#[mudu_proc]
pub fn proc_sys_call(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    let _affected_rows = mudu_command(xid,
                                     &r#"
CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);"#.to_string(), &vec![])?;

    for i in 1..=2 {
        let _affected_rows = mudu_command(xid,
                                         &r#"
INSERT INTO wallets
(
    user_id,
    balance,
    updated_at
) VALUES (
    ?,
    ?,
    ?
)"#.to_string(), &(i, 100i32, 10000i32))?;
    }

    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id, balance, updated_at FROM wallets;"),
        sql_params!(&()),
    )?;

    let mut result = String::new();
    while let Some(row) = wallet_rs.next_record()? {
        let value = row.to_value(Wallets::dat_type())?;
        let s = value.to_textual(Wallets::dat_type())?;
        result.push_str(&s);
        result.push('\n');
    };
    Ok(((a + b as i32), format!("xid:{}, a={}, b={}, c={}, result {}", xid, a, b, c, result)))
}


#[mudu_proc]
pub fn proc2(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok(((a + b as i32), format!("xid:{}, a={}, b={}, c={}", xid, a, b, c)))
}

#[allow(unused)]
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

    const TABLE_WALLETS: &str = "wallets";
    const COLUMN_USER_ID: &str = "user_id";
    const COLUMN_BALANCE: &str = "balance";
    const COLUMN_UPDATED_AT: &str = "updated_at";
    #[allow(unused)]
    #[derive(Debug, Clone)]
    pub struct Wallets {
        user_id: Option<i32>,
        balance: Option<i32>,
        updated_at: Option<i32>,
    }

    impl TupleDatumMarker for Wallets {}

    impl SQLParamMarker for Wallets {}
    #[allow(unused)]
    impl Wallets {
        pub fn new(user_id: Option<i32>, balance: Option<i32>, updated_at: Option<i32>) -> Self {
            let s = Self {
                user_id,
                balance,
                updated_at,
            };
            s
        }

        pub fn set_user_id(&mut self, user_id: i32) {
            self.user_id = Some(user_id);
        }

        pub fn get_user_id(&self) -> &Option<i32> {
            &self.user_id
        }

        pub fn set_balance(&mut self, balance: i32) {
            self.balance = Some(balance);
        }

        pub fn get_balance(&self) -> &Option<i32> {
            &self.balance
        }

        pub fn set_updated_at(&mut self, updated_at: i32) {
            self.updated_at = Some(updated_at);
        }

        pub fn get_updated_at(&self) -> &Option<i32> {
            &self.updated_at
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
            let s = Self {
                user_id: None,
                balance: None,
                updated_at: None,
            };
            s
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
            TABLE_WALLETS
        }

        fn get_field_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_USER_ID => attr_field_access::attr_get_binary::<_>(&self.user_id),
                COLUMN_BALANCE => attr_field_access::attr_get_binary::<_>(&self.balance),
                COLUMN_UPDATED_AT => attr_field_access::attr_get_binary::<_>(&self.updated_at),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_USER_ID => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.user_id, binary.as_ref())?;
                }
                COLUMN_BALANCE => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.balance, binary.as_ref())?;
                }
                COLUMN_UPDATED_AT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        &mut self.updated_at,
                        binary.as_ref(),
                    )?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
        fn get_field_value(&self, column: &str) -> RS<Option<DatValue>> {
            match column {
                COLUMN_USER_ID => attr_field_access::attr_get_value::<_>(&self.user_id),
                COLUMN_BALANCE => attr_field_access::attr_get_value::<_>(&self.balance),
                COLUMN_UPDATED_AT => attr_field_access::attr_get_value::<_>(&self.updated_at),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, column: &str, value: B) -> RS<()> {
            match column {
                COLUMN_USER_ID => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.user_id, value)?;
                }
                COLUMN_BALANCE => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.balance, value)?;
                }
                COLUMN_UPDATED_AT => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.updated_at, value)?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    pub struct AttrUserId {}

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
            TABLE_WALLETS
        }

        fn attr_name() -> &'static str {
            COLUMN_USER_ID
        }
    }

    pub struct AttrBalance {}

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
            TABLE_WALLETS
        }

        fn attr_name() -> &'static str {
            COLUMN_BALANCE
        }
    }

    pub struct AttrUpdatedAt {}

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
            TABLE_WALLETS
        }

        fn attr_name() -> &'static str {
            COLUMN_UPDATED_AT
        }
    }
} // end mod object
