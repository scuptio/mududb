use crate::tuple::read_datum::{read_fixed_len_value, read_var_len_value};
use crate::tuple::slot::Slot;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use serde::{Deserialize, Serialize};

/// Metadata descriptor for a binary format tuple's field
/// Contains structural information about how the field is stored in its binary format tuple
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldDesc {
    oid: OID,
    is_fixed_len: bool,
    slot: Slot,
    type_obj: DatType,
}

impl FieldDesc {
    /// Constructs a new FieldDesc
    /// # Arguments
    /// * `slot` - Storage slot information
    /// * `data_type` - Type identifier from TypeID
    /// * `type_param` - Configuration parameters for the data type
    /// * `is_fixed_len` - Whether the type has fixed-length storage
    /// # Panics
    /// If the data_type's inherent fixed-length property doesn't match is_fixed_len parameter
    pub fn new(slot: Slot, data_type: DatType, is_fixed_len: bool) -> Self {
        Self {
            oid: 0,
            is_fixed_len,
            slot,
            type_obj: data_type,
        }
    }

    /// Returns the field's unique identifier
    pub fn id(&self) -> OID {
        self.oid
    }

    /// Extracts the field's value from raw tuple bytes
    /// # Arguments
    /// * `tuple` - Raw byte slice containing the tuple data
    /// # Returns
    /// * `RS<&[u8]>` - Result containing reference to the field's byte slice
    /// # Behavior
    /// * For fixed-length fields: Direct offset+length access
    /// * For variable-length fields: Reads length prefix from tuple header
    pub fn get<'a>(&self, tuple: &'a [u8]) -> RS<&'a [u8]> {
        if self.is_fixed_len {
            read_fixed_len_value(self.slot.offset(), self.slot.length(), tuple)
        } else {
            read_var_len_value(self.slot.offset(), tuple)
        }
    }

    /// Returns reference to storage slot information
    pub fn slot(&self) -> &Slot {
        &self.slot
    }

    /// Returns the data type identifier
    pub fn data_type(&self) -> DatTypeID {
        self.type_obj.dat_type_id()
    }

    /// Returns reference to parameter object of the field's type
    pub fn type_obj(&self) -> &DatType {
        &self.type_obj
    }

    /// Indicates if the field is a fixed-length type
    pub fn is_fixed_len(&self) -> bool {
        self.is_fixed_len
    }
}
