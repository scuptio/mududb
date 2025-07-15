use crate::code_gen::column_def::TableColumnDef;
use crate::code_gen::src_builder::SrcBuilder;
use crate::code_gen::table_def::TableDef;
use mudu::common::error::ER;
use mudu::common::result::RS;
use mudu::data_type::dt_impl::dat_type_id::DatTypeID;
use mudu::data_type::type_declare::TypeDeclare;
use std::fmt;
use std::fmt::Write;

pub struct RustBuilder {}

pub fn to_struct_field_name(column_name: &str) -> RS<String> {
    Ok(column_name.to_string().to_ascii_lowercase())
}

pub fn to_object_struct_name(table_name: &str) -> RS<String> {
    let object_name = snake_case_to_upper_camel_case(table_name);
    Ok(object_name)
}

fn to_attr_struct_name(column_name: &str) -> RS<String> {
    let n = snake_case_to_upper_camel_case(column_name);
    Ok(format!("Attr{}", n))
}

fn write_error(e: fmt::Error) -> ER {
    ER::WriteError(e.to_string())
}

fn print_indent(s: &str, indent_n: u32, writer: &mut dyn Write) -> RS<()> {
    let vec = s.split("\n").collect::<Vec<&str>>();
    let mut indent_space = String::new();
    for _i in 0..indent_n {
        indent_space.push_str("\t");
    }
    for (i, s) in vec.iter().enumerate() {
        if i != vec.len() - 1 {
            writer
                .write_fmt(format_args!("{indent_space}{s}\n"))
                .map_err(write_error)?;
        }
    }
    Ok(())
}
fn to_data_type(n: &TypeDeclare) -> RS<String> {
    let s = match n.id() {
        DatTypeID::I32 => "i32".to_string(),
        DatTypeID::I64 => "i64".to_string(),
        DatTypeID::F32 => "f32".to_string(),
        DatTypeID::F64 => "f64".to_string(),
        DatTypeID::FixedLenString => "String".to_string(),
        DatTypeID::VarLenString => "String".to_string(),
    };
    Ok(s)
}

fn to_data_typed_enum(type_id: DatTypeID) -> RS<String> {
    let s = match type_id {
        DatTypeID::I32 => "I32".to_string(),
        DatTypeID::I64 => "I64".to_string(),
        DatTypeID::F32 => "F32".to_string(),
        DatTypeID::F64 => "F64".to_string(),
        DatTypeID::FixedLenString => "String".to_string(),
        DatTypeID::VarLenString => "String".to_string(),
    };
    Ok(s)
}

fn is_basic_type(_n: &TypeDeclare) -> bool {
    true
}

fn to_table_name_const(s: &str) -> String {
    format!("TABLE_{}", s.to_ascii_uppercase())
}

fn to_column_name_const(s: &str) -> String {
    format!("COLUMN_{}", s.to_ascii_uppercase())
}

