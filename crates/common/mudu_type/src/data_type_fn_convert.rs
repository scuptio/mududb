use crate::data_json::DataJson;
use crate::type_error::TyErr;
use crate::{
    data_binary::DataBinary, data_textual::DataTextual, data_type::DataType, data_value::DataValue,
};
use mudu::utils::json::JsonValue;
use mudu::utils::msg_pack::MsgPackValue;
// =============================================================================
// Function Type Definitions
// =============================================================================

/// Converts external textual representation to internal representation
pub type FnInputTextual = fn(&str, &DataType) -> Result<DataValue, TyErr>;

/// Converts internal representation to external textual representation
pub type FnOutputTextual = fn(&DataValue, &DataType) -> Result<DataTextual, TyErr>;

/// Converts external textual representation to internal representation
pub type FnInputJson = fn(&JsonValue, &DataType) -> Result<DataValue, TyErr>;

/// Converts internal representation to external textual representation
pub type FnOutputJson = fn(&DataValue, &DataType) -> Result<DataJson, TyErr>;

/// Converts internal msg pack value representation to internal representation
pub type FnInputMsgPack = fn(&MsgPackValue, &DataType) -> Result<DataValue, TyErr>;

/// Converts internal representation to external msg pack representation
pub type FnOutputMsgPack = fn(&DataValue, &DataType) -> Result<MsgPackValue, TyErr>;

/// Returns fixed byte length for fixed-length data types
pub type FnTypeLength = fn(&DataType) -> Result<Option<u32>, TyErr>;

/// Returns byte length for variable-length data types
pub type FnDataLength = fn(&DataValue, &DataType) -> Result<u32, TyErr>;

/// Converts internal representation to external binary representation
pub type FnSend = fn(&DataValue, &DataType) -> Result<DataBinary, TyErr>;

/// Converts internal representation to external binary representation into provided buffer
pub type FnSendTo = fn(&DataValue, &DataType, &mut [u8]) -> Result<u32, TyErr>;

/// Converts external binary representation to internal representation
pub type FnReceive = fn(&[u8], &DataType) -> Result<(DataValue, u32), TyErr>;

/// Provides default value for data type
pub type FnDefault = fn(&DataType) -> Result<DataValue, TyErr>;

// =============================================================================
// Core Function Structure
// =============================================================================

/// Collection of base functions that define data type operations
pub struct FnBase {
    /// Converts text input to internal representation
    pub input_textual: FnInputTextual,
    /// Converts internal representation to text output
    pub output_textual: FnOutputTextual,
    /// Converts JSON input to internal representation
    pub input_json: FnInputJson,
    /// Converts internal representation to JSON output
    pub output_json: FnOutputJson,
    /// Converts MsgPack Value input to internal representation
    pub input_msg_pack: FnInputMsgPack,
    /// Converts internal representation to MsgPack Value output
    pub output_msg_pack: FnOutputMsgPack,
    /// Returns fixed length for data type
    pub type_len: FnTypeLength,
    /// Returns byte length for variable-length data type
    pub data_len: FnDataLength,
    /// Receives binary data and converts to internal representation
    pub receive: FnReceive,
    /// Sends internal representation as binary data
    pub send: FnSend,
    /// Sends internal representation to provided buffer
    pub send_to: FnSendTo,
    /// Provides default value for data type
    pub default: FnDefault,
}
