pub mod object {
    use mudu::common::result::RS;
    use mudu::data_type::dt_impl::dat_typed::DatTyped;
    use mudu::database::attr_datum::AttrDatum;
    use mudu::database::attribute::Attribute;
    use mudu::database::record::Record;
    use mudu::database::row_desc::RowDesc;
    use mudu::database::tuple_row::TupleRow;
    use mudu::tuple::datum::Datum;

    const TABLE_ITEM: &str = "item";
    const COLUMN_I_ID: &str = "i_id";
    const COLUMN_I_NAME: &str = "i_name";
    const COLUMN_I_PRICE: &str = "i_price";
    const COLUMN_I_DATA: &str = "i_data";
    const COLUMN_I_IM_ID: &str = "i_im_id";

    pub struct Item {
        i_id: Option<AttrIId>,
        i_name: Option<AttrIName>,
        i_price: Option<AttrIPrice>,
        i_data: Option<AttrIData>,
        i_im_id: Option<AttrIImId>,
    }

    impl Item {
        pub fn new(
            i_id: AttrIId,
            i_name: AttrIName,
            i_price: AttrIPrice,
            i_data: AttrIData,
            i_im_id: AttrIImId,
        ) -> Self {
            let s = Self {
                i_id: Some(i_id),
                i_name: Some(i_name),
                i_price: Some(i_price),
                i_data: Some(i_data),
                i_im_id: Some(i_im_id),
            };
            s
        }

