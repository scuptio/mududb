#![cfg(target_os = "linux")]

#[path = "wallet_mpk_common/mod.rs"]
mod wallet_mpk_common;

use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use std::sync::LazyLock;
use wallet_mpk_common::WalletGuestSpec;

static WALLET_PY_MPK_TEST_LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));

// End-to-end check that the componentize-py guest packaged as `wallet-py.mpk`
// speaks the MSSP v1 ABI against the real runtime: the CPython component,
// which imports `mududb:api/system` directly and frames MSSP through the
// `mududb` Python package, is installed into a full mudud backend and its
// procedures are invoked over HTTP. Correct query/command results prove the
// guest's syscalls were framed through MSSP correctly.
//
// The test skips (passes) when wallet-py.mpk has not been built, so machines
// without the componentize-py toolchain stay green.
#[cfg_attr(miri, ignore)]
#[test]
fn wallet_py_mpk_http_end_to_end_tokio() -> RS<()> {
    let _guard = WALLET_PY_MPK_TEST_LOCK
        .lock()
        .expect("wallet-py mpk test lock poisoned");
    wallet_mpk_common::run_wallet_guest_mpk_http_scenario(&WalletGuestSpec {
        app_name: "wallet-py",
        module: "wallet_py",
        mpk_file: "wallet-py.mpk",
        package_hint: "example/wallet-py",
        temp_prefix: "mududb-testing-wallet-py",
        level_as_i64: true,
    })
}
