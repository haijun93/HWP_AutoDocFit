//! PHOTOPATH1 합성 DB 왕복 시험용. DB와 사진 경로를 모두 명시해야 한다.
use photoview_win::library::{Library, resolved_key, stored_path};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert!(args.len() >= 4, "path_peer <db> <operation> <photo> [tag]");
    let library = Library::new(args[1].clone().into());
    let path = &args[3];
    let value = match args[2].as_str() {
        "path" => serde_json::json!(stored_path(path)),
        "read" => {
            let mark = library.marks().unwrap().get(&resolved_key(path)).cloned().unwrap_or_default();
            serde_json::json!({"favorite": mark.favorite, "tags": mark.tags})
        }
        "favorite" => serde_json::json!(library.toggle_favorite(path).unwrap()),
        "add" => { library.add_tag(path, &args[4]).unwrap(); serde_json::Value::Null }
        "remove" => { library.remove_tag(path, &args[4]).unwrap(); serde_json::Value::Null }
        _ => panic!("unknown operation"),
    };
    println!("{value}");
}
