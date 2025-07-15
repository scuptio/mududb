pub mod object {
	use mudu::common::result::RS;
	use mudu::data_type::dt_impl::dat_typed::DatTyped;
	use mudu::database::attr_datum::AttrDatum;
	use mudu::database::attribute::Attribute;
	use mudu::database::record::Record;
	use mudu::database::row_desc::RowDesc;
	use mudu::database::tuple_row::TupleRow;
	use mudu::tuple::datum::Datum;
	const TABLE_USERS:&str = "users";
	const COLUMN_USER_ID:&str = "user_id";
	const COLUMN_NAME:&str = "name";
	const COLUMN_PHONE:&str = "phone";
	const COLUMN_EMAIL:&str = "email";
	const COLUMN_PASSWORD:&str = "password";
	const COLUMN_CREATED_AT:&str = "created_at";


	pub struct Users {
		user_id : Option<AttrUserId>,
		name : Option<AttrName>,
		phone : Option<AttrPhone>,
		email : Option<AttrEmail>,
		password : Option<AttrPassword>,
		created_at : Option<AttrCreatedAt>,
	}
	
	impl Users {
		pub fn new(
			user_id:AttrUserId,
			name:AttrName,
			phone:AttrPhone,
			email:AttrEmail,
			password:AttrPassword,
			created_at:AttrCreatedAt,
		) -> Self {
			let s = Self {
				user_id:Some(user_id),
				name:Some(name),
				phone:Some(phone),
				email:Some(email),
				password:Some(password),
				created_at:Some(created_at),
			};
			s
		}
	
		pub fn new_empty(
		) -> Self {
			let s = Self {
				user_id:None,
				name:None,
				phone:None,
				email:None,
				password:None,
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
	
		pub fn set_user_id(
			& mut self,
			user_id : AttrUserId,
		){
			self.user_id = Some(user_id);
		}
	
		pub fn get_user_id(
			& self,
		) -> & Option<AttrUserId> {
			& self.user_id
		}
	
		pub fn set_name(
			& mut self,
			name : AttrName,
		){
			self.name = Some(name);
		}
	
		pub fn get_name(
			& self,
		) -> & Option<AttrName> {
			& self.name
		}
	
		pub fn set_phone(
			& mut self,
			phone : AttrPhone,
		){
			self.phone = Some(phone);
		}
	
		pub fn get_phone(
			& self,
		) -> & Option<AttrPhone> {
			& self.phone
		}
	
		pub fn set_email(
			& mut self,
			email : AttrEmail,
		){
			self.email = Some(email);
		}
	
		pub fn get_email(
			& self,
		) -> & Option<AttrEmail> {
			& self.email
		}
	
		pub fn set_password(
			& mut self,
			password : AttrPassword,
		){
			self.password = Some(password);
		}
	
		pub fn get_password(
			& self,
		) -> & Option<AttrPassword> {
			& self.password
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

	impl Record for Users {
		fn table_name() -> &'static str {
			TABLE_USERS
		}
	
		fn from_tuple<T:AsRef<TupleRow>, D:AsRef<RowDesc>>(row: T, desc:D) -> RS<Self> {
			let mut s = Self::new_empty();
			if row.as_ref().items().len() != desc.as_ref().desc().len() {
				panic!("Users::from_tuple wrong length");
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
				COLUMN_USER_ID => {
					Self::get_datum(&self.user_id)
				}
				COLUMN_NAME => {
					Self::get_datum(&self.name)
				}
				COLUMN_PHONE => {
					Self::get_datum(&self.phone)
				}
				COLUMN_EMAIL => {
					Self::get_datum(&self.email)
				}
				COLUMN_PASSWORD => {
					Self::get_datum(&self.password)
				}
				COLUMN_CREATED_AT => {
					Self::get_datum(&self.created_at)
				}
				_ => { panic!("unknown name"); }
			}
		}
	
		fn set<D:AsRef<Datum>>(&mut self, column:&str, opt_datum:Option<D>) -> RS<()> {
			match column {
				COLUMN_USER_ID => {
					Self::set_datum(&mut self.user_id, opt_datum)?;
				}
				COLUMN_NAME => {
					Self::set_datum(&mut self.name, opt_datum)?;
				}
				COLUMN_PHONE => {
					Self::set_datum(&mut self.phone, opt_datum)?;
				}
				COLUMN_EMAIL => {
					Self::set_datum(&mut self.email, opt_datum)?;
				}
				COLUMN_PASSWORD => {
					Self::set_datum(&mut self.password, opt_datum)?;
				}
				COLUMN_CREATED_AT => {
					Self::set_datum(&mut self.created_at, opt_datum)?;
				}
				_ => { panic!("unknown name"); }
			}
			Ok(())
		}
	}


	pub struct AttrUserId {
		value : i32, 
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
	
	impl Attribute<i32> for AttrUserId {
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
			TABLE_USERS
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

	pub struct AttrName {
		opt_value : Option<String>, 
	}
	
	impl AttrName {
		pub fn new(opt_value: Option<String>) -> Self {
			Self { opt_value }
		}
	}
	
	impl AttrDatum for AttrName {
		fn get_datum(&self) -> RS<Datum> {
			match &self.opt_value {
				None => Ok(Datum::Null),
				Some(value) => Ok(Datum::Typed(DatTyped::String(value.clone())))
			}
		}
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(n) => {
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
	
	impl Attribute<String> for AttrName {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_USERS
		}
		
		fn column_name() -> &'static str {
			COLUMN_NAME
		}
		
		fn is_null(&self) -> bool {
			self.opt_value.is_none()
		}
		
		fn get_opt_value(&self) -> Option<String> {
			self.opt_value.clone()
		}
		
		fn set_opt_value(&mut self, opt_value : Option<String>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> String {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute name is null");
			}
		}
		
		fn set_value(&mut self, value : String) {
			self.opt_value = Some(value);
		}
		
	}

	pub struct AttrPhone {
		opt_value : Option<String>, 
	}
	
	impl AttrPhone {
		pub fn new(opt_value: Option<String>) -> Self {
			Self { opt_value }
		}
	}
	
	impl AttrDatum for AttrPhone {
		fn get_datum(&self) -> RS<Datum> {
			match &self.opt_value {
				None => Ok(Datum::Null),
				Some(value) => Ok(Datum::Typed(DatTyped::String(value.clone())))
			}
		}
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(n) => {
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
	
	impl Attribute<String> for AttrPhone {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_USERS
		}
		
		fn column_name() -> &'static str {
			COLUMN_PHONE
		}
		
		fn is_null(&self) -> bool {
			self.opt_value.is_none()
		}
		
		fn get_opt_value(&self) -> Option<String> {
			self.opt_value.clone()
		}
		
		fn set_opt_value(&mut self, opt_value : Option<String>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> String {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute phone is null");
			}
		}
		
		fn set_value(&mut self, value : String) {
			self.opt_value = Some(value);
		}
		
	}

	pub struct AttrEmail {
		opt_value : Option<String>, 
	}
	
	impl AttrEmail {
		pub fn new(opt_value: Option<String>) -> Self {
			Self { opt_value }
		}
	}
	
	impl AttrDatum for AttrEmail {
		fn get_datum(&self) -> RS<Datum> {
			match &self.opt_value {
				None => Ok(Datum::Null),
				Some(value) => Ok(Datum::Typed(DatTyped::String(value.clone())))
			}
		}
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(n) => {
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
	
	impl Attribute<String> for AttrEmail {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_USERS
		}
		
		fn column_name() -> &'static str {
			COLUMN_EMAIL
		}
		
		fn is_null(&self) -> bool {
			self.opt_value.is_none()
		}
		
		fn get_opt_value(&self) -> Option<String> {
			self.opt_value.clone()
		}
		
		fn set_opt_value(&mut self, opt_value : Option<String>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> String {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute email is null");
			}
		}
		
		fn set_value(&mut self, value : String) {
			self.opt_value = Some(value);
		}
		
	}

	pub struct AttrPassword {
		opt_value : Option<String>, 
	}
	
	impl AttrPassword {
		pub fn new(opt_value: Option<String>) -> Self {
			Self { opt_value }
		}
	}
	
	impl AttrDatum for AttrPassword {
		fn get_datum(&self) -> RS<Datum> {
			match &self.opt_value {
				None => Ok(Datum::Null),
				Some(value) => Ok(Datum::Typed(DatTyped::String(value.clone())))
			}
		}
		
		fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {
			match datum.as_ref() {
				Datum::Null => {
					 self.opt_value = None; 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(n) => {
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
	
	impl Attribute<String> for AttrPassword {
		fn from_datum(datum:&Datum) -> RS<Self> {
			match datum {
				Datum::Null => {
					 Ok(Self { opt_value: None }) 
				}
				Datum::Typed(typed) => {
					match typed  {
						DatTyped::String(value) => {
							Ok(Self { opt_value : Some(value.clone()) })
						}
						_ => { unimplemented!() }
					}
				}
				_ => { unimplemented!() }
			}
		}
		
		fn table_name() -> &'static str {
			TABLE_USERS
		}
		
		fn column_name() -> &'static str {
			COLUMN_PASSWORD
		}
		
		fn is_null(&self) -> bool {
			self.opt_value.is_none()
		}
		
		fn get_opt_value(&self) -> Option<String> {
			self.opt_value.clone()
		}
		
		fn set_opt_value(&mut self, opt_value : Option<String>) {
			self.opt_value = opt_value;
		}
		
		fn get_value(&self) -> String {
			if let Some(value) = &self.opt_value {
				value.clone()
			}
			else {
				panic!("attribute password is null");
			}
		}
		
		fn set_value(&mut self, value : String) {
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
			TABLE_USERS
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
