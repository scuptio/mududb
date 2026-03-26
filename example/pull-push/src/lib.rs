wit_bindgen::generate!({
    inline:
    r##"package mudu:pull-push;

    world pull-push {
        export push: func(message: list<u8>) -> list<u8>;
        export pull: func(message: list<u8>) -> list<u8>;
    }
    "##,
    async: true
});

use mudu::common::result::RS;

struct KeyValueComponent;

impl Guest for KeyValueComponent {
    async fn push(message: Vec<u8>) -> Vec<u8> {
        match kv_put(&message, &message).await {
            Ok(()) => message,
            Err(err) => err.to_string().into_bytes(),
        }
    }

    async fn pull(message: Vec<u8>) -> Vec<u8> {
        match kv_get(&message).await {
            Ok(Some(value)) => value,
            Ok(None) => Vec::new(),
            Err(err) => err.to_string().into_bytes(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn kv_put(key: &[u8], value: &[u8]) -> RS<()> {
    sys_interface::api::mudu_put(0, key, value).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn kv_put(key: &[u8], value: &[u8]) -> RS<()> {
    sys_interface::api::mudu_put(0, key, value)
}

#[cfg(target_arch = "wasm32")]
async fn kv_get(key: &[u8]) -> RS<Option<Vec<u8>>> {
    sys_interface::api::mudu_get(0, key).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn kv_get(key: &[u8]) -> RS<Option<Vec<u8>>> {
    sys_interface::api::mudu_get(0, key)
}

export!(KeyValueComponent);
