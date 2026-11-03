#![no_main]

use libfuzzer_sys::fuzz_target;
use l7_load_balancer::config::Config;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(mut config) = toml::from_str::<Config>(s) {
            let _ = config.validate();
        }
    }
});

