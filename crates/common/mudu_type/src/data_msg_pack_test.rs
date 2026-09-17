#[cfg(test)]
mod tests {
    use crate::data_msg_pack::DataMsgPack;
    use mudu::utils::msg_pack::MsgPackValue;

    #[test]
    fn data_msg_pack_constructors_and_accessors() {
        let inner = MsgPackValue::Nil;
        let wrapper = DataMsgPack::from(inner.clone());
        assert!(matches!(wrapper.msg_pack(), MsgPackValue::Nil));
        assert!(matches!(wrapper.into_msg_pack(), MsgPackValue::Nil));
    }
}
