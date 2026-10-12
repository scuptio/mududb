#![cfg(target_os = "linux")]

#[path = "wallet_mpk_common/mod.rs"]
mod wallet_mpk_common;

use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use std::sync::LazyLock;
use wallet_mpk_common::WalletGuestSpec;

static WALLET_C_MPK_TEST_LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));

// End-to-end check that the freestanding C guest packaged as `wallet-c.mpk`
// speaks the MSSP v1 ABI against the real runtime: the C component, which
// imports `mududb:api/system` directly and frames MSSP through the in-repo
// C binding (`mududb/types` + `mududb/codec`), is installed into a full
// mudud backend and its procedures are invoked over HTTP. Correct
// query/command results prove the guest's syscalls were framed through MSSP
// correctly.
//
// The test skips (passes) when wallet-c.mpk has not been built, so machines
// without a wasm32 clang toolchain stay green.
#[cfg_attr(miri, ignore)]
#[test]
fn wallet_c_mpk_http_end_to_end_tokio() -> RS<()> {
    let _guard = WALLET_C_MPK_TEST_LOCK
        .lock()
        .expect("wallet-c mpk test lock poisoned");
    wallet_mpk_common::run_wallet_guest_mpk_http_scenario(&WalletGuestSpec {
        app_name: "wallet-c",
        module: "wallet_c",
        mpk_file: "wallet-c.mpk",
        package_hint: "example/wallet-c",
        temp_prefix: "mududb-testing-wallet-c",
        level_as_i64: false,
    })
}
