use crate::dat_type::DatType;
use crate::datum::Datum;
use std::any::TypeId;

pub struct DtOfDatum {
    id_2_type: scc::HashMap<TypeId, Box<DatType>>,
}

impl DtOfDatum {
    pub fn new() -> Self {
        Self {
            id_2_type: scc::HashMap::new(),
        }
    }

    pub fn ty_of_datum<D: Datum, F: FnOnce() -> DatType>(
        &'static self,
        new_type: F,
    ) -> &'static DatType {
        let id = TypeId::of::<D>();
        let dat_type = match self.id_2_type.get_sync(&id) {
            Some(dt) => {
                let boxed: &Box<DatType> = dt.get();
                let pointer: *const DatType = &**boxed;
                pointer
            }
            None => {
                let boxed = Box::new(new_type());
                let pointer: *const DatType = &*boxed;
                let _ = self.id_2_type.insert_sync(id, boxed);
                pointer
            }
        };
        unsafe { &*dat_type }
    }
}
