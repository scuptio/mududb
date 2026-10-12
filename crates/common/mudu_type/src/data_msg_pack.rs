use mudu::utils::msg_pack::MsgPackValue;

/// A datum in MessagePack form.
#[derive(Clone)]
pub struct DataMsgPack {
    value: MsgPackValue,
}

impl DataMsgPack {
    /// Wraps a MessagePack value into a datum.
    pub fn from(buf: MsgPackValue) -> Self {
        Self { value: buf }
    }

    /// Returns the underlying MessagePack value.
    pub fn msg_pack(&self) -> &MsgPackValue {
        &self.value
    }

    /// Consumes the datum and returns the underlying MessagePack value.
    pub fn into_msg_pack(self) -> MsgPackValue {
        self.value
    }
}
