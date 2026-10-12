#![cfg(target_os = "linux")]

#[path = "wallet_mpk_common/mod.rs"]
mod wallet_mpk_common;

use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use std::sync::LazyLock;
use wallet_mpk_common::WalletGuestSpec;

static WALLET_GO_MPK_TEST_LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));

// End-to-end check that the TinyGo guest packaged as `wallet-go.mpk` speaks
// the MSSP v1 ABI against the real runtime: the Go component, which imports
// `mududb:api/system` directly and frames MSSP through the in-repo Go
// binding (`types` + `codec`), is installed into a full mudud backend and
// its procedures are invoked over HTTP. Correct query/command results prove
// the guest's syscalls were framed through MSSP correctly.
//
// The test skips (passes) when wallet-go.mpk has not been built, so machines
// without the TinyGo toolchain stay green.
#[cfg_attr(miri, ignore)]
#[test]
fn wallet_go_mpk_http_end_to_end_tokio() -> RS<()> {
    let _guard = WALLET_GO_MPK_TEST_LOCK
        .lock()
        .expect("wallet-go mpk test lock poisoned");
    wallet_mpk_common::run_wallet_guest_mpk_http_scenario(&WalletGuestSpec {
        app_name: "wallet-go",
        module: "wallet_go",
        mpk_file: "wallet-go.mpk",
        package_hint: "example/wallet-go",
        temp_prefix: "mududb-testing-wallet-go",
        level_as_i64: false,
    })
}