        pub fn new_empty() -> Self {
            let s = Self {
                i_id: None,
                i_name: None,
                i_price: None,
                i_data: None,
                i_im_id: None,
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

        pub fn set_i_id(&mut self, i_id: AttrIId) {
            self.i_id = Some(i_id);
        }

        pub fn get_i_id(&self) -> &Option<AttrIId> {
            &self.i_id
        }

        pub fn set_i_name(&mut self, i_name: AttrIName) {
            self.i_name = Some(i_name);
        }

        pub fn get_i_name(&self) -> &Option<AttrIName> {
            &self.i_name
        }

        pub fn set_i_price(&mut self, i_price: AttrIPrice) {
            self.i_price = Some(i_price);
        }

        pub fn get_i_price(&self) -> &Option<AttrIPrice> {
            &self.i_price
        }

        pub fn set_i_data(&mut self, i_data: AttrIData) {
            self.i_data = Some(i_data);
        }

        pub fn get_i_data(&self) -> &Option<AttrIData> {
            &self.i_data
        }

        pub fn set_i_im_id(&mut self, i_im_id: AttrIImId) {
            self.i_im_id = Some(i_im_id);
        }

        pub fn get_i_im_id(&self) -> &Option<AttrIImId> {
            &self.i_im_id
        }
    }

    impl Record for Item {
        fn table_name() -> &'static str {
            TABLE_ITEM
        }

        fn from_tuple<T: AsRef<TupleRow>, D: AsRef<RowDesc>>(row: T, desc: D) -> RS<Self> {
            let mut s = Self::new_empty();
            if row.as_ref().items().len() != desc.as_ref().desc().len() {
                panic!("Item::from_tuple wrong length");
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
                COLUMN_I_ID => Self::get_datum(&self.i_id),
                COLUMN_I_NAME => Self::get_datum(&self.i_name),
                COLUMN_I_PRICE => Self::get_datum(&self.i_price),
                COLUMN_I_DATA => Self::get_datum(&self.i_data),
                COLUMN_I_IM_ID => Self::get_datum(&self.i_im_id),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set<D: AsRef<Datum>>(&mut self, column: &str, opt_datum: Option<D>) -> RS<()> {
            match column {
                COLUMN_I_ID => {
                    Self::set_datum(&mut self.i_id, opt_datum)?;
                }
                COLUMN_I_NAME => {
                    Self::set_datum(&mut self.i_name, opt_datum)?;
                }
                COLUMN_I_PRICE => {
                    Self::set_datum(&mut self.i_price, opt_datum)?;
                }
                COLUMN_I_DATA => {
                    Self::set_datum(&mut self.i_data, opt_datum)?;
                }
                COLUMN_I_IM_ID => {
                    Self::set_datum(&mut self.i_im_id, opt_datum)?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    pub struct AttrIId {
        value: i32,
    }

    impl AttrIId {
        pub fn new(value: i32) -> Self {
            Self { value }
        }
    }

    impl AttrDatum for AttrIId {
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

    impl Attribute<i32> for AttrIId {
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
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_ID
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

    pub struct AttrIName {
        opt_value: Option<String>,
    }

    impl AttrIName {
        pub fn new(opt_value: Option<String>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrIName {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::String(value.clone()))),
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => match typed {
                    DatTyped::String(n) => {
                        self.opt_value = Some(n.clone());
                    }
                    _ => {}
                },
                _ => {}
            }
            Ok(())
        }
    }

    impl Attribute<String> for AttrIName {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => Ok(Self { opt_value: None }),
                Datum::Typed(typed) => match typed {
                    DatTyped::String(value) => Ok(Self {
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
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_NAME
        }

        fn is_null(&self) -> bool {
            self.opt_value.is_none()
        }

        fn get_opt_value(&self) -> Option<String> {
            self.opt_value.clone()
        }

        fn set_opt_value(&mut self, opt_value: Option<String>) {
            self.opt_value = opt_value;
        }

        fn get_value(&self) -> String {
            if let Some(value) = &self.opt_value {
                value.clone()
            } else {
                panic!("attribute i_name is null");
            }
        }

        fn set_value(&mut self, value: String) {
            self.opt_value = Some(value);
        }
    }

    pub struct AttrIPrice {
        opt_value: Option<f64>,
    }

    impl AttrIPrice {
        pub fn new(opt_value: Option<f64>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrIPrice {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::F64(value.clone()))),
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => match typed {
                    DatTyped::F64(n) => {
                        self.opt_value = Some(n.clone());
                    }
                    _ => {}
                },
                _ => {}
            }
            Ok(())
        }
    }

    impl Attribute<f64> for AttrIPrice {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => Ok(Self { opt_value: None }),
                Datum::Typed(typed) => match typed {
                    DatTyped::F64(value) => Ok(Self {
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
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_PRICE
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
                panic!("attribute i_price is null");
            }
        }

        fn set_value(&mut self, value: f64) {
            self.opt_value = Some(value);
        }
    }

    pub struct AttrIData {
        opt_value: Option<String>,
    }

    impl AttrIData {
        pub fn new(opt_value: Option<String>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrIData {
        fn get_datum(&self) -> RS<Datum> {
            match &self.opt_value {
                None => Ok(Datum::Null),
                Some(value) => Ok(Datum::Typed(DatTyped::String(value.clone()))),
            }
        }

        fn set_datum<D: AsRef<Datum>>(&mut self, datum: D) -> RS<()> {
            match datum.as_ref() {
                Datum::Null => {
                    self.opt_value = None;
                }
                Datum::Typed(typed) => match typed {
                    DatTyped::String(n) => {
                        self.opt_value = Some(n.clone());
                    }
                    _ => {}
                },
                _ => {}
            }
            Ok(())
        }
    }

    impl Attribute<String> for AttrIData {
        fn from_datum(datum: &Datum) -> RS<Self> {
            match datum {
                Datum::Null => Ok(Self { opt_value: None }),
                Datum::Typed(typed) => match typed {
                    DatTyped::String(value) => Ok(Self {
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
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_DATA
        }

        fn is_null(&self) -> bool {
            self.opt_value.is_none()
        }

        fn get_opt_value(&self) -> Option<String> {
            self.opt_value.clone()
        }

        fn set_opt_value(&mut self, opt_value: Option<String>) {
            self.opt_value = opt_value;
        }

        fn get_value(&self) -> String {
            if let Some(value) = &self.opt_value {
                value.clone()
            } else {
                panic!("attribute i_data is null");
            }
        }

        fn set_value(&mut self, value: String) {
            self.opt_value = Some(value);
        }
    }

    pub struct AttrIImId {
        opt_value: Option<i32>,
    }

    impl AttrIImId {
        pub fn new(opt_value: Option<i32>) -> Self {
            Self { opt_value }
        }
    }

    impl AttrDatum for AttrIImId {
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

    impl Attribute<i32> for AttrIImId {
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
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_IM_ID
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
                panic!("attribute i_im_id is null");
            }
        }

        fn set_value(&mut self, value: i32) {
            self.opt_value = Some(value);
        }
    }
} // end mod object