fn snake_case_to_upper_camel_case(n: &str) -> String {
    n.split('_')
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

impl RustBuilder {
    pub fn new() -> Self {
        Self {}
    }

    fn const_list(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        let str_table_name = table_def.table_name();
        let const_table_name = to_table_name_const(&str_table_name);
        writer
            .write_fmt(format_args!(
                "const {const_table_name}:&str = \"{str_table_name}\";\n"
            ))
            .map_err(write_error)?;
        for c in table_def.table_columns() {
            let str_column_name = c.column_name();
            let const_column_name = to_column_name_const(&str_column_name);
            writer
                .write_fmt(format_args!(
                    "const {const_column_name}:&str = \"{str_column_name}\";\n"
                ))
                .map_err(write_error)?;
        }
        Ok(())
    }



    fn use_mod(&self, writer: &mut dyn Write) -> RS<()> {
        // use mod declaration
        writer
            .write_str("use mudu::common::result::RS;\n")
            .map_err(write_error)?;
        writer
            .write_str("use mudu::database::attribute::Attribute;\n")
            .map_err(write_error)?;
        writer
            .write_str("use mudu::database::attr_datum::AttrDatum;\n")
            .map_err(write_error)?;
        writer
            .write_str("use mudu::tuple::datum::Datum;\n")
            .map_err(write_error)?;
        writer
            .write_str("use mudu::data_type::dt_impl::dat_typed::DatTyped;\n")
            .map_err(write_error)?;
        writer
            .write_str("use mudu::database::record::Record;\n")
            .map_err(write_error)?;
        writer
            .write_str("use mudu::database::tuple_row::TupleRow;\n")
            .map_err(write_error)?;
        writer
            .write_str("use mudu::database::row_desc::RowDesc;\n")
            .map_err(write_error)?;
        Ok(())
    }
    fn build_object(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        writer.write_str("pub mod object {\n").map_err(write_error)?;

        let mut use_mod = String::new();
        self.use_mod(&mut use_mod)?;
        print_indent(&use_mod, 1, writer)?;

        writer.write_str("\n\n").map_err(write_error)?;

        let mut const_list = String::new();
        self.const_list(table_def, &mut const_list)?;
        print_indent(&const_list, 1, writer)?;

        writer.write_str("\n\n").map_err(write_error)?;

        let mut obj_struct = String::new();
        self.object_struct(table_def, &mut obj_struct)?;
        print_indent(&obj_struct, 1, writer)?;

        let mut obj_impl = String::new();
        self.impl_object(table_def, &mut obj_impl)?;
        print_indent(&obj_impl, 1, writer)?;

        writer.write_str("\n").map_err(write_error)?;

        let mut impl_record_trait = String::new();
        self.impl_record_trait(table_def, &mut impl_record_trait)?;
        print_indent(&impl_record_trait, 1, writer)?;

        writer.write_fmt(format_args!("\n")).map_err(write_error)?;

        writer.write_fmt(format_args!("\n")).map_err(write_error)?;

        self.struct_impl_attribute_list(table_def, writer)?;

        writer
            .write_str("} // end mod object\n")
            .map_err(write_error)?;

        Ok(())
    }

    fn struct_impl_attribute_list(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        let mut attributes = vec![];
        for column in table_def.table_columns() {
            let mut attribute = String::new();
            self.struct_impl_attribute(table_def.table_name(), column, &mut attribute)?;
            attributes.push(attribute);
        }

        for (i, attribute) in attributes.iter().enumerate() {
            print_indent(attribute, 1, writer)?;
            if i != attributes.len() - 1 {
                writer.write_str("\n").map_err(write_error)?;
            }
        }
        Ok(())
    }

    fn object_struct(&self, def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        let name = to_object_struct_name(def.table_name())?;
        writer
            .write_fmt(format_args!("pub struct {name} {{\n"))
            .map_err(write_error)?;

        for column in def.table_columns() {
            let mut field = String::new();
            self.build_field(column, &mut field)?;
            writer
                .write_fmt(format_args!("\t{field}"))
                .map_err(write_error)?;
        }
        writer
            .write_fmt(format_args!("}}\n\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn impl_object(&self, def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        let name = to_object_struct_name(def.table_name())?;
        writer
            .write_fmt(format_args!("impl {name} {{\n"))
            .map_err(write_error)?;
        let mut methods = Vec::new();
        let mut constructor = String::new();
        self.impl_object_fn_constructor(def, &mut constructor, false)?;
        methods.push(constructor);
        let mut constructor_empty = String::new();
        self.impl_object_fn_constructor(def, &mut constructor_empty, true)?;
        methods.push(constructor_empty);

        let mut get_datum = String::new();
        self.impl_object_fn_get_datum(&mut get_datum)?;
        methods.push(get_datum);

        let mut set_datum = String::new();
        self.impl_object_fn_set_datum(&mut set_datum)?;
        methods.push(set_datum);

        for column in def.table_columns() {
            let mut setter = String::new();
            self.build_setter(column, &mut setter)?;
            methods.push(setter);

            let mut getter = String::new();
            self.build_getter(column, &mut getter)?;
            methods.push(getter);
        }

        for (i, method) in methods.iter().enumerate() {
            print_indent(method, 1, writer)?;
            if i != methods.len() - 1 {
                writer.write_fmt(format_args!("\n")).map_err(write_error)?;
            }
        }

        writer.write_str("}\n").map_err(write_error)?;
        Ok(())
    }

    fn impl_record_trait(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        let table_name = table_def.table_name();
        let struct_obj_name = to_object_struct_name(table_name)?;
        // impl Record for XXX {
        writer
            .write_fmt(format_args!("impl Record for {struct_obj_name} {{\n"))
            .map_err(write_error)?;

        let mut fn_table_name = String::new();
        self.impl_record_fn_table_name(table_name, &mut fn_table_name)?;
        print_indent(&fn_table_name, 1, writer)?;
        writer.write_str("\n").map_err(write_error)?;

        let mut fn_from_tuple = String::new();
        self.impl_record_fn_from_tuple(&struct_obj_name, &mut fn_from_tuple)?;
        print_indent(&fn_from_tuple, 1, writer)?;
        writer.write_str("\n").map_err(write_error)?;

        let mut fn_to_tuple = String::new();
        self.impl_record_fn_to_tuple(table_def, &mut fn_to_tuple)?;
        print_indent(&fn_to_tuple, 1, writer)?;
        writer.write_str("\n").map_err(write_error)?;

        let mut fn_get = String::new();
        self.impl_record_fn_get(table_def, &mut fn_get)?;
        print_indent(&fn_get, 1, writer)?;

        writer.write_str("\n").map_err(write_error)?;

        let mut fn_set = String::new();
        self.impl_record_fn_set(table_def, &mut fn_set)?;
        print_indent(&fn_set, 1, writer)?;

        // } end impl Record for XXX {
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn impl_record_fn_table_name(&self, table_name: &String, writer: &mut dyn Write) -> RS<()> {
        let table_name_const = to_table_name_const(table_name);
        writer
            .write_fmt(format_args!("fn table_name() -> &'static str {{\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t{table_name_const}\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn impl_record_fn_from_tuple(&self, struct_obj_name: &str, writer: &mut dyn Write) -> RS<()> {
        writer.write_fmt(format_args!("fn from_tuple<T:AsRef<TupleRow>, D:AsRef<RowDesc>>(row: T, desc:D) -> RS<Self> {{\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\tlet mut s = Self::new_empty();\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!(
                "\tif row.as_ref().items().len() != desc.as_ref().desc().len() {{\n"
            ))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!(
                "\t\tpanic!(\"{struct_obj_name}::from_tuple wrong length\");\n"
            ))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t}}\n"))
            .map_err(write_error)?;

        writer
            .write_fmt(format_args!(
                "\tfor (i, dat) in row.as_ref().items().iter().enumerate() {{\n"
            ))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t\tlet dd = &desc.as_ref().desc()[i];\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t\ts.set(dd.name(), Some(dat.as_ref()))?;\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t}}\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\tOk(s)\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;

        Ok(())
    }

    fn impl_record_fn_to_tuple(&self, _table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        writer
            .write_fmt(format_args!(
                "fn to_tuple<D:AsRef<RowDesc>>(&self, desc:D) -> RS<TupleRow> {{\n"
            ))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\tlet mut tuple = vec![];\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\tfor d in desc.as_ref().desc() {{\n"))
            .map_err(write_error)?;

        writer
            .write_fmt(format_args!("\t\tlet opt_datum = self.get(d.name())?;\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t\tif let Some(datum) = opt_datum {{\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t\t\ttuple.push(datum);\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t\t}}\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t}}\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\tOk(TupleRow::new(tuple))\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn impl_record_fn_get(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        writer
            .write_fmt(format_args!(
                "fn get(&self, column:&str) -> RS<Option<Datum>> {{\n"
            ))
            .map_err(write_error)?;

        // let datum = match column {
        writer
            .write_fmt(format_args!("\tmatch column {{\n"))
            .map_err(write_error)?;
        for column in table_def.table_columns() {
            let name = column.column_name();
            let column_name_const = to_column_name_const(name);
            let field_name = to_struct_field_name(name)?;
            writer
                .write_fmt(format_args!("\t\t{column_name_const} => {{\n"))
                .map_err(write_error)?;

            writer
                .write_fmt(format_args!("\t\t\tSelf::get_datum(&self.{field_name})\n"))
                .map_err(write_error)?;

            writer
                .write_fmt(format_args!("\t\t}}\n"))
                .map_err(write_error)?;
        }

        writer
            .write_fmt(format_args!("\t\t_ => {{ panic!(\"unknown name\"); }}\n"))
            .map_err(write_error)?;

        // }; END match column {
        writer
            .write_fmt(format_args!("\t}}\n"))
            .map_err(write_error)?;

        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn impl_record_fn_set(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        writer.write_fmt(format_args!("fn set<D:AsRef<Datum>>(&mut self, column:&str, opt_datum:Option<D>) -> RS<()> {{\n"))
            .map_err(write_error)?;
        // match column {{
        writer
            .write_fmt(format_args!("\tmatch column {{\n"))
            .map_err(write_error)?;
        for column in table_def.table_columns() {
            let name = column.column_name();
            let column_name_const = to_column_name_const(name);
            let field_name = to_struct_field_name(name)?;
            writer
                .write_fmt(format_args!("\t\t{column_name_const} => {{\n"))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!(
                    "\t\t\tSelf::set_datum(&mut self.{field_name}, opt_datum)?;\n"
                ))
                .map_err(write_error)?;

            writer
                .write_fmt(format_args!("\t\t}}\n"))
                .map_err(write_error)?;
        }

        writer
            .write_fmt(format_args!("\t\t_ => {{ panic!(\"unknown name\"); }}\n"))
            .map_err(write_error)?;

        // } END match column {{
        writer
            .write_fmt(format_args!("\t}}\n"))
            .map_err(write_error)?;

        writer
            .write_fmt(format_args!("\tOk(())\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }
    

    fn impl_object_fn_constructor(
        &self,
        table_def: &TableDef,
        writer: &mut dyn Write,
        is_empty: bool,
    ) -> RS<()> {
        let constructor_name = if is_empty { "new_empty" } else { "new" };
        writer
            .write_fmt(format_args!("pub fn {constructor_name}(\n"))
            .map_err(write_error)?;
        if !is_empty {
            for column_def in table_def.table_columns() {
                let field_name = to_struct_field_name(column_def.column_name())?;
                let object_name = to_attr_struct_name(column_def.column_name())?;
                writer
                    .write_fmt(format_args!("\t{field_name}:{object_name},\n"))
                    .map_err(write_error)?;
            }
        }
        writer
            .write_fmt(format_args!(") -> Self {{\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\tlet s = Self {{\n"))
            .map_err(write_error)?;
        if is_empty {
            for column_def in table_def.table_columns() {
                let field_name = to_struct_field_name(column_def.column_name())?;
                writer
                    .write_fmt(format_args!("\t\t{field_name}:None,\n"))
                    .map_err(write_error)?;
            }

            writer
                .write_fmt(format_args!("\t}};\n"))
                .map_err(write_error)?;
        } else {
            for column_def in table_def.table_columns() {
                let field_name = to_struct_field_name(column_def.column_name())?;
                writer
                    .write_fmt(format_args!("\t\t{field_name}:Some({field_name}),\n"))
                    .map_err(write_error)?;
            }

            writer
                .write_fmt(format_args!("\t}};\n"))
                .map_err(write_error)?;
        }

        writer
            .write_fmt(format_args!("\ts\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn impl_object_fn_get_datum(&self, writer: &mut dyn Write) -> RS<()> {
        writer
            .write_str(
                r##"
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
"##,
            )
            .map_err(write_error)?;
        Ok(())
    }

    fn impl_object_fn_set_datum(&self, writer: &mut dyn Write) -> RS<()> {
        writer
            .write_str(
                r##"
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
"##,
            )
            .map_err(write_error)?;
        Ok(())
    }

    fn build_field(&self, column_def: &TableColumnDef, writer: &mut dyn Write) -> RS<()> {
        let field_name = to_struct_field_name(column_def.column_name())?;
        let data_type = to_attr_struct_name(&column_def.column_name())?;
        writer
            .write_fmt(format_args!("{field_name} : Option<{data_type}>,\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_setter(&self, column_def: &TableColumnDef, writer: &mut dyn Write) -> RS<()> {
        let field_name = to_struct_field_name(column_def.column_name())?;
        let data_type = to_attr_struct_name(&column_def.column_name())?;
        writer
            .write_fmt(format_args!("pub fn set_{field_name}(\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t& mut self,\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t{field_name} : {data_type},\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("){{\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\tself.{field_name} = Some({field_name});\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_getter(&self, column_def: &TableColumnDef, writer: &mut dyn Write) -> RS<()> {
        let field_name = to_struct_field_name(column_def.column_name())?;
        let data_type = to_attr_struct_name(column_def.column_name())?;
        writer
            .write_fmt(format_args!("pub fn get_{field_name}(\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t& self,\n"))
            .map_err(write_error)?;
        let is_basic_type = is_basic_type(column_def.data_type());

        if is_basic_type {
            writer
                .write_fmt(format_args!(") -> & Option<{data_type}> {{\n"))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!("\t& self.{field_name}\n"))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\t) -> & Option<{data_type}> {{\n"))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!("\t & self.{field_name}\n"))
                .map_err(write_error)?;
        }
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn struct_impl_attribute(
        &self,
        table_name: &String,
        column_def: &TableColumnDef,
        writer: &mut dyn Write,
    ) -> RS<()> {
        let column_name = column_def.column_name();
        let attr_obj_name = to_attr_struct_name(column_name)?;

        // pub struct AttrXXX {
        writer
            .write_fmt(format_args!("pub struct {attr_obj_name} {{\n"))
            .map_err(write_error)?;
        let data_type = to_data_type(&column_def.data_type())?;
        let (field_name, opt_data_type) = if column_def.is_not_null() {
            ("value".to_string(), data_type.clone())
        } else {
            ("opt_value".to_string(), format!("Option<{}>", data_type))
        };
        writer
            .write_fmt(format_args!("\t{field_name} : {opt_data_type}, \n"))
            .map_err(write_error)?;

        // }
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        writer.write_fmt(format_args!("\n")).map_err(write_error)?;

        // impl Object
        writer
            .write_fmt(format_args!("impl {attr_obj_name} {{\n"))
            .map_err(write_error)?;
        let mut attr_fn = String::new();
        self.build_attr_fn(
            &attr_obj_name,
            &data_type,
            column_def.is_not_null(),
            &mut attr_fn,
        )?;
        print_indent(&attr_fn, 1, writer)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        writer.write_fmt(format_args!("\n")).map_err(write_error)?;

        // impl AttrDatum trait
        writer
            .write_fmt(format_args!("impl AttrDatum for {attr_obj_name} {{\n"))
            .map_err(write_error)?;
        let mut attr_trait_fn = String::new();
        self.build_attr_datum_trait_fn(column_def, &mut attr_trait_fn)?;
        print_indent(&attr_trait_fn, 1, writer)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        writer.write_fmt(format_args!("\n")).map_err(write_error)?;

        // impl Attribute trait
        writer
            .write_fmt(format_args!(
                "impl Attribute<{data_type}> for {attr_obj_name} {{\n"
            ))
            .map_err(write_error)?;
        let mut attr_trait_fn = String::new();
        self.build_attr_trait_fn(table_name, column_def, &mut attr_trait_fn)?;
        print_indent(&attr_trait_fn, 1, writer)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;

        Ok(())
    }

    fn build_attr_fn_constructor(
        &self,
        _attr_obj_name: &String,
        data_type: &String,
        is_null: bool,
        writer: &mut dyn Write,
    ) -> RS<()> {
        if is_null {
            writer
                .write_fmt(format_args!("pub fn new(value: {data_type}) -> Self {{\n"))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!("\tSelf {{ value }}\n"))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!(
                    "pub fn new(opt_value: Option<{data_type}>) -> Self {{\n"
                ))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!("\tSelf {{ opt_value }}\n"))
                .map_err(write_error)?;
        }
        writer.write_str("}\n").map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn(
        &self,
        attr_obj_name: &String,
        data_type: &String,
        is_null: bool,
        writer: &mut dyn Write,
    ) -> RS<()> {
        self.build_attr_fn_constructor(attr_obj_name, data_type, is_null, writer)?;
        Ok(())
    }

    fn build_attr_datum_trait_fn(
        &self,
        column_def: &TableColumnDef,
        writer: &mut dyn Write,
    ) -> RS<()> {
        self.build_attr_datum_fn_get_datum(
            column_def.data_type().id(),
            column_def.is_not_null(),
            writer,
        )?;
        writer.write_str("\n").map_err(write_error)?;

        self.build_attr_datum_fn_set_datum(
            column_def.data_type().id(),
            column_def.is_not_null(),
            writer,
        )?;
        writer.write_str("\n").map_err(write_error)?;
        Ok(())
    }

    fn build_attr_trait_fn(
        &self,
        table_name: &String,
        column_def: &TableColumnDef,
        writer: &mut dyn Write,
    ) -> RS<()> {
        let column_name = column_def.column_name();
        self.build_attr_fn_from_datum(
            column_def.data_type().id(),
            column_def.is_not_null(),
            writer,
        )?;
        writer.write_str("\n").map_err(write_error)?;
        self.build_attr_fn_table_name(table_name, writer)?;
        writer.write_str("\n").map_err(write_error)?;
        self.build_attr_fn_column_name(column_name, writer)?;
        writer.write_str("\n").map_err(write_error)?;
        self.build_attr_fn_is_null(column_def.is_not_null(), writer)?;
        writer.write_str("\n").map_err(write_error)?;
        let data_type = to_data_type(&column_def.data_type())?;

        self.build_attr_fn_get_opt_value(column_def.is_not_null(), &data_type, writer)?;
        writer.write_str("\n").map_err(write_error)?;

        self.build_attr_fn_set_opt_value(column_def.is_not_null(), &data_type, writer)?;
        writer.write_str("\n").map_err(write_error)?;

        self.build_attr_fn_get_value(column_name, column_def.is_not_null(), &data_type, writer)?;
        writer.write_str("\n").map_err(write_error)?;
        self.build_attr_fn_set_value(column_def.is_not_null(), &data_type, writer)?;
        writer.write_str("\n").map_err(write_error)?;

        Ok(())
    }

    fn build_attr_fn_table_name(&self, table_name: &String, writer: &mut dyn Write) -> RS<()> {
        let table_name_const = to_table_name_const(table_name);
        writer
            .write_fmt(format_args!("fn table_name() -> &'static str {{\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t{table_name_const}\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn_column_name(&self, column_name: &String, writer: &mut dyn Write) -> RS<()> {
        let column_name_const = to_column_name_const(column_name);
        writer
            .write_fmt(format_args!("fn column_name() -> &'static str {{\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("\t{column_name_const}\n"))
            .map_err(write_error)?;
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn_is_null(&self, not_null: bool, writer: &mut dyn Write) -> RS<()> {
        writer
            .write_fmt(format_args!("fn is_null(&self) -> bool {{\n"))
            .map_err(write_error)?;

        if not_null {
            writer
                .write_fmt(format_args!("\tfalse\n"))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\tself.opt_value.is_none()\n"))
                .map_err(write_error)?;
        }

        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn_from_datum(
        &self,
        type_id: DatTypeID,
        not_null: bool,
        writer: &mut dyn Write,
    ) -> RS<()> {
        writer
            .write_fmt(format_args!("fn from_datum(datum:&Datum) -> RS<Self> {{\n"))
            .map_err(write_error)?;
        let typed_enum = to_data_typed_enum(type_id)?;

        // begin match datum
        writer
            .write_fmt(format_args!("\tmatch datum {{\n"))
            .map_err(write_error)?;

        // begin Datum::Null case
        writer
            .write_fmt(format_args!("\t\tDatum::Null => {{\n"))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_fmt(format_args!(
                    "\t\t\t panic!(\"cannot set non-null attribute NULL\")\n"
                ))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\t\t\t Ok(Self {{ opt_value: None }}) \n"))
                .map_err(write_error)?;
        }
        // end Datum::Null case
        writer
            .write_fmt(format_args!("\t\t}}\n"))
            .map_err(write_error)?;

        // Datum::Typed => {{
        writer
            .write_fmt(format_args!("\t\tDatum::Typed(typed) => {{\n"))
            .map_err(write_error)?;

        // match typed {
        writer
            .write_fmt(format_args!("\t\t\tmatch typed  {{\n"))
            .map_err(write_error)?;
        // DatTyped::{typed_enum}(n) =>
        writer
            .write_fmt(format_args!(
                "\t\t\t\tDatTyped::{typed_enum}(value) => {{\n"
            ))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_fmt(format_args!(
                    "\t\t\t\t\tOk(Self {{ value: value.clone() }})\n"
                ))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!(
                    "\t\t\t\t\tOk(Self {{ opt_value : Some(value.clone()) }})\n"
                ))
                .map_err(write_error)?;
        }
        // } end DatTyped::{typed_enum}(n) =>
        writer
            .write_fmt(format_args!("\t\t\t\t}}\n"))
            .map_err(write_error)?;

        // other DatTyped case
        writer
            .write_fmt(format_args!("\t\t\t\t_ => {{ unimplemented!() }}\n"))
            .map_err(write_error)?;

        // }  end match typed {
        writer
            .write_fmt(format_args!("\t\t\t}}\n"))
            .map_err(write_error)?;

        // } end Datum::Typed => {{
        writer
            .write_fmt(format_args!("\t\t}}\n"))
            .map_err(write_error)?;

        // other Datum::_ case
        writer
            .write_fmt(format_args!("\t\t_ => {{ unimplemented!() }}\n"))
            .map_err(write_error)?;

        // end match datum
        writer
            .write_fmt(format_args!("\t}}\n"))
            .map_err(write_error)?;

        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_datum_fn_set_datum(
        &self,
        type_id: DatTypeID,
        not_null: bool,
        writer: &mut dyn Write,
    ) -> RS<()> {
        writer
            .write_fmt(format_args!(
                "fn set_datum<D:AsRef<Datum>>(&mut self, datum:D) -> RS<()> {{\n"
            ))
            .map_err(write_error)?;
        let typed_enum = to_data_typed_enum(type_id)?;

        // begin match datum
        writer
            .write_fmt(format_args!("\tmatch datum.as_ref() {{\n"))
            .map_err(write_error)?;

        // begin Datum::Null case
        writer
            .write_fmt(format_args!("\t\tDatum::Null => {{\n"))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_fmt(format_args!(
                    "\t\t\t panic!(\"cannot set non-null attribute NULL\")\n"
                ))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\t\t\t self.opt_value = None; \n"))
                .map_err(write_error)?;
        }
        // end Datum::Null case
        writer
            .write_fmt(format_args!("\t\t}}\n"))
            .map_err(write_error)?;

        // Datum::Typed => {{
        writer
            .write_fmt(format_args!("\t\tDatum::Typed(typed) => {{\n"))
            .map_err(write_error)?;

        // match typed {
        writer
            .write_fmt(format_args!("\t\t\tmatch typed  {{\n"))
            .map_err(write_error)?;
        // DatTyped::{typed_enum}(n) =>
        writer
            .write_fmt(format_args!("\t\t\t\tDatTyped::{typed_enum}(n) => {{\n"))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_fmt(format_args!("\t\t\t\t\tself.value = n.clone();\n"))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!(
                    "\t\t\t\t\tself.opt_value = Some(n.clone());\n"
                ))
                .map_err(write_error)?;
        }
        // } end DatTyped::{typed_enum}(n) =>
        writer
            .write_fmt(format_args!("\t\t\t\t}}\n"))
            .map_err(write_error)?;

        // other DatTyped case
        writer
            .write_fmt(format_args!("\t\t\t\t_ => {{}}\n"))
            .map_err(write_error)?;

        // }  end match typed {
        writer
            .write_fmt(format_args!("\t\t\t}}\n"))
            .map_err(write_error)?;

        // } end Datum::Typed => {{
        writer
            .write_fmt(format_args!("\t\t}}\n"))
            .map_err(write_error)?;

        // other Datum::_ case
        writer
            .write_fmt(format_args!("\t\t_ => {{ }}\n"))
            .map_err(write_error)?;

        // end match datum
        writer
            .write_fmt(format_args!("\t}}\n"))
            .map_err(write_error)?;

        writer
            .write_fmt(format_args!("\tOk(())\n"))
            .map_err(write_error)?;

        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_datum_fn_get_datum(
        &self,
        type_id: DatTypeID,
        not_null: bool,
        writer: &mut dyn Write,
    ) -> RS<()> {
        writer
            .write_fmt(format_args!("fn get_datum(&self) -> RS<Datum> {{\n"))
            .map_err(write_error)?;
        let typed_enum = to_data_typed_enum(type_id)?;
        if not_null {
            writer
                .write_fmt(format_args!(
                    "\tOk(Datum::Typed(DatTyped::{typed_enum}(self.value.clone())))\n"
                ))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\tmatch &self.opt_value {{\n"))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!("\t\tNone => Ok(Datum::Null),\n"))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!("\t\tSome(value) =>"))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!(
                    " Ok(Datum::Typed(DatTyped::{typed_enum}(value.clone())))\n"
                ))
                .map_err(write_error)?;
            writer
                .write_fmt(format_args!("\t}}\n"))
                .map_err(write_error)?;
        }

        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn_set_opt_value(
        &self,
        not_null: bool,
        data_type: &String,
        writer: &mut dyn Write,
    ) -> RS<()> {
        writer
            .write_fmt(format_args!(
                "fn set_opt_value(&mut self, opt_value : Option<{data_type}>) {{\n"
            ))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_str("\tif let Some(value) = opt_value {\n")
                .map_err(write_error)?;
            writer
                .write_str("\t\tself.value = value;\n")
                .map_err(write_error)?;
            writer.write_str("\t}\n").map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\tself.opt_value = opt_value;\n"))
                .map_err(write_error)?;
        }
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn_get_opt_value(
        &self,
        not_null: bool,
        data_type: &String,
        writer: &mut dyn Write,
    ) -> RS<()> {
        writer
            .write_fmt(format_args!(
                "fn get_opt_value(&self) -> Option<{data_type}> {{\n"
            ))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_fmt(format_args!("\tSome(self.value.clone())\n"))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\tself.opt_value.clone()\n"))
                .map_err(write_error)?;
        }
        writer
            .write_fmt(format_args!("}}\n"))
            .map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn_set_value(
        &self,
        not_null: bool,
        data_type: &String,
        writer: &mut dyn Write,
    ) -> RS<()> {
        writer
            .write_fmt(format_args!(
                "fn set_value(&mut self, value : {data_type}) {{\n"
            ))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_fmt(format_args!("\tself.value = value;\n"))
                .map_err(write_error)?;
        } else {
            writer
                .write_fmt(format_args!("\tself.opt_value = Some(value);\n"))
                .map_err(write_error)?;
        }
        writer.write_str("}\n").map_err(write_error)?;
        Ok(())
    }

    fn build_attr_fn_get_value(
        &self,
        attr_name: &String,
        not_null: bool,
        data_type: &String,
        writer: &mut dyn Write,
    ) -> RS<()> {
        writer
            .write_fmt(format_args!("fn get_value(&self) -> {data_type} {{\n"))
            .map_err(write_error)?;
        if not_null {
            writer
                .write_fmt(format_args!("\tself.value.clone()\n"))
                .map_err(write_error)?;
        } else {
            writer
                .write_str("\tif let Some(value) = &self.opt_value {\n")
                .map_err(write_error)?;
            writer
                .write_str("\t\tvalue.clone()\n")
                .map_err(write_error)?;
            writer.write_str("\t}\n").map_err(write_error)?;
            writer.write_str("\telse {\n").map_err(write_error)?;
            writer
                .write_fmt(format_args!(
                    "\t\tpanic!(\"attribute {attr_name} is null\");\n"
                ))
                .map_err(write_error)?;
            writer.write_str("\t}\n").map_err(write_error)?;
        }
        writer.write_str("}\n").map_err(write_error)?;
        Ok(())
    }
}

impl SrcBuilder for RustBuilder {
    fn build(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()> {
        self.build_object(table_def, writer)?;
        Ok(())
    }
}
