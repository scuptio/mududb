pub fn lang_handle_tuple(inner:&Vec<String>) -> String {
    let mut s = String::from("(");
    for (i, ty_s) in inner.iter().enumerate() {
        s.push_str(ty_s);
        if i != inner.len() - 1 {
            s.push_str(", ");
        }
    }
    s.push_str(")");
    s
}