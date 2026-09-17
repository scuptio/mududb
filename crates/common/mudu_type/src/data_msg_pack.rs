use mudu::utils::msg_pack::MsgPackValue;

#[derive(Clone)]
pub struct DataMsgPack {
    value: MsgPackValue,
}

impl DataMsgPack {
    pub fn from(buf: MsgPackValue) -> Self {
        Self { value: buf }
    }

    pub fn msg_pack(&self) -> &MsgPackValue {
        &self.value
    }

    pub fn into_msg_pack(self) -> MsgPackValue {
        self.value
    }
}
