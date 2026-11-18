use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn cli_handles_parse_edit_search_backlinks_daily_page_and_errors() {
    let requests = [
        json!({"id":"p","op":"parse","document":"- [[Home]] #work\n"}),
        json!({"id":"e","op":"edit","document":"- old\n","args":{"action":"set_text","block_id":1,"text":"new"}}),
        json!({"id":"s","op":"search","document":"- 가나다\n","args":{"query":"나다"}}),
        json!({"id":"b","op":"backlinks","document":"- [[Home]]\n","args":{"target":"Home"}}),
        json!({"id":"d","op":"daily_page","args":{"date":"2026-11-18"}}),
        json!({"id":"x","op":"search","document":"- x\n","args":{"query":"x"}}),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_outliner"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for request in &requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), requests.len());
    assert_eq!(responses[0]["id"], "p");
    assert_eq!(responses[0]["ok"], true);
    assert_eq!(responses[1]["result"]["markdown"], "- new\n");
    assert_eq!(responses[2]["result"]["block_ids"], json!([1]));
    assert_eq!(responses[3]["result"]["block_ids"], json!([1]));
    assert_eq!(responses[4]["result"]["filename"], "2026-11-18.md");
    assert_eq!(responses[5]["error"]["code"], "QUERY_TOO_SHORT");
}
