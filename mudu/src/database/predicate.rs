use crate::database::filter::Filter;


pub struct Predicate {
    filter:Vec<Vec<Filter>>
}


impl Predicate {
    pub fn new(filter_and:Vec<Filter>) -> Self {
        let mut s = Self {
            filter:Vec::new()
        };
        s.filter.push(filter_and);
        s
    }
    
    pub fn or(&mut self, predicate:Predicate) -> &mut Self {
        let mut filter = predicate.filter;
        self.filter.append(&mut filter);
        self
    }
}