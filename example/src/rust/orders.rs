pub mod object {
	use mudu::common::result::RS;
	use mudu::data_type::dt_impl::dat_typed::DatTyped;
	use mudu::database::attr_datum::AttrDatum;
	use mudu::database::attribute::AttrValue;
	use mudu::database::record::Record;
	use mudu::database::row_desc::RowDesc;
	use mudu::database::tuple_row::TupleRow;
	use mudu::tuple::datum::Datum;


	const TABLE_ORDERS: &str = "orders";
    const COLUMN_ORDER_ID: &str = "order_id";
    const COLUMN_USER_ID: &str = "user_id";
    const COLUMN_MERCH_ID: &str = "merch_id";
    const COLUMN_AMOUNT: &str = "amount";
    const COLUMN_CREATED_AT: &str = "created_at";


    pub struct Orders {
        order_id: Option<AttrOrderId>,
        user_id: Option<AttrUserId>,
        merch_id: Option<AttrMerchId>,
        amount: Option<AttrAmount>,
        created_at: Option<AttrCreatedAt>,
    }

    impl Orders {
        pub fn new(
            order_id: AttrOrderId,
            user_id: AttrUserId,
            merch_id: AttrMerchId,
            amount: AttrAmount,
            created_at: AttrCreatedAt,
        ) -> Self {
            let s = Self {
                order_id: Some(order_id),
                user_id: Some(user_id),
                merch_id: Some(merch_id),
                amount: Some(amount),
                created_at: Some(created_at),
            };
            s
        }

        pub fn new_empty() -> Self {
            let s = Self {
                order_id: None,
                user_id: None,
                merch_id: None,
                amount: None,
                created_at: None,
            };
            s
        }


        fn get_datum<R, A: AttrValue<R>>(
            attribute: &Option<A>
        ) -> RS<Option<Datum>> {
            let opt_datum = match attribute {
                Some(value) => {
                    Some(value.get_datum()?)
                }
                None => {
                    None
                }
            };
            Ok(opt_datum)
        }


        fn set_datum<R, A: AttrValue<R>, D: AsRef<Datum>>(
			attribute: &mut Option<A>,
			opt_datum: Option<D>,
		) -> RS<()> {
            match attribute {
                Some(value) => {
                    match opt_datum {
                        Some(datum) => {
                            value.set_datum(datum)?;
                        }
                        None => {
                            value.set_datum(Datum::Null)?;
                        }
                    }
                }
                None => {
                    match opt_datum {
                        Some(datum) => {
                            *attribute = Some(A::from_datum(datum.as_ref())?);
                        }
                        None => {
                            *attribute = None;
                        }
                    }
                }
            }
            Ok(())
        }

        pub fn set_order_id(
            &mut self,
            order_id: AttrOrderId,
        ) {
            self.order_id = Some(order_id);
        }

        pub fn get_order_id(
            &self,
        ) -> &Option<AttrOrderId> {
            &self.order_id
        }

        pub fn set_user_id(
            &mut self,
            user_id: AttrUserId,
        ) {
            self.user_id = Some(user_id);
        }

        pub fn get_user_id(
            &self,
        ) -> &Option<AttrUserId> {
            &self.user_id
        }

        pub fn set_merch_id(
            &mut self,
            merch_id: AttrMerchId,
        ) {
            self.merch_id = Some(merch_id);
        }

        pub fn get_merch_id(
            &self,
        ) -> &Option<AttrMerchId> {
            &self.merch_id
        }

        pub fn set_amount(
            &mut self,
            amount: AttrAmount,
        ) {
            self.amount = Some(amount);
        }

        pub fn get_amount(
            &self,
        ) -> &Option<AttrAmount> {
            &self.amount
        }

        pub fn set_created_at(
            &mut self,
            created_at: AttrCreatedAt,
        ) {
            self.created_at = Some(created_at);
        }

        pub fn get_created_at(
            &self,
        ) -> &Option<AttrCreatedAt> {
            &self.created_at
        }
    }

    impl Record for Orders {
        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn from_tuple<T: AsRef<TupleRow>, D: AsRef<RowDesc>>(row: T, desc: D) -> RS<Self> {
            let mut s = Self::new_empty();
            if row.as_ref().items().len() != desc.as_ref().desc().len() {
                panic!("Orders::from_tuple wrong length");
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
                COLUMN_ORDER_ID => {
                    Self::get_datum(&self.order_id)
                }
                COLUMN_USER_ID => {
                    Self::get_datum(&self.user_id)
                }
                COLUMN_MERCH_ID => {
                    Self::get_datum(&self.merch_id)
                }
                COLUMN_AMOUNT => {
                    Self::get_datum(&self.amount)
                }
                COLUMN_CREATED_AT => {
                    Self::get_datum(&self.created_at)
                }
                _ => { panic!("unknown name"); }
            }
        }

        fn set<D: AsRef<Datum>>(&mut self, column: &str, opt_datum: Option<D>) -> RS<()> {
            match column {
                COLUMN_ORDER_ID => {
                    Self::set_datum(&mut self.order_id, opt_datum)?;
                }
                COLUMN_USER_ID => {
                    Self::set_datum(&mut self.user_id, opt_datum)?;
                }
                COLUMN_MERCH_ID => {
                    Self::set_datum(&mut self.merch_id, opt_datum)?;
                }
                COLUMN_AMOUNT => {
                    Self::set_datum(&mut self.amount, opt_datum)?;
                }
                COLUMN_CREATED_AT => {
                    Self::set_datum(&mut self.created_at, opt_datum)?;
                }
                _ => { panic!("unknown name"); }
            }
            Ok(())
        }
    }


    pub struct AttrOrderId {
        value: i32,
    }

    impl AttrOrderId {
        pub fn new(value: i32) -> Self {
            Self { value }
        }
    }

    impl AttrDatum for AttrOrderId {
        fn get_datum(&self) -> RS<Datum> {
            Ok(Datum::Typed(DatTyped::I32(self.value.clone())))
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    panic!("cannot set non-null attribute NULL")
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I32(n) => {
                            self.value = n.clone();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrOrderId {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => {
                    panic!("cannot set non-null attribute NULL")
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I32(value) => {
                            Ok(Self { value: value.clone() })
                        }
                        _ => { unimplemented!() }
                    }
                }
                _ => { unimplemented!() }
            }
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_ORDER_ID
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

    pub struct AttrUserId {
        opt_value: Option<i32>,
    }

    impl AttrUserId {
        pub fn new(opt_value: Option<i32>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrUserId {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::I32(value.clone())))
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I32(n) => {
                            self.opt_value = Some(n.clone());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrUserId {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => {
                    Ok(Self { opt_value: None })
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I32(value) => {
                            Ok(Self { opt_value: Some(value.clone()) })
                        }
                        _ => { unimplemented!() }
                    }
                }
                _ => { unimplemented!() }
            }
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_USER_ID
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
                panic!("attribute user_id is null");
            }
        }

        fn set_value(&mut self, value: i32) {
            self.opt_value = Some(value);
        }
    }

    pub struct AttrMerchId {
        opt_value: Option<i32>,
    }

    impl AttrMerchId {
        pub fn new(opt_value: Option<i32>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrMerchId {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::I32(value.clone())))
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I32(n) => {
                            self.opt_value = Some(n.clone());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrMerchId {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => {
                    Ok(Self { opt_value: None })
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I32(value) => {
                            Ok(Self { opt_value: Some(value.clone()) })
                        }
                        _ => { unimplemented!() }
                    }
                }
                _ => { unimplemented!() }
            }
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_MERCH_ID
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
                panic!("attribute merch_id is null");
            }
        }

        fn set_value(&mut self, value: i32) {
            self.opt_value = Some(value);
        }
    }

    pub struct AttrAmount {
        opt_value: Option<f64>,
    }

    impl AttrAmount {
        pub fn new(opt_value: Option<f64>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrAmount {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::F64(value.clone())))
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::F64(n) => {
                            self.opt_value = Some(n.clone());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    impl AttrValue<f64> for AttrAmount {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => {
                    Ok(Self { opt_value: None })
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::F64(value) => {
                            Ok(Self { opt_value: Some(value.clone()) })
                        }
                        _ => { unimplemented!() }
                    }
                }
                _ => { unimplemented!() }
            }
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_AMOUNT
        }

        fn is_null(&self) -> bool {
            self.opt_value.is_none()
        }

        fn get_opt_value(&self) -> Option<f64> {
            self.opt_value.clone()
        }

        fn set_opt_value(&mut self, opt_value: Option<f64>) {
            self.opt_value = opt_value;
        }

        fn get_value(&self) -> f64 {
            if let Some(value) = &self.opt_value {
                value.clone()
            } else {
                panic!("attribute amount is null");
            }
        }

        fn set_value(&mut self, value: f64) {
            self.opt_value = Some(value);
        }
    }

    pub struct AttrCreatedAt {
        opt_value: Option<i64>,
    }

    impl AttrCreatedAt {
        pub fn new(opt_value: Option<i64>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrCreatedAt {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::I64(value.clone())))
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I64(n) => {
                            self.opt_value = Some(n.clone());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            Ok(())
        }
    }

    impl AttrValue<i64> for AttrCreatedAt {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => {
                    Ok(Self { opt_value: None })
                }
                Datum::Typed(typed) => {
                    match typed {
                        DatTyped::I64(value) => {
                            Ok(Self { opt_value: Some(value.clone()) })
                        }
                        _ => { unimplemented!() }
                    }
                }
                _ => { unimplemented!() }
            }
        }

        fn table_name() -> &'static str {
            TABLE_ORDERS
        }

        fn column_name() -> &'static str {
            COLUMN_CREATED_AT
        }

        fn is_null(&self) -> bool {
            self.opt_value.is_none()
        }

        fn get_opt_value(&self) -> Option<i64> {
            self.opt_value.clone()
        }

        fn set_opt_value(&mut self, opt_value: Option<i64>) {
            self.opt_value = opt_value;
        }

        fn get_value(&self) -> i64 {
            if let Some(value) = &self.opt_value {
                value.clone()
            } else {
                panic!("attribute created_at is null");
            }
        }

        fn set_value(&mut self, value: i64) {
            self.opt_value = Some(value);
        }
    }
} // end mod object
