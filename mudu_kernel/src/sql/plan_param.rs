use std::any::Any;

pub type PParam = Box<dyn Any + Send>;

/// use Box::into_inner when it is stable
fn box_into_inner<T: 'static>(boxed: Box<T>) -> T {
    *boxed
}

pub fn downcast_param<T: 'static>(param: PParam) -> T {
    let r_downcast = param.downcast::<T>();
    match r_downcast {
        Ok(param) => box_into_inner(param),
        Err(_e) => {
            panic!("downcast to build parameter error")
        }
    }
}
