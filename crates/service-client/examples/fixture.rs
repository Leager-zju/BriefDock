//! Offline UI/API contract fixture; no business service or network dependency.
#![cfg_attr(windows, windows_subsystem = "windows")]
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let mut preferences = json!({"auto_refresh":false,"refresh_minutes":5,"followed":[],"hidden":[],"order":[],"default_followed":false});
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?).map_err(io::Error::other)?;
        let data = match request["method"].as_str() {
            Some("health") => json!({"service":"contract-fixture","api_version":1}),
            Some("sources") => json!([
                {"id":"first","name":"Sample A","category":"Sample","subtitle":""},
                {"id":"second","name":"Sample B","category":"Sample","subtitle":""}
            ]),
            Some("state") => json!({"preferences":preferences,"cache":{}}),
            Some("configure") => {
                preferences = request["args"]["preferences"].clone();
                json!({"preferences":preferences})
            }
            Some("preferences") => json!({"preferences":preferences}),
            Some("fetch") => json!([]),
            Some("lifecycle.suspend") => json!({"suspended":true}),
            _ => {
                serde_json::to_writer(
                    &mut output,
                    &json!({"version":1,"id":request["id"],"ok":false,"error":{"code":"unknown_method"}}),
                )?;
                writeln!(output)?;
                output.flush()?;
                continue;
            }
        };
        serde_json::to_writer(
            &mut output,
            &json!({"version":1,"id":request["id"],"ok":true,"data":data}),
        )?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
