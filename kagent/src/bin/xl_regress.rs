#[path = "../util.rs"] mod util;
#[path = "../json.rs"] mod json;
use json::Value;
use std::io::{Read, Write};

fn main() {
    const LIMIT: usize = 8 * 1024 * 1024;
    let path = std::env::args().nth(1).expect("input file argument");
    let mut input = Vec::new();
    std::fs::File::open(path).unwrap().take((LIMIT + 1) as u64).read_to_end(&mut input).unwrap();
    assert!(input.len() <= LIMIT, "input limit exceeded");
    let root = json::parse(std::str::from_utf8(&input).unwrap()).expect("json");
    let Value::Arr(items) = root else { panic!("array required") };
    let mut results = Vec::new();
    for item in items {
        let name = item.str("name").unwrap_or("");
        let src = util::unbase64(item.str("input").unwrap_or("")).unwrap_or_default();
        assert!(src.len() <= LIMIT, "HTML limit exceeded");
        let out = excel_target::convert(&src, item.str("shading").unwrap_or("keep"));
        results.push(Value::obj(vec![("name", name.into()), ("output", util::base64(&out).into())]));
    }
    let _ = std::io::stdout().write_all(json::to_string(&Value::Arr(results)).as_bytes());
}

#[path = "../excel_target.rs"] mod excel_target;
