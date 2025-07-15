pub mod object {
    use mudu::common::result::RS;
    use mudu::data_type::dt_impl::dat_typed::DatTyped;
    use mudu::database::attr_datum::AttrDatum;
    use mudu::database::attribute::Attribute;
    use mudu::database::record::Record;
    use mudu::database::row_desc::RowDesc;
    use mudu::database::tuple_row::TupleRow;
    use mudu::tuple::datum::Datum;

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

        pub fn new_empty() -> Self {
            let s = Self {
                user_id: None,
                balance: None,
            };
            s
        }

        fn get_datum<R, A: Attribute<R>>(attribute: &Option<A>) -> RS<Option<Datum>> {
            let opt_datum = match attribute {
                Some(value) => Some(value.get_datum()?),
                None => None,
            };
            Ok(opt_datum)
        }

        fn set_datum<R, A: Attribute<R>, D: AsRef<Datum>>(
            attribute: &mut Option<A>,
            opt_datum: Option<D>,
        ) -> RS<()> {
            match attribute {
                Some(value) => match opt_datum {
                    Some(datum) => {
                        value.set_datum(datum)?;
                    }
                    None => {
                        value.set_datum(Datum::Null)?;
                    }
                },
                None => match opt_datum {
                    Some(datum) => {
                        *attribute = Some(A::from_datum(datum.as_ref())?);
                    }
                    None => {
                        *attribute = None;
                    }
                },
            }
            Ok(())
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
        fn table_name() -> &'static str {
            TABLE_WALLETS
        }

        fn from_tuple<T: AsRef<TupleRow>, D: AsRef<RowDesc>>(row: T, desc: D) -> RS<Self> {
            let mut s = Self::new_empty();
            if row.as_ref().items().len() != desc.as_ref().desc().len() {
                panic!("Wallets::from_tuple wrong length");
            }
            for (i, dat) in row.as_ref().items().iter().enumerate() {
                let dd = &desc.as_ref().desc()[i];
                s.set(dd.name(), Some(dat.as_ref()))?;
            }
            Ok(s)
        }

        fn to_tuple<D: AsRef<RowDesc>>(&self, desc: D) -> RS<TupleRow> {
            let mut tuple = vec![];
            for d in desc.as_ref().desc() {
                let opt_datum = self.get(d.name())?;
                if let Some(datum) = opt_datum {
                    tuple.push(datum);
                }
            }
            Ok(TupleRow::new(tuple))
        }

        fn get(&self, column: &str) -> RS<Option<Datum>> {
            match column {
                COLUMN_USER_ID => Self::get_datum(&self.user_id),
                COLUMN_BALANCE => Self::get_datum(&self.balance),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set<D: AsRef<Datum>>(&mut self, column: &str, opt_datum: Option<D>) -> RS<()> {
            match column {
                COLUMN_USER_ID => {
                    Self::set_datum(&mut self.user_id, opt_datum)?;
                }
                COLUMN_BALANCE => {
                    Self::set_datum(&mut self.balance, opt_datum)?;
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

    impl AttrUserId {
        pub fn new(value: i32) -> Self {
            Self { value }
        }
    }

    impl AttrDatum for AttrUserId {
        fn get_datum(&self) -> RS<Datum> {
            Ok(Datum::Typed(DatTyped::I32(self.value.clone())))
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    panic!("cannot set non-null attribute NULL")
                }
                Datum::Typed(typed) => match typed {
                    DatTyped::I32(n) => {
                        self.value = n.clone();
                    }
                    _ => {}
                },
                _ => {}
            }
            Ok(())
        }
    }

    impl Attribute<i32> for AttrUserId {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => {
                    panic!("cannot set non-null attribute NULL")
                }
                Datum::Typed(typed) => match typed {
                    DatTyped::I32(value) => Ok(Self {
                        value: value.clone(),
                    }),
                    _ => {
                        unimplemented!()
                    }
                },
                _ => {
                    unimplemented!()
                }
            }
        }

        fn table_name() -> &'static str {
            TABLE_WALLETS
        }

        fn column_name() -> &'static str {
            COLUMN_USER_ID
        }

        fn is_null(&self) -> bool {
            false
        }

        fn get_opt_value(&self) -> Option<i32> {
            Some(self.value.clone())
        }

        fn set_opt_value(&mut self, opt_value: Option<i32>) {
            if let Some(value) = opt_value {
                self.value = value;
            }
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrBalance {
        opt_value: Option<i32>,
    }

    impl AttrBalance {
        pub fn new(opt_value: Option<i32>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrBalance {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::I32(value.clone()))),
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => match typed {
                    DatTyped::I32(n) => {
                        self.opt_value = Some(n.clone());
                    }
                    _ => {}
                },
                _ => {}
            }
            Ok(())
        }
    }

    impl Attribute<i32> for AttrBalance {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => Ok(Self { opt_value: None }),
                Datum::Typed(typed) => match typed {
                    DatTyped::I32(value) => Ok(Self {
                        opt_value: Some(value.clone()),
                    }),
                    _ => {
                        unimplemented!()
                    }
                },
                _ => {
                    unimplemented!()
                }
            }
        }

        fn table_name() -> &'static str {
            TABLE_WALLETS
        }

        fn column_name() -> &'static str {
            COLUMN_BALANCE
        }

        fn is_null(&self) -> bool {
            self.opt_value.is_none()
        }

        fn get_opt_value(&self) -> Option<i32> {
            self.opt_value.clone()
        }

        fn set_opt_value(&mut self, opt_value: Option<i32>) {
            self.opt_value = opt_value;
        }

        fn get_value(&self) -> i32 {
            if let Some(value) = &self.opt_value {
                value.clone()
            } else {
                panic!("attribute balance is null");
            }
        }

        fn set_value(&mut self, value: i32) {
            self.opt_value = Some(value);
        }
    }
} // end mod object
