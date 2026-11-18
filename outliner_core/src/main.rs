use outliner_core::handle_request;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(request) => handle_request(request),
                Err(error) => {
                    json!({"id": null, "ok": false, "error": {"code": "INVALID_JSON", "message": error.to_string()}})
                }
            },
            Err(error) => {
                json!({"id": null, "ok": false, "error": {"code": "IO_ERROR", "message": error.to_string()}})
            }
        };
        let _ = writeln!(output, "{response}");
        let _ = output.flush();
    }
}
