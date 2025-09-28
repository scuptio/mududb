#![no_main]
#[macro_use]
extern crate libfuzzer_sys;

use mududb::fuzz::_fuzz_run::_target;

fuzz_target!(|param:&[u8]| {
    _target("_de_en_x_l_batch", param);
});