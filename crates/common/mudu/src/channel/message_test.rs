use super::{ChannelMessage, ChannelReply, MessageKind, ServiceCtx};
use crate::error::ErrorCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Unencodable;

impl Serialize for Unencodable {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("payload rejected"))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct FailedRequest(Unencodable);
#[derive(Debug, Serialize, Deserialize)]
struct FailedReply(Unencodable);
#[derive(Debug, Serialize, Deserialize)]
struct FailedNotification(Unencodable);

crate::impl_channel_message!(
    FailedRequest,
    reply = FailedReply,
    kind = MessageKind::User(0x7f03)
);
crate::impl_channel_message!(
    FailedNotification,
    tell_only,
    kind = MessageKind::User(0x7f04)
);

#[test]
fn codec_errors_preserve_error_code_and_cause() {
    for error in [
        FailedRequest(Unencodable).encode().unwrap_err(),
        FailedReply(Unencodable).encode().unwrap_err(),
        FailedNotification(Unencodable).encode().unwrap_err(),
    ] {
        assert_eq!(error.ec(), ErrorCode::Encode);
        assert!(format!("{error:?}").contains("payload rejected"));
    }
    let nil = [0xc0];
    assert!(FailedRequest::decode(&nil).is_ok());
    assert!(FailedReply::decode(&nil).is_ok());
    assert!(FailedNotification::decode(&nil).is_ok());
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Request(String);
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Reply(u64);
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Notification(Vec<u8>);

crate::impl_channel_message!(
    Request,
    reply = Reply,
    kind = MessageKind::User(0x7f01),
    idempotent = true
);
crate::impl_channel_message!(Notification, tell_only, kind = MessageKind::User(0x7f02));

#[test]
fn message_and_reply_roundtrip_and_reject_invalid_payloads() {
    let request = Request("你好\0world".into());
    assert_eq!(
        Request::decode(&request.encode().unwrap()).unwrap(),
        request
    );
    let reply = Reply(u64::MAX);
    assert_eq!(Reply::decode(&reply.encode().unwrap()).unwrap(), reply);
    let notification = Notification(vec![0, 128, 255]);
    assert_eq!(
        Notification::decode(&notification.encode().unwrap()).unwrap(),
        notification
    );
    assert_eq!(Request::KIND, MessageKind::User(0x7f01));
    assert_eq!(Notification::KIND, MessageKind::User(0x7f02));
    for bytes in [&[][..], &[0xc1][..]] {
        assert_eq!(Request::decode(bytes).unwrap_err().ec(), ErrorCode::Decode);
        assert_eq!(Reply::decode(bytes).unwrap_err().ec(), ErrorCode::Decode);
        assert_eq!(
            Notification::decode(bytes).unwrap_err().ec(),
            ErrorCode::Decode
        );
    }
}

#[test]
fn context_preserves_full_identity_and_retry_attempt() {
    let ctx = ServiceCtx::new(u128::MAX, 1 << 100, u32::MAX);
    assert_eq!(ctx.src(), u128::MAX);
    assert_eq!(ctx.request_id(), 1 << 100);
    assert_eq!(ctx.attempt(), u32::MAX);
    assert!(ChannelReply::encode(&()).unwrap().is_empty());
    <() as ChannelReply>::decode(&[]).unwrap();
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Ids {
    #[serde(with = "super::wire::serde_u128")]
    required: u128,
    #[serde(with = "super::wire::serde_opt_u128")]
    optional: Option<u128>,
}

#[test]
fn wire_ids_preserve_high_bits_and_reject_wrong_lengths() {
    for required in [0, 1 << 127, u128::MAX] {
        for optional in [None, Some(0), Some(u128::MAX)] {
            let ids = Ids { required, optional };
            let encoded = rmp_serde::to_vec(&ids).unwrap();
            assert_eq!(rmp_serde::from_slice::<Ids>(&encoded).unwrap(), ids);
        }
    }
    for length in [0, 1, 15, 17, 32] {
        let bad = vec![0_u8; length];
        let valid = vec![255_u8; 16];
        for pair in [(bad.clone(), Some(valid.clone())), (valid, Some(bad))] {
            let encoded = rmp_serde::to_vec(&pair).unwrap();
            let error = rmp_serde::from_slice::<Ids>(&encoded).unwrap_err();
            assert!(error.to_string().contains("invalid u128 byte length"));
        }
    }
}
