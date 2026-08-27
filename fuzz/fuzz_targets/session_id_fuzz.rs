#![no_main]

use libfuzzer_sys::fuzz_target;
use phi_agent::session::validate_session_id;

fuzz_target!(|data: &str| {
    let _ = validate_session_id(data);
});
