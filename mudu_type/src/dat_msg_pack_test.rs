#[cfg(test)]
mod tests {
    use crate::dat_msg_pack::DatMsgPack;
    use mudu::utils::msg_pack::MsgPackValue;

    #[test]
    fn dat_msg_pack_constructors_and_accessors() {
        let inner = MsgPackValue::Nil;
        let wrapper = DatMsgPack::from(inner.clone());
        assert!(matches!(wrapper.msg_pack(), MsgPackValue::Nil));
        assert!(matches!(wrapper.into_msg_pack(), MsgPackValue::Nil));
    }
}
