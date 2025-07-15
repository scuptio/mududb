pub mod object {
	use mudu::common::result::RS;
	use mudu::data_type::dt_impl::dat_typed::DatTyped;
	use mudu::database::attr_datum::AttrDatum;
	use mudu::database::attribute::Attribute;
	use mudu::database::record::Record;
	use mudu::database::row_desc::RowDesc;
	use mudu::database::tuple_row::TupleRow;
	use mudu::tuple::datum::Datum;
	const TABLE_TRANSACTIONS:&str = "transactions";
	const COLUMN_TRANS_ID:&str = "trans_id";
	const COLUMN_FROM_USER:&str = "from_user";
	const COLUMN_TO_USER:&str = "to_user";
	const COLUMN_AMOUNT:&str = "amount";
	const COLUMN_CREATED_AT:&str = "created_at";


	pub struct Transactions {
		trans_id : Option<AttrTransId>,
		from_user : Option<AttrFromUser>,
		to_user : Option<AttrToUser>,
		amount : Option<AttrAmount>,
		created_at : Option<AttrCreatedAt>,
	}
	
	impl Transactions {
		pub fn new(
			trans_id:AttrTransId,
			from_user:AttrFromUser,
			to_user:AttrToUser,
			amount:AttrAmount,
			created_at:AttrCreatedAt,
		) -> Self {
			let s = Self {
				trans_id:Some(trans_id),
				from_user:Some(from_user),
				to_user:Some(to_user),
				amount:Some(amount),
				created_at:Some(created_at),
			};
			s
		}
	
		pub fn new_empty(
		) -> Self {
			let s = Self {
				trans_id:None,
				from_user:None,
				to_user:None,
				amount:None,
				created_at:None,
			};
			s
		}
	
		
		fn get_datum<R, A:Attribute<R>>(
		    attribute: & Option<A>
		) -> RS<Option<Datum>> {
		    let opt_datum = match  attribute  {
		        Some(value) => {
		            Some(value.get_datum()?)
		        }
		        None => {
		            None
		        }
		    };
		    Ok(opt_datum)
		}
	
		
		fn set_datum<R, A:Attribute<R>, D:AsRef<Datum>>(
		    attribute: &mut Option<A>,
		    opt_datum:Option<D>
		) -> RS<()> {
		    match  attribute  {
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
	
		pub fn set_trans_id(
			& mut self,
			trans_id : AttrTransId,
		){
			self.trans_id = Some(trans_id);
		}
	
		pub fn get_trans_id(
			& self,
		) -> & Option<AttrTransId> {
			& self.trans_id
		}
	
		pub fn set_from_user(
			& mut self,
			from_user : AttrFromUser,
		){
			self.from_user = Some(from_user);
		}
	
		pub fn get_from_user(
			& self,
		) -> & Option<AttrFromUser> {
			& self.from_user
		}
	
		pub fn set_to_user(
			& mut self,
			to_user : AttrToUser,
		){
			self.to_user = Some(to_user);
		}
	
		pub fn get_to_user(
			& self,
		) -> & Option<AttrToUser> {
			& self.to_user
		}
	
		pub fn set_amount(
			& mut self,
			amount : AttrAmount,
		){
			self.amount = Some(amount);
		}
	
		pub fn get_amount(
			& self,
		) -> & Option<AttrAmount> {
			& self.amount
		}
	
		pub fn set_created_at(
			& mut self,
			created_at : AttrCreatedAt,
		){
			self.created_at = Some(created_at);
		}
	
		pub fn get_created_at(
			& self,
		) -> & Option<AttrCreatedAt> {
			& self.created_at
		}
	}

	impl Record for Transactions {
		fn table_name() -> &'static str {
			TABLE_TRANSACTIONS
		}
	
		fn from_tuple<T:AsRef<TupleRow>, D:AsRef<RowDesc>>(row: T, desc:D) -> RS<Self> {
			let mut s = Self::new_empty();
			if row.as_ref().items().len() != desc.as_ref().desc().len() {
				panic!("Transactions::from_tuple wrong length");
			}
			for (i, dat) in row.as_ref().items().iter().enumerate() {
				let dd = &desc.as_ref().desc()[i];
				s.set(dd.name(), Some(dat.as_ref()))?;
			}
			Ok(s)
		}
	
		fn to_tuple<D:AsRef<RowDesc>>(&self, desc:D) -> RS<TupleRow> {
			let mut tuple = vec![];
			for d in desc.as_ref().desc() {
				let opt_datum = self.get(d.name())?;
				if let Some(datum) = opt_datum {
					tuple.push(datum);
				}
			}
			Ok(TupleRow::new(tuple))
		}
	
		fn get(&self, column:&str) -> RS<Option<Datum>> {
			match column {
				COLUMN_TRANS_ID => {
					Self::get_datum(&self.trans_id)
				}
				COLUMN_FROM_USER => {
					Self::get_datum(&self.from_user)
				}
				COLUMN_TO_USER => {
					Self::get_datum(&self.to_user)
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
	
		fn set<D:AsRef<Datum>>(&mut self, column:&str, opt_datum:Option<D>) -> RS<()> {
			match column {
				COLUMN_TRANS_ID => {
					Self::set_datum(&mut self.trans_id, opt_datum)?;
				}
				COLUMN_FROM_USER => {
					Self::set_datum(&mut self.from_user, opt_datum)?;
				}
				COLUMN_TO_USER => {
					Self::set_datum(&mut self.to_user, opt_datum)?;
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


	pub struct AttrTransId {
		value : i32, 
	}
	
	impl AttrTransId {
		pub fn new(value: i32) -> Self {
			Self { value }
		}
	}
	
	impl AttrDatum for AttrTransId {
		fn get_datum(&self) -> RS<Datum> {
			Ok(Datum::Typed(DatTyped::I32(self.value.clone())))
		}
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 panic!("cannot set non-null attribute NULL")
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::I32(n) => {
							self.value = n.clone();
						}
						_ => {}
					}
				}
				_ => { }
			}
			Ok(())
		}
		
	}
	
	impl Attribute<i32> for AttrTransId {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 panic!("cannot set non-null attribute NULL")
				}
				Datum::Typed(typed) => {
					match typed  {
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
			TABLE_TRANSACTIONS
		}
		
		fn column_name() -> &'static str {
			COLUMN_TRANS_ID
		}
		
		fn is_null(&self) -> bool {
			false
		}
		
		fn get_opt_value(&self) -> Option<i32> {
			Some(self.value.clone())
		}
		
		fn set_opt_value(&mut self, opt_value : Option<i32>) {
			if let Some(value) = opt_value {
				self.value = value;
			}
		}
		
		fn get_value(&self) -> i32 {
			self.value.clone()
		}
		
		fn set_value(&mut self, value : i32) {
			self.value = value;
		}
		
	}

	pub struct AttrFromUser {
		opt_value : Option<i32>, 
	}
	
	impl AttrFromUser {
		pub fn new(opt_value: Option<i32>) -> Self {
			Self { opt_value }
		}
	}
	
	impl AttrDatum for AttrFromUser {
		fn get_datum(&self) -> RS<Datum> {
			match &self.opt_value {
				None => Ok(Datum::Null),
				Some(value) => Ok(Datum::Typed(DatTyped::I32(value.clone())))
			}
		}
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::I32(n) => {
							self.opt_value = Some(n.clone());
						}
						_ => {}
					}
				}
				_ => { }
			}
			Ok(())
		}
		
	}
	
	impl Attribute<i32> for AttrFromUser {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::I32(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_TRANSACTIONS
		}
		
		fn column_name() -> &'static str {
			COLUMN_FROM_USER
		}
		
		fn is_null(&self) -> bool {
			self.opt_value.is_none()
		}
		
		fn get_opt_value(&self) -> Option<i32> {
			self.opt_value.clone()
		}
		
		fn set_opt_value(&mut self, opt_value : Option<i32>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> i32 {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute from_user is null");
			}
		}
		
		fn set_value(&mut self, value : i32) {
			self.opt_value = Some(value);
		}
		
	}

	pub struct AttrToUser {
		opt_value : Option<i32>, 
	}
	
	impl AttrToUser {
		pub fn new(opt_value: Option<i32>) -> Self {
			Self { opt_value }
		}
	}
	
	impl AttrDatum for AttrToUser {
		fn get_datum(&self) -> RS<Datum> {
			match &self.opt_value {
				None => Ok(Datum::Null),
				Some(value) => Ok(Datum::Typed(DatTyped::I32(value.clone())))
			}
		}
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::I32(n) => {
							self.opt_value = Some(n.clone());
						}
						_ => {}
					}
				}
				_ => { }
			}
			Ok(())
		}
		
	}
	
	impl Attribute<i32> for AttrToUser {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::I32(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_TRANSACTIONS
		}
		
		fn column_name() -> &'static str {
			COLUMN_TO_USER
		}
		
		fn is_null(&self) -> bool {
			self.opt_value.is_none()
		}
		
		fn get_opt_value(&self) -> Option<i32> {
			self.opt_value.clone()
		}
		
		fn set_opt_value(&mut self, opt_value : Option<i32>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> i32 {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute to_user is null");
			}
		}
		
		fn set_value(&mut self, value : i32) {
			self.opt_value = Some(value);
		}
		
	}

	pub struct AttrAmount {
		opt_value : Option<f64>, 
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
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::F64(n) => {
							self.opt_value = Some(n.clone());
						}
						_ => {}
					}
				}
				_ => { }
			}
			Ok(())
		}
		
	}
	
	impl Attribute<f64> for AttrAmount {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::F64(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_TRANSACTIONS
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
		
		fn set_opt_value(&mut self, opt_value : Option<f64>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> f64 {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute amount is null");
			}
		}
		
		fn set_value(&mut self, value : f64) {
			self.opt_value = Some(value);
		}
		
	}

	pub struct AttrCreatedAt {
		opt_value : Option<i64>, 
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
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::I64(n) => {
							self.opt_value = Some(n.clone());
						}
						_ => {}
					}
				}
				_ => { }
			}
			Ok(())
		}
		
	}
	
	impl Attribute<i64> for AttrCreatedAt {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::I64(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_TRANSACTIONS
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
		
		fn set_opt_value(&mut self, opt_value : Option<i64>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> i64 {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute created_at is null");
			}
		}
		
		fn set_value(&mut self, value : i64) {
			self.opt_value = Some(value);
		}
		
	}
} // end mod object
