use mudu::common::id::OID;
use mudu::common::result::RS;

/**mudu-proc**/
pub fn command(oid:OID, message: Vec<u8>) -> RS<Vec<u8>> {
    Ok(message)
}

/**mudu-proc**/
pub fn event(oid:OID) -> RS<Vec<u8>> {
    Ok(Vec::new())
}