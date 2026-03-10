//! Stream JSON output (line-delimited JSON)

use serde::Serialize;

pub fn print_stream_json<T: Serialize>(value: &T) {
    match serde_json::to_string(value) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Error serializing to stream JSON: {}", e),
    }
}
