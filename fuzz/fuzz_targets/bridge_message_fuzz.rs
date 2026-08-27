#![no_main]

use libfuzzer_sys::fuzz_target;
use phi_agent::bridge::messages::IncomingMessage;

fuzz_target!(|data: &str| {
    let _ = serde_json::from_str::<IncomingMessage>(data);
});
