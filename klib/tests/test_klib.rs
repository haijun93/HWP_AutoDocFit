//! 가상 관계형 자료함 엔진 통합 테스트.
//! CRUD, 자료 추가/사본, 공간 이동, 관계선, 검색, 물리 폴더 전환, IPC 통신, 격리 검증.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

// klib 내부 모듈 접근을 위한 선언 (크레이트 바이너리 테스트)
#[path = "../src/scan.rs"]
mod scan;
#[path = "../src/db.rs"]
mod db;
#[path = "../src/json.rs"]
mod json;
#[path = "../src/pipe.rs"]
mod pipe;
#[path = "../src/util.rs"]
mod util;
#[path = "../src/search.rs"]
mod search;
#[path = "../src/notebook.rs"]
mod notebook;
#[path = "../src/nb_transform.rs"]
mod nb_transform;
#[path = "../src/nb_store.rs"]
mod nb_store;
#[path = "../src/content.rs"]
mod content;
#[path = "../src/reindex.rs"]
mod reindex;

static SEARCH_INDEX: std::sync::OnceLock<Result<search::SearchIndex, String>> = std::sync::OnceLock::new();
fn search_index() -> Result<&'static search::SearchIndex, String> {
    // 시험마다 지우는 KLIB_TEST_ROOT가 아니라 프로세스 단위 폴더에 둔다(먼저 부른 시험의 폴더에 묶이면 순서에 따라 실패).
    SEARCH_INDEX.get_or_init(|| { let dir=std::env::var("KLIB_TANTIVY_DIR").unwrap_or_else(|_| std::env::temp_dir().join(format!("klib-test-tantivy-{}", std::process::id())).to_string_lossy().into_owned()); search::SearchIndex::open_or_create(dir).map_err(|e|e.to_string()) }).as_ref().map_err(Clone::clone)
}

use db::Database;
use json::Value;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

struct TestContext {
    dir: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl TestContext {
    fn new(name: &str) -> Self {
        let guard = TEST_MUTEX.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("klib-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("KLIB_TEST_ROOT", dir.to_str().unwrap());
        }
        TestContext { dir, _guard: guard }
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("KLIB_TEST_ROOT");
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn test_box_crud() {
    let ctx = TestContext::new("box_crud");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();

    // Create
    let b1 = db.box_create("프로젝트A", "보고서 준비용 자료함").unwrap();
    assert!(b1 > 0);

    // List
    let list = db.box_list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "프로젝트A");
    assert_eq!(list[0].description, "보고서 준비용 자료함");
    assert_eq!(list[0].item_count, 0);

    // Get
    let detail = db.box_get(b1).unwrap().expect("자료함 존재");
    assert_eq!(detail.0.name, "프로젝트A");
    assert!(detail.1.is_empty());
    assert!(detail.2.is_empty());

    // Update
    let updated = db.box_update(b1, "프로젝트A-수정", "설명 변경").unwrap();
    assert!(updated);
    let detail2 = db.box_get(b1).unwrap().unwrap();
    assert_eq!(detail2.0.name, "프로젝트A-수정");
    assert_eq!(detail2.0.description, "설명 변경");

    // Delete
    let deleted = db.box_delete(b1).unwrap();
    assert!(deleted);
    assert!(db.box_get(b1).unwrap().is_none());
}

/// kvf 가상 폴더 창이 직접 읽는 box 5열과 item 9열의 이름·순서를 고정한다.
#[test]
fn test_kvf_read_column_contract() {
    let ctx = TestContext::new("kvf_columns");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let conn = db.connection();

    let box_columns = {
        let mut stmt = conn.prepare("SELECT id, name, description, created_at, updated_at FROM box LIMIT 0").unwrap();
        stmt.column_names().map(str::to_string).collect::<Vec<_>>()
    };
    assert_eq!(box_columns, ["id", "name", "description", "created_at", "updated_at"]);

    let item_columns = {
        let mut stmt = conn.prepare(
            "SELECT id, box_id, kind, source_path, display_name, space, created_at, origin_path, managed_path FROM item LIMIT 0",
        ).unwrap();
        stmt.column_names().map(str::to_string).collect::<Vec<_>>()
    };
    assert_eq!(item_columns, ["id", "box_id", "kind", "source_path", "display_name", "space", "created_at", "origin_path", "managed_path"]);
}

#[test]
fn test_item_types_and_copy() {
    let ctx = TestContext::new("item_types");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("자료형식시험", "").unwrap();

    // 1. 파일
    let sample_file = ctx.dir.join("sample.hwp");
    fs::write(&sample_file, "한글문서내용").unwrap();
    let it_file = db.item_add(b1, "file", sample_file.to_str().unwrap(), "표준서식.hwp", "basis", 10.0, 20.0, None).unwrap();
    assert!(it_file > 0);

    // 2. 글 (데이터 폴더에 사본 보관)
    let it_text = db.item_add(b1, "text", "", "회의메모", "core", 50.0, 60.0, Some("2026-11-03 회의 안건 내용")).unwrap();
    let text_row = db.item_get(it_text).unwrap().unwrap();
    assert!(Path::new(&text_row.source_path).is_file());
    let saved_text = fs::read_to_string(&text_row.source_path).unwrap();
    assert_eq!(saved_text, "2026-11-03 회의 안건 내용");

    // 3. 그림 (Base64 사본 보관)
    let png_header_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    let it_img = db.item_add(b1, "image", "", "스크린샷1", "temp", 0.0, 0.0, Some(png_header_b64)).unwrap();
    let img_row = db.item_get(it_img).unwrap().unwrap();
    assert!(Path::new(&img_row.source_path).is_file());
    assert!(img_row.source_path.ends_with(".png"));

    // 4. 링크
    let it_link = db.item_add(b1, "link", "https://example.com/doc", "참고링크", "reference", 30.0, 40.0, None).unwrap();
    let link_row = db.item_get(it_link).unwrap().unwrap();
    assert_eq!(link_row.source_path, "https://example.com/doc");

    // 개수 확인
    let list = db.box_list().unwrap();
    assert_eq!(list[0].item_count, 4);
}

#[test]
fn test_item_move_and_coordinates() {
    let ctx = TestContext::new("item_move");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("이동시험", "").unwrap();

    let it = db.item_add(b1, "note", "", "메모1", "temp", 0.0, 0.0, Some("내용")).unwrap();
    let it_row = db.item_get(it).unwrap().unwrap();
    assert_eq!(it_row.space, "temp");
    assert_eq!(it_row.x, 0.0);
    assert_eq!(it_row.y, 0.0);

    // 공간 이동: temp -> core, 좌표: (150.5, 300.0)
    let moved = db.item_move(it, "core", 150.5, 300.0).unwrap();
    assert!(moved);

    let moved_row = db.item_get(it).unwrap().unwrap();
    assert_eq!(moved_row.space, "core");
    assert_eq!(moved_row.x, 150.5);
    assert_eq!(moved_row.y, 300.0);

    // 유효하지 않은 공간명 거부 검증
    let invalid = db.item_move(it, "invalid_space", 10.0, 10.0);
    assert!(invalid.is_err());
}

#[test]
fn test_relation_crud() {
    let ctx = TestContext::new("relation_crud");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("관계시험", "").unwrap();
    let it1 = db.item_add(b1, "text", "", "핵심기록", "core", 10.0, 10.0, Some("핵심")).unwrap();
    let it2 = db.item_add(b1, "note", "", "근거메모", "core", 100.0, 10.0, Some("근거")).unwrap();

    // 관계선 추가
    let rel_id = db.relation_add(b1, it1, it2, "근거 자료 연결").unwrap();
    assert!(rel_id > 0);

    // 자기 자신에 대한 관계선 거부 확인
    let self_rel = db.relation_add(b1, it1, it1, "자기 연결");
    assert!(self_rel.is_err());

    // 관계선 목록
    let list = db.relation_list(b1).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].from_item, it1);
    assert_eq!(list[0].to_item, it2);
    assert_eq!(list[0].label, "근거 자료 연결");

    // 다른 자료함의 항목과 연결 시도 시 거부 확인
    let b2 = db.box_create("다른자료함", "").unwrap();
    let it3 = db.item_add(b2, "note", "", "타자료", "temp", 0.0, 0.0, Some("타")).unwrap();
    let cross_err = db.relation_add(b1, it1, it3, "교차 연결 시도");
    assert!(cross_err.is_err());

    // 존재하지 않는 자료함 ID 또는 항목 ID로 관계선 시도 시 거부 확인 (지적 2)
    let non_exist_box = db.relation_add(99999, it1, it2, "없는 자료함");
    assert!(non_exist_box.is_err());
    let non_exist_err = db.relation_add(b1, 9999, 9998, "없는 연결");
    assert!(non_exist_err.is_err());

    // 관계선 삭제
    let del = db.relation_delete(rel_id).unwrap();
    assert!(del);
    assert!(db.relation_list(b1).unwrap().is_empty());
}

#[test]
fn test_item_search() {
    let ctx = TestContext::new("item_search");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("자료함1", "").unwrap();
    let b2 = db.box_create("자료함2", "").unwrap();

    db.item_add(b1, "text", "", "2026년 업무보고서.hwpx", "core", 0.0, 0.0, Some("내용")).unwrap();
    db.item_add(b1, "text", "", "예산 집행 계획.txt", "basis", 0.0, 0.0, Some("내용")).unwrap();
    db.item_add(b2, "text", "", "기타 보고서.hwpx", "temp", 0.0, 0.0, Some("내용")).unwrap();

    // 1. 특정 자료함 검색
    let res1 = db.item_search(Some(b1), "보고서").unwrap();
    assert_eq!(res1.len(), 1);
    assert_eq!(res1[0].display_name, "2026년 업무보고서.hwpx");

    // 2. 전체 자료함 검색
    let res2 = db.item_search(None, "보고서").unwrap();
    assert_eq!(res2.len(), 2);

    // 3. 대소문자 무시 검색
    let res3 = db.item_search(Some(b1), "HWPX").unwrap();
    assert_eq!(res3.len(), 1);
}

#[test]
fn test_physical_folder_export() {
    let ctx = TestContext::new("export");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("물리전환시험", "").unwrap();

    // 원본 파일 준비
    let original_file = ctx.dir.join("원본문서.txt");
    fs::write(&original_file, "원본 불변 데이터").unwrap();

    // core 공간에 파일 추가
    db.item_add(b1, "file", original_file.to_str().unwrap(), "원본문서.txt", "core", 0.0, 0.0, None).unwrap();
    // core 공간에 중복 이름 파일 하나 더 추가 (충돌 회피 검사)
    db.item_add(b1, "file", original_file.to_str().unwrap(), "원본문서.txt", "core", 10.0, 10.0, None).unwrap();
    // basis 공간에 글 추가
    db.item_add(b1, "text", "", "기초메모", "basis", 0.0, 0.0, Some("기초 지식 정리")).unwrap();
    // reference 공간에 링크 추가
    db.item_add(b1, "link", "https://gov.kr", "전자정부", "reference", 0.0, 0.0, None).unwrap();

    // 물리 폴더로 전환
    let export_dir = ctx.dir.join("내보내기폴더");
    let count = db.box_export(b1, export_dir.to_str().unwrap()).unwrap();
    assert_eq!(count, 4);

    // 원본 파일 보존 확인
    assert_eq!(fs::read_to_string(&original_file).unwrap(), "원본 불변 데이터");

    // 공간별 하위 폴더 존재 확인
    assert!(export_dir.join("1_핵심자료").is_dir());
    assert!(export_dir.join("2_기초자료").is_dir());
    assert!(export_dir.join("3_참고자료").is_dir());
    assert!(export_dir.join("4_임시저장소").is_dir());

    // 1_핵심자료에 충돌 회피 파일 확인 (원본문서.txt 및 원본문서 (1).txt)
    assert!(export_dir.join("1_핵심자료").join("원본문서.txt").is_file());
    assert!(export_dir.join("1_핵심자료").join("원본문서 (1).txt").is_file());

    // 2_기초자료에 텍스트 파일 생성 확인
    assert!(export_dir.join("2_기초자료").join("기초메모.txt").is_file());
    assert_eq!(fs::read_to_string(export_dir.join("2_기초자료").join("기초메모.txt")).unwrap(), "기초 지식 정리");

    // 3_참고자료에 URL 바로가기 파일 생성 확인
    let url_file = export_dir.join("3_참고자료").join("전자정부.url");
    assert!(url_file.is_file());
    let url_content = fs::read_to_string(url_file).unwrap();
    assert!(url_content.contains("URL=https://gov.kr"));

    // DB에 export_path 기록 확인
    let box_info = db.box_get(b1).unwrap().unwrap().0;
    assert_eq!(box_info.export_path, Some(export_dir.to_str().unwrap().to_string()));
}

#[cfg(windows)]
#[test]
fn test_physical_folder_export_reparse_point_rolls_back() {
    use std::os::windows::fs::symlink_file;

    let ctx = TestContext::new("export_reparse");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("링크 포함 폴더 전환", "").unwrap();

    let source_dir = ctx.dir.join("원본폴더");
    fs::create_dir_all(&source_dir).unwrap();
    let source_file = source_dir.join("정상파일.txt");
    fs::write(&source_file, "원본 내용").unwrap();
    let link_target = ctx.dir.join("링크대상.txt");
    fs::write(&link_target, "링크 대상").unwrap();
    symlink_file(&link_target, source_dir.join("하위링크.txt")).unwrap();

    assert!(db.item_add(b1, "folder", source_dir.to_str().unwrap(), "원본폴더", "core", 0.0, 0.0, None).is_err());
    assert!(db.item_search(Some(b1), "").unwrap().is_empty());
    fs::remove_file(source_dir.join("하위링크.txt")).unwrap();
    let iid = db.item_add(b1, "folder", source_dir.to_str().unwrap(), "원본폴더", "core", 0.0, 0.0, None).unwrap();
    let managed = db.item_get(iid).unwrap().unwrap().managed_path;
    symlink_file(&link_target, Path::new(&managed).join("하위링크.txt")).unwrap();

    let export_dir = ctx.dir.join("불완전전환");
    let error = db.box_export(b1, export_dir.to_str().unwrap()).unwrap_err();
    assert!(error.contains("폴더 복사 실패"));
    assert!(error.contains("심볼릭 링크나 정션"));
    assert!(!export_dir.exists(), "실패한 내보내기 산출물은 롤백되어야 합니다");

    let box_info = db.box_get(b1).unwrap().unwrap().0;
    assert_eq!(box_info.export_path, None, "실패한 전환 경로를 DB에 기록하면 안 됩니다");
    assert_eq!(fs::read_to_string(source_file).unwrap(), "원본 내용");
    assert_eq!(fs::read_to_string(link_target).unwrap(), "링크 대상");
}

#[test]
fn test_pipe_ipc_communication() {
    let ctx = TestContext::new("pipe_ipc");
    let db = Arc::new(Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));

    // 1. ping
    let resp1 = pipe::handle_request(r#"{"cmd":"ping"}"#, &db, &running);
    let val1 = json::parse(&resp1).unwrap();
    assert_eq!(val1.bool("ok"), Some(true));
    assert_eq!(val1.str("message"), Some("pong"));

    // 2. box_create
    let resp2 = pipe::handle_request(r#"{"cmd":"box_create","name":"파이프자료함","description":"설명"}"#, &db, &running);
    let val2 = json::parse(&resp2).unwrap();
    assert_eq!(val2.bool("ok"), Some(true));
    let bid = val2.i64("id").unwrap();

    // 3. item_add
    let resp3 = pipe::handle_request(
        &format!(r#"{{"cmd":"item_add","box_id":{bid},"kind":"text","display_name":"기록","space":"temp","content":"내용"}}"#),
        &db,
        &running,
    );
    let val3 = json::parse(&resp3).unwrap();
    assert_eq!(val3.bool("ok"), Some(true));
    let iid = val3.i64("id").unwrap();

    // 4. item_move
    let resp4 = pipe::handle_request(
        &format!(r#"{{"cmd":"item_move","id":{iid},"space":"core","x":100.0,"y":200.0}}"#),
        &db,
        &running,
    );
    let val4 = json::parse(&resp4).unwrap();
    assert_eq!(val4.bool("ok"), Some(true));

    // 5. item_search
    let resp5 = pipe::handle_request(
        &format!(r#"{{"cmd":"item_search","box_id":{bid},"query":"기록"}}"#),
        &db,
        &running,
    );
    let val5 = json::parse(&resp5).unwrap();
    assert_eq!(val5.bool("ok"), Some(true));
    let items = val5.arr("items").unwrap();
    assert_eq!(items.len(), 1);

    // 6. quit
    let resp6 = pipe::handle_request(r#"{"cmd":"quit"}"#, &db, &running);
    let val6 = json::parse(&resp6).unwrap();
    assert_eq!(val6.bool("ok"), Some(true));
    assert!(!running.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn test_reparse_point_rejection() {
    let ctx = TestContext::new("reparse");
    let regular_file = ctx.dir.join("normal.txt");
    fs::write(&regular_file, "normal").unwrap();
    assert!(!util::is_reparse_point(&regular_file));
    assert!(!util::is_or_has_reparse_point(&regular_file));
}

#[test]
fn test_json_strictness_and_surrogates() {
    assert!(json::parse("+1").is_none());
    assert!(json::parse("01").is_none());
    assert!(json::parse("1.").is_none());
    assert!(json::parse("1e").is_none());
    assert!(json::parse("x").is_none());
    assert_eq!(json::parse("0").unwrap().num("").unwrap_or(0.0), 0.0);
    assert!(matches!(json::parse("-12.34").unwrap(), json::Value::Num(n) if n == -12.34));

    assert!(json::parse(r#""abc"#).is_none());
    assert!(json::parse("\"\x01\"").is_none());

    let parsed_emoji = json::parse(r#""\uD83D\uDE00""#).unwrap();
    match parsed_emoji {
        Value::Str(s) => assert_eq!(s, "😀"),
        _ => panic!("문자열이어야 함"),
    }

    assert!(json::parse(r#""\uD83D""#).is_none());
    assert!(json::parse(r#""\uDE00""#).is_none());
}

#[test]
fn test_item_add_nonexistent_box_rejection() {
    let ctx = TestContext::new("nonexistent_box");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let res = db.item_add(99999, "text", "", "메모", "core", 0.0, 0.0, Some("내용"));
    assert!(res.is_err());
}

#[test]
fn test_path_traversal_prevention() {
    let ctx = TestContext::new("path_traversal");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("경로탈출방지", "").unwrap();

    // display_name에 상위 경로 탈출(../ 또는 ..\..) 주입
    let evil_name = "../../escape_attempt.txt";
    db.item_add(b1, "text", "", evil_name, "core", 0.0, 0.0, Some("탈출 시도")).unwrap();

    let export_dir = ctx.dir.join("export_safe");
    let count = db.box_export(b1, export_dir.to_str().unwrap()).unwrap();
    assert_eq!(count, 1);

    // export_dir 바로 아래나 상위 경로에 생성되지 않고, 반드시 1_핵심자료 안에 안전하게 정규화되어 생성되어야 함
    assert!(!export_dir.join("escape_attempt.txt").exists());
    assert!(!ctx.dir.join("escape_attempt.txt").exists());
    let core_files: Vec<_> = fs::read_dir(export_dir.join("1_핵심자료")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(core_files.len(), 1);
    assert!(!core_files[0].contains('/'));
    assert!(!core_files[0].contains('\\'));
    assert!(core_files[0].ends_with(".txt"));
}

#[test]
fn test_export_non_empty_rejection_and_rollback() {
    let ctx = TestContext::new("export_rollback");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("전환검증자료함", "").unwrap();

    let sample = ctx.dir.join("valid.txt");
    fs::write(&sample, "정상내용").unwrap();
    db.item_add(b1, "file", sample.to_str().unwrap(), "valid.txt", "core", 0.0, 0.0, None).unwrap();

    // 1. 이미 파일이 들어있는 비어있지 않은 폴더로 내보내기 시도 -> 거부 확인
    let non_empty_dir = ctx.dir.join("existing_folder");
    fs::create_dir_all(&non_empty_dir).unwrap();
    fs::write(non_empty_dir.join("existing.txt"), "기존데이터").unwrap();

    let err = db.box_export(b1, non_empty_dir.to_str().unwrap());
    assert!(err.is_err());
    assert_eq!(fs::read_to_string(non_empty_dir.join("existing.txt")).unwrap(), "기존데이터");
    assert!(!non_empty_dir.join("1_핵심자료").exists());

    // 2. 존재하지 않던 신규 폴더로 내보내기 시 원자적 롤백 시험
    // 유효하지 않은 원본 경로를 가진 아이템을 강제로 DB에 넣어 내보내기 실패 유발
    let bad_b = db.box_create("오류자료함", "").unwrap();
    let phantom_file = ctx.dir.join("phantom.txt");
    // 원본은 보존하고 관리 사본만 없애 내보내기 시 사본 누락 에러 유발.
    fs::write(&phantom_file, "삭제예정").unwrap();
    let iid = db.item_add(bad_b, "file", phantom_file.to_str().unwrap(), "phantom.txt", "core", 0.0, 0.0, None).unwrap();
    fs::remove_file(db.item_get(iid).unwrap().unwrap().managed_path).unwrap();
    assert_eq!(fs::read_to_string(&phantom_file).unwrap(), "삭제예정");

    let target_fail = ctx.dir.join("fail_export_dir");
    let fail_res = db.box_export(bad_b, target_fail.to_str().unwrap());
    assert!(fail_res.is_err());
    // 실패 시 롤백되어 target_fail 폴더 자체가 남아있지 않아야 함
    assert!(!target_fail.exists());
}

#[test]
fn test_export_folder_reparse_point_rejection_and_rollback() {
    let ctx = TestContext::new("export_reparse");
    let db = Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let b1 = db.box_create("폴더정션검증", "").unwrap();

    let src_dir = ctx.dir.join("parent_folder");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("normal.txt"), "일반자료").unwrap();

    let target_file = ctx.dir.join("target.txt");
    fs::write(&target_file, "대상내용").unwrap();
    let link_child = src_dir.join("link_child.txt");

    #[cfg(windows)]
    let link_created = std::os::windows::fs::symlink_file(&target_file, &link_child).is_ok();
    #[cfg(not(windows))]
    let link_created = std::os::unix::fs::symlink(&target_file, &link_child).is_ok();

    if link_created {
        assert!(util::is_or_has_reparse_point(&link_child));
        assert!(db.item_add(b1, "folder", src_dir.to_str().unwrap(), "parent_folder", "core", 0.0, 0.0, None).is_err());
        assert!(db.item_search(Some(b1), "").unwrap().is_empty());
        fs::remove_file(&link_child).unwrap();
        let iid = db.item_add(b1, "folder", src_dir.to_str().unwrap(), "parent_folder", "core", 0.0, 0.0, None).unwrap();
        let managed = db.item_get(iid).unwrap().unwrap().managed_path;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&target_file, Path::new(&managed).join("link_child.txt")).unwrap();
        #[cfg(not(windows))]
        std::os::unix::fs::symlink(&target_file, Path::new(&managed).join("link_child.txt")).unwrap();

        let export_dir = ctx.dir.join("export_reparse_output");
        let res = db.box_export(b1, export_dir.to_str().unwrap());
        // 하위에 reparse point가 하나라도 있으면 전환 실패 및 롤백 확인
        assert!(res.is_err());
        assert!(!export_dir.exists());
    }
}

#[test]
fn test_copy_dir_recursive_reparse_point_rejection() {
    let ctx = TestContext::new("copy_dir_reparse");
    let src_dir = ctx.dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let target = ctx.dir.join("target.txt");
    fs::write(&target, "target").unwrap();
    let symlink = src_dir.join("link.txt");

    #[cfg(windows)]
    let link_created = std::os::windows::fs::symlink_file(&target, &symlink).is_ok();
    #[cfg(not(windows))]
    let link_created = std::os::unix::fs::symlink(&target, &symlink).is_ok();

    if link_created {
        let dest = ctx.dir.join("dst");
        let res = db::copy_dir_recursive(&src_dir, &dest);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }
}

#[test]
fn test_nb_source_add_structured_and_python_paragraphs() {
    let ctx = TestContext::new("nb_source_add");
    let db = Arc::new(Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let nb = call(r#"{"cmd":"nb_create","name":"노트"}"#.into()).i64("id").unwrap();

    // klib 직접 추출: md 제목 수준과 기호 수준이 문단에 남는다.
    let md = ctx.dir.join("계획.md");
    fs::write(&md, "# 추진 계획\n□ 예산 현황\n본문\n").unwrap();
    let md_path = md.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/");
    let added = call(format!(r#"{{"cmd":"nb_source_add","notebook_id":{nb},"source_path":"{md_path}"}}"#));
    assert_eq!(added.bool("ok"), Some(true), "{added:?}");
    assert_eq!(added.i64("paragraph_count"), Some(3));
    let outline = call(format!(r#"{{"cmd":"nb_transform","notebook_id":{nb},"type":"outline"}}"#));
    let rows = outline.arr("results").unwrap();
    assert_eq!(rows.iter().map(|r| (r.str("text").unwrap(), r.i64("level").unwrap())).collect::<Vec<_>>(), vec![("추진 계획", 1), ("□ 예산 현황", 3)]);

    // 구형 hwp·pdf는 klib가 직접 읽지 않는다.
    let pdf = ctx.dir.join("자료.pdf");
    fs::write(&pdf, b"%PDF-1.4").unwrap();
    let pdf_path = pdf.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/");
    let refused = call(format!(r#"{{"cmd":"nb_source_add","notebook_id":{nb},"source_path":"{pdf_path}"}}"#));
    assert_eq!(refused.bool("ok"), Some(false));

    // Python 추출 문단을 첫 묶음으로 받고, 나머지 묶음은 nb_index로 덧붙인다.
    let first = call(format!(r#"{{"cmd":"nb_source_add","notebook_id":{nb},"source_path":"{pdf_path}","paragraphs":[{{"text":"Ⅰ. 개요","level":1,"location":"1쪽"}},{{"text":"구분\t금액","table":true}}]}}"#));
    assert_eq!(first.bool("ok"), Some(true), "{first:?}");
    let sid = first.i64("id").unwrap();
    let more = call(format!(r#"{{"cmd":"nb_index","notebook_id":{nb},"source_id":{sid},"paragraphs":[{{"text":"예산\t1,250만원","kind":"table"}}]}}"#));
    assert_eq!(more.i64("paragraph_count"), Some(3), "{more:?}");
    let ps = call(format!(r#"{{"cmd":"nb_source_paragraphs","notebook_id":{nb},"source_id":{sid}}}"#));
    let ps = ps.arr("paragraphs").unwrap();
    assert_eq!(ps.iter().map(|p| p.i64("ordinal").unwrap()).collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(ps[0].str("location"), Some("1쪽"));
    let tables = call(format!(r#"{{"cmd":"nb_transform","notebook_id":{nb},"type":"tables","source_ids":[{sid}]}}"#));
    assert_eq!(tables.arr("results").unwrap().iter().map(|r| r.str("text").unwrap()).collect::<Vec<_>>(), vec!["구분\t금액", "예산\t1,250만원"]);
    let hits = call(format!(r#"{{"cmd":"nb_search","notebook_id":{nb},"query":"1,250만원","source_ids":[{sid}]}}"#));
    assert!(!hits.arr("results").unwrap().is_empty(), "{hits:?}");
}

#[test]
fn test_nb_note_tags_compare_and_export() {
    let ctx = TestContext::new("nb_tags_compare");
    let db = Arc::new(Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let nb = call(r#"{"cmd":"nb_create","name":"비교"}"#.into()).i64("id").unwrap();
    let add = |name: &str, text: &str| {
        let path = ctx.dir.join(name);
        fs::write(&path, text).unwrap();
        let path = path.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/");
        let added = call(format!(r#"{{"cmd":"nb_source_add","notebook_id":{nb},"source_path":"{path}"}}"#));
        assert_eq!(added.bool("ok"), Some(true), "{added:?}");
        added.i64("id").unwrap()
    };
    let a = add("가.txt", "복지 예산은 120억 원이다\n도로 정비\n복지 예산 복지 예산 증가\n");
    let b = add("나.txt", "복지 지원 확대\n");
    let c = add("다.txt", "무관한 내용\n");

    // 출처별 비교: 출처마다 따로 찾고, 근거가 없는 출처도 빈 칸으로 낸다.
    let cmp = call(format!(r#"{{"cmd":"nb_compare","notebook_id":{nb},"query":"복지 예산","per_source":2}}"#));
    assert_eq!(cmp.bool("ok"), Some(true), "{cmp:?}");
    let groups = cmp.arr("groups").unwrap();
    assert_eq!(groups.iter().map(|g| g.i64("source_id").unwrap()).collect::<Vec<_>>(), vec![a, b, c]);
    assert_eq!(groups[0].arr("hits").unwrap().len(), 2);
    assert!(groups[0].arr("hits").unwrap().iter().all(|h| h.i64("source_id") == Some(a)));
    assert_eq!(groups[1].arr("hits").unwrap()[0].i64("ordinal"), Some(0));
    assert!(groups[2].arr("hits").unwrap().is_empty());
    let scoped = call(format!(r#"{{"cmd":"nb_compare","notebook_id":{nb},"query":"복지","source_ids":[{b}]}}"#));
    assert_eq!(scoped.arr("groups").unwrap().len(), 1);
    assert_eq!(call(format!(r#"{{"cmd":"nb_compare","notebook_id":{nb},"query":"  "}}"#)).bool("ok"), Some(false));
    assert_eq!(call(format!(r#"{{"cmd":"nb_compare","notebook_id":{nb},"query":"복지","source_ids":["x"]}}"#)).bool("ok"), Some(false));

    // 태그 저장·거르기·태그 목록.
    let saved = call(format!(r##"{{"cmd":"nb_note_save","notebook_id":{nb},"title":"메모","body":"본문","tags":["#예산"," 복지 정책 ","예산"],"citations":[{{"source_id":{a},"paragraph_ordinal":0,"quote":"복지 예산은 120억 원이다"}}]}}"##));
    assert_eq!(saved.bool("ok"), Some(true), "{saved:?}");
    let note = saved.i64("id").unwrap();
    assert!(matches!(saved.arr("tags").unwrap(), [Value::Str(x), Value::Str(y)] if x == "예산" && y == "복지_정책"), "{saved:?}");
    call(format!(r#"{{"cmd":"nb_note_save","notebook_id":{nb},"title":"다른","body":"","tags":["도로"]}}"#));
    let all = call(format!(r#"{{"cmd":"nb_note_list","notebook_id":{nb}}}"#));
    assert_eq!(all.arr("notes").unwrap().len(), 2);
    assert_eq!(all.arr("tags").unwrap().iter().map(|t| t.str("tag").unwrap()).collect::<Vec<_>>(), vec!["도로", "복지_정책", "예산"]);
    let filtered = call(format!(r#"{{"cmd":"nb_note_list","notebook_id":{nb},"tag":"예산"}}"#));
    let notes = filtered.arr("notes").unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].i64("id"), Some(note));
    assert_eq!(notes[0].arr("tags").unwrap().len(), 2);
    assert_eq!(notes[0].arr("citations").unwrap().len(), 1);
    // tags 없이 저장하면 태그를 그대로 둔다.
    call(format!(r#"{{"cmd":"nb_note_save","notebook_id":{nb},"id":{note},"title":"메모","body":"고친 본문","citations":[]}}"#));
    assert_eq!(call(format!(r#"{{"cmd":"nb_note_list","notebook_id":{nb},"tag":"복지_정책"}}"#)).arr("notes").unwrap().len(), 1);
    assert_eq!(call(format!(r#"{{"cmd":"nb_note_save","notebook_id":{nb},"title":"x","tags":[1]}}"#)).bool("ok"), Some(false));

    // 내보내기: 태그 줄과 출처 비교 절.
    let plain = call(format!(r#"{{"cmd":"nb_export_md","notebook_id":{nb},"note_id":{note}}}"#));
    let plain = plain.str("markdown").unwrap();
    assert!(plain.starts_with("# 메모\n\n태그: #예산 #복지_정책\n\n고친 본문"), "{plain}");
    assert!(!plain.contains("출처 비교"));
    let md = call(format!(r#"{{"cmd":"nb_export_md","notebook_id":{nb},"note_id":{note},"compare_query":"복지 예산","compare_source_ids":[{a},{c}],"per_source":1}}"#));
    let md = md.str("markdown").unwrap();
    assert!(md.contains("## 출처 비교: 복지 예산"), "{md}");
    assert!(md.contains("### 가.txt"));
    assert!(!md.contains("### 나.txt"));
    assert!(md.contains("### 다.txt"));
    assert!(md.contains("근거 문단 없음"));
}

#[test]
fn test_box_scan_pipe_commands_incremental_progress_cancel() {
    let ctx = TestContext::new("box_scan");
    let db = Arc::new(Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let wait_done = || {
        for _ in 0..400 {
            let s = call(r#"{"cmd":"box_scan_status"}"#.into());
            if s.str("state") != Some("scanning") { return s; }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("스캔이 끝나지 않음");
    };
    let b = call(r#"{"cmd":"box_create","name":"스캔"}"#.into()).i64("id").unwrap();

    assert_eq!(call(r#"{"cmd":"box_scan_status"}"#.into()).str("state"), Some("idle"));
    assert_eq!(call(r#"{"cmd":"box_scan_cancel"}"#.into()).bool("cancelling"), Some(false));

    let src = ctx.dir.join("폴더");
    fs::create_dir_all(src.join("하위")).unwrap();
    for i in 0..4 { fs::write(src.join(format!("f{i}.txt")), "x").unwrap(); }
    fs::write(src.join("하위").join("g.txt"), "y").unwrap();
    // 상위 폴더에 링크가 있으면(macOS /var) 스캔을 거부하므로 실제 경로로 바꾼다. Windows는 \\?\ 접두가 붙어 그대로 둔다.
    let src = if cfg!(windows) { src } else { fs::canonicalize(&src).unwrap() };
    let p = src.to_str().unwrap().replace('\\', "/");

    assert_eq!(call(format!(r#"{{"cmd":"box_scan","box_id":999,"path":"{p}"}}"#)).bool("ok"), Some(false));
    assert_eq!(call(format!(r#"{{"cmd":"box_scan","box_id":{b},"path":"{p}/없는폴더"}}"#)).bool("ok"), Some(false));

    let started = call(format!(r#"{{"cmd":"box_scan","box_id":{b},"path":"{p}"}}"#));
    assert_eq!(started.bool("started"), Some(true), "{:?}", started.str("message"));
    let s = wait_done();
    assert_eq!(s.str("state"), Some("done"));
    assert_eq!((s.i64("total"), s.i64("processed"), s.i64("added"), s.i64("unchanged")), (Some(5), Some(5), Some(5), Some(0)));

    call(format!(r#"{{"cmd":"box_scan","box_id":{b},"path":"{p}"}}"#));
    let s = wait_done();
    assert_eq!((s.i64("added"), s.i64("updated"), s.i64("unchanged"), s.i64("removed")), (Some(0), Some(0), Some(5), Some(0)));

    fs::remove_file(src.join("f0.txt")).unwrap();
    call(format!(r#"{{"cmd":"box_scan","box_id":{b},"path":"{p}"}}"#));
    let s = wait_done();
    assert_eq!((s.i64("unchanged"), s.i64("removed")), (Some(4), Some(1)));

    // 확장자 필터와 결과 목록 조회(box_scan_files)
    fs::write(src.join("a.HWPX"), "1").unwrap();
    fs::write(src.join("b.exe"), "2").unwrap();
    let ext_started = call(format!(r#"{{"cmd":"box_scan","box_id":{b},"path":"{p}","extensions":["hwpx",".TXT"]}}"#));
    assert_eq!(ext_started.bool("started"), Some(true));
    let s = wait_done();
    assert_eq!(s.i64("total"), Some(5), "txt 4개(f1~f3, 하위/g)+hwpx 1개, exe 제외");
    let listed = call(format!(r#"{{"cmd":"box_scan_files","box_id":{b},"path":"{p}","limit":100}}"#));
    assert_eq!(listed.bool("ok"), Some(true));
    let files = listed.arr("files").unwrap();
    assert_eq!(files.len(), 5);
    assert!(files.iter().all(|f| !f.str("path").unwrap().ends_with(".exe")));
    let page = call(format!(r#"{{"cmd":"box_scan_files","box_id":{b},"path":"{p}","offset":4,"limit":100}}"#));
    assert_eq!(page.arr("files").unwrap().len(), 1);
    assert_eq!(call(r#"{"cmd":"box_scan_files"}"#.into()).bool("ok"), Some(false));
}

#[test]
fn test_nb_source_from_item_reuse_and_back_link() {
    let ctx = TestContext::new("nb_item_link");
    let db = Arc::new(Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let nb = call(r#"{"cmd":"nb_create","name":"노트"}"#.into()).i64("id").unwrap();
    let md = ctx.dir.join("계획.md");
    fs::write(&md, "# 추진 계획\n본문\n").unwrap();
    let b = db.lock().unwrap().box_create("자료함", "").unwrap();
    // 원본 경로만 있던 기존 항목과 그 본문 색인을 재현한다.
    let item = {
        let guard = db.lock().unwrap();
        guard.connection().execute("INSERT INTO item(box_id,kind,source_path,display_name,space,x,y,created_at) VALUES(?1,'file',?2,'계획.md','basis',0,0,0)", rusqlite::params![b, md.to_str().unwrap()]).unwrap();
        guard.connection().last_insert_rowid()
    };
    content::index_file(search_index().unwrap(), md.to_str().unwrap(), "계획.md", b).unwrap();
    assert!(search_index().unwrap().search_box(b, "추진", 20).unwrap().iter().any(|hit| hit.path == md.to_str().unwrap()));

    // 처음 추가하면 새 출처, 다시 추가하면 같은 출처를 재사용한다.
    let first = call(format!(r#"{{"cmd":"nb_source_from_item","notebook_id":{nb},"item_id":{item}}}"#));
    assert_eq!(first.bool("ok"), Some(true), "{first:?}");
    assert_eq!(first.bool("reused"), Some(false));
    let sid = first.i64("id").unwrap();
    let migrated = db.lock().unwrap().item_get(item).unwrap().unwrap();
    assert!(!migrated.managed_path.is_empty());
    let hits = search_index().unwrap().search_box(b, "추진", 20).unwrap();
    assert!(hits.iter().any(|hit| hit.path == migrated.managed_path));
    assert!(hits.iter().all(|hit| hit.path != md.to_str().unwrap()));
    assert_eq!(fs::read_to_string(&md).unwrap(), "# 추진 계획\n본문\n");
    let again = call(format!(r#"{{"cmd":"nb_source_from_item","notebook_id":{nb},"item_id":{item}}}"#));
    assert_eq!(again.i64("id"), Some(sid));
    assert_eq!(again.bool("reused"), Some(true));
    let list = call(format!(r#"{{"cmd":"nb_source_list","notebook_id":{nb}}}"#));
    assert_eq!(list.arr("sources").unwrap().len(), 1);
    assert_eq!(list.arr("sources").unwrap()[0].i64("item_id"), Some(item));

    // 출처 → 원본 항목 조회.
    let back = call(format!(r#"{{"cmd":"nb_source_item","notebook_id":{nb},"source_id":{sid}}}"#));
    assert_eq!(back.bool("found"), Some(true), "{back:?}");
    assert_eq!(back.i64("item_id"), Some(item));
    assert_eq!(back.i64("box_id"), Some(b));

    // 직접 추가한 출처는 연결이 없고, 항목을 지우면 found:false, 없는 항목·노트북은 오류.
    let md2 = ctx.dir.join("직접.md");
    fs::write(&md2, "본문\n").unwrap();
    let p2 = md2.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/");
    let direct = call(format!(r#"{{"cmd":"nb_source_add","notebook_id":{nb},"source_path":"{p2}"}}"#)).i64("id").unwrap();
    let none = call(format!(r#"{{"cmd":"nb_source_item","notebook_id":{nb},"source_id":{direct}}}"#));
    assert_eq!(none.bool("found"), Some(false));
    call(format!(r#"{{"cmd":"item_delete","id":{item}}}"#));
    let gone = call(format!(r#"{{"cmd":"nb_source_item","notebook_id":{nb},"source_id":{sid}}}"#));
    assert_eq!(gone.bool("found"), Some(false));
    assert_eq!(call(format!(r#"{{"cmd":"nb_source_from_item","notebook_id":{nb},"item_id":99999}}"#)).bool("ok"), Some(false));
    assert_eq!(call(format!(r#"{{"cmd":"nb_source_from_item","notebook_id":99999,"item_id":1}}"#)).bool("ok"), Some(false));
}

#[test]
fn test_nb_source_item_column_migration_on_old_db() {
    let ctx = TestContext::new("nb_item_migrate");
    let path = ctx.dir.join("library.db");
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        drop(db);
    }
    // item_id 열이 없는 옛 nb_source 표를 흉내 낸 뒤 다시 열어도 열이 생긴다(업무자료함 표는 별도 DB, NBIDX1).
    let c = rusqlite::Connection::open(ctx.dir.join("notebook").join("notebook.db")).unwrap();
    c.execute_batch("DROP INDEX IF EXISTS idx_nb_source_item; ALTER TABLE nb_source DROP COLUMN item_id;").unwrap();
    drop(c);
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let has: bool = db.connection().query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('nb_source') WHERE name='item_id')", [], |r| r.get(0)).unwrap();
    assert!(has);
}

/// 메모 편집기(edit_*) 명령이 파이프 처리기에서 editor_core로 위임되는지: Flutter KlibEditorCore가 보내는 순서 그대로.
#[test]
fn test_edit_commands_delegate_to_editor_core() {
    let ctx = TestContext::new("edit_cmds");
    let db = Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap());
    let running = AtomicBool::new(true);
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let ok = |v: &json::Value| assert_eq!(v.bool("ok"), Some(true), "응답 실패: {:?}", v.str("message"));

    let pong = call(r#"{"cmd":"edit_ping"}"#.into());
    ok(&pong);
    assert_eq!(pong.str("message"), Some("pong"));

    let opened = call(r#"{"cmd":"edit_open","text":"가나\r\n다"}"#.into());
    ok(&opened);
    let doc = opened.i64("doc").unwrap();
    assert_eq!(opened.str("line_ending"), Some("CRLF"));
    assert_eq!(opened.str("encoding"), Some("UTF-8"));
    assert_eq!(opened.i64("lines"), Some(2));

    // 한글 조합: 같은 구간 교체가 한 번의 실행취소 묶음이 된다("가나\n다" 끝 = 10바이트).
    for (s, e, t) in [(10, 10, "ㅎ"), (10, 13, "하"), (10, 13, "한")] {
        ok(&call(format!(r#"{{"cmd":"edit_replace","doc":{doc},"start":{s},"end":{e},"text":"{t}"}}"#)));
    }
    let text = call(format!(r#"{{"cmd":"edit_text","doc":{doc}}}"#));
    assert_eq!(text.str("text"), Some("가나\n다한"));
    let undo = call(format!(r#"{{"cmd":"edit_undo","doc":{doc}}}"#));
    ok(&undo);
    assert_eq!(undo.bool("changed"), Some(true));
    assert_eq!(call(format!(r#"{{"cmd":"edit_text","doc":{doc}}}"#)).str("text"), Some("가나\n다"));
    let redo = call(format!(r#"{{"cmd":"edit_redo","doc":{doc}}}"#));
    assert_eq!(redo.i64("cursor"), Some(13));

    // 찾기·모두 바꾸기·서식·상태값·줄 조회.
    let found = call(format!(r#"{{"cmd":"edit_find","doc":{doc},"query":"다","from":0}}"#));
    assert_eq!((found.bool("found"), found.i64("start"), found.i64("end")), (Some(true), Some(7), Some(10)));
    let all = call(format!(r#"{{"cmd":"edit_replace_all","doc":{doc},"query":"가","with":"카"}}"#));
    assert_eq!(all.i64("count"), Some(1));
    let bold = call(format!(r#"{{"cmd":"edit_md_inline","doc":{doc},"start":0,"end":6,"kind":"bold"}}"#));
    ok(&bold);
    assert_eq!(call(format!(r#"{{"cmd":"edit_lines","doc":{doc},"from":0,"to":1}}"#)).arr("lines").unwrap()[0].str("text"), Some("**카나**"));
    let lc = call(format!(r#"{{"cmd":"edit_line_col","doc":{doc},"pos":5}}"#));
    assert_eq!((lc.i64("line"), lc.i64("col")), (Some(1), Some(4)));
    let info = call(format!(r#"{{"cmd":"edit_info","doc":{doc}}}"#));
    assert_eq!(info.i64("chars"), Some(9));
    assert_eq!(info.bool("dirty"), Some(true));

    // 저장(원래 CRLF로) 후 닫기. 닫은 문서는 오류.
    let path = ctx.dir.join("memo.md");
    let p = path.to_str().unwrap().replace('\\', "/");
    ok(&call(format!(r#"{{"cmd":"edit_save","doc":{doc},"path":"{p}"}}"#)));
    assert_eq!(fs::read(&path).unwrap(), "**카나**\r\n다한".as_bytes());
    ok(&call(format!(r#"{{"cmd":"edit_close","doc":{doc}}}"#)));
    let gone = call(format!(r#"{{"cmd":"edit_info","doc":{doc}}}"#));
    assert_eq!(gone.bool("ok"), Some(false));
    assert!(gone.str("message").unwrap().contains("찾을 수 없습니다"));

    // 알 수 없는 edit_ 명령은 코어의 오류로, 다른 명령은 기존 처리 그대로.
    assert_eq!(call(r#"{"cmd":"edit_nope"}"#.into()).bool("ok"), Some(false));
    assert_eq!(call(r#"{"cmd":"ping"}"#.into()).str("message"), Some("pong"));
}

#[test]
fn test_item_text_get_set_for_memo_editor() {
    let ctx = TestContext::new("item_text");
    let db = Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap());
    let running = AtomicBool::new(true);
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let ok = |v: &json::Value| assert_eq!(v.bool("ok"), Some(true), "응답 실패: {:?}", v.str("message"));

    let bid = call(r#"{"cmd":"box_create","name":"메모함"}"#.into()).i64("id").unwrap();
    let added = call(format!(r#"{{"cmd":"item_add","box_id":{bid},"kind":"text","display_name":"회의","space":"core","content":"첫 줄\n둘째 줄"}}"#));
    ok(&added);
    let iid = added.i64("id").unwrap();

    // 읽기: 사본 내용과 이름이 온다.
    let got = call(format!(r#"{{"cmd":"item_text_get","id":{iid}}}"#));
    ok(&got);
    assert_eq!(got.str("content"), Some("첫 줄\n둘째 줄"));
    assert_eq!(got.str("display_name"), Some("회의"));
    assert_eq!(got.str("kind"), Some("text"));

    // 고치기: 내용과 이름을 바꾸고, 사본 경로는 그대로, 임시 파일은 남지 않는다.
    let before = db.lock().unwrap().item_get(iid).unwrap().unwrap().source_path;
    let set = call(format!(r###"{{"cmd":"item_text_set","id":{iid},"display_name":"회의 정리","content":"# 결론\n**예산** 확정"}}"###));
    ok(&set);
    assert_eq!(set.str("display_name"), Some("회의 정리"));
    let row = db.lock().unwrap().item_get(iid).unwrap().unwrap();
    assert_eq!(row.source_path, before);
    assert_eq!(row.display_name, "회의 정리");
    assert_eq!(fs::read_to_string(&row.source_path).unwrap(), "# 결론\n**예산** 확정");
    let dir = Path::new(&row.source_path).parent().unwrap();
    assert_eq!(fs::read_dir(dir).unwrap().count(), 1, "임시 파일이 남았습니다");
    assert_eq!(call(format!(r#"{{"cmd":"item_text_get","id":{iid}}}"#)).str("content"), Some("# 결론\n**예산** 확정"));

    // 이름을 비우면 이름은 그대로 두고 내용만 고친다.
    ok(&call(format!(r#"{{"cmd":"item_text_set","id":{iid},"display_name":"  ","content":"다시"}}"#)));
    assert_eq!(db.lock().unwrap().item_get(iid).unwrap().unwrap().display_name, "회의 정리");

    // 거절: content 없음, 없는 항목, 파일 항목, 자료함 데이터 폴더 밖을 가리키는 글 항목.
    assert_eq!(call(format!(r#"{{"cmd":"item_text_set","id":{iid}}}"#)).bool("ok"), Some(false));
    assert_eq!(call(r#"{"cmd":"item_text_get","id":999999}"#.into()).bool("ok"), Some(false));
    let outside = ctx.dir.join("원본.txt");
    fs::write(&outside, "사용자 원본").unwrap();
    let op = outside.to_str().unwrap().replace('\\', "/");
    let file_item = call(format!(r#"{{"cmd":"item_add","box_id":{bid},"kind":"file","source_path":"{op}","display_name":"원본.txt","space":"core"}}"#));
    ok(&file_item);
    let fid = file_item.i64("id").unwrap();
    let refused = call(format!(r#"{{"cmd":"item_text_set","id":{fid},"content":"덮어쓰기"}}"#));
    assert_eq!(refused.bool("ok"), Some(false));
    assert!(refused.str("message").unwrap().contains("글 자료"));
    let nid = call(format!(r#"{{"cmd":"item_add","box_id":{bid},"kind":"note","display_name":"메모","space":"core","content":"y"}}"#)).i64("id").unwrap();
    // DB를 직접 바꿔 사본 경로를 사용자 원본으로 돌려 놓아도 편집기는 거절한다.
    let raw = rusqlite::Connection::open(ctx.dir.join("library.db")).unwrap();
    raw.execute("UPDATE item SET source_path = ?1 WHERE id = ?2", rusqlite::params![outside.to_str().unwrap(), nid]).unwrap();
    let escaped = call(format!(r#"{{"cmd":"item_text_set","id":{nid},"content":"덮어쓰기"}}"#));
    assert_eq!(escaped.bool("ok"), Some(false));
    assert!(escaped.str("message").unwrap().contains("데이터 폴더 밖"));
    assert_eq!(fs::read_to_string(&outside).unwrap(), "사용자 원본");
}

/// 3단계(EDCORE3) 명령: 큰 파일 열기 → 보이는 줄만 조회 → 줄 편집 → 변환 불가 글자 → 인코딩·줄바꿈 선택 저장.
/// Flutter 큰 글 보기(LargeTextSession)가 보내는 순서 그대로. 10만 줄 파일 열기 시간을 출력한다(--nocapture).
#[test]
fn test_edit_stage3_large_file_and_encoding_save() {
    let ctx = TestContext::new("edit_stage3");
    let db = Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap());
    let running = AtomicBool::new(true);
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let ok = |v: &json::Value| assert_eq!(v.bool("ok"), Some(true), "응답 실패: {:?}", v.str("message"));

    // 10만 줄 CP949 + CRLF 파일(한글·영문 혼합).
    let mut text = String::new();
    for i in 0..100_000 {
        text.push_str(&format!("{i:06}번째 줄 가나다라마바사 메모장 벤치마킹 line {i}\r\n"));
    }
    let bytes = editor_core::cp949::encode(&text).unwrap();
    let path = ctx.dir.join("big.txt");
    fs::write(&path, &bytes).unwrap();
    let p = path.to_str().unwrap().replace('\\', "/");

    let t0 = std::time::Instant::now();
    let opened = call(format!(r#"{{"cmd":"edit_open","path":"{p}"}}"#));
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    ok(&opened);
    let doc = opened.i64("doc").unwrap();
    assert_eq!((opened.str("encoding"), opened.str("line_ending")), (Some("CP949"), Some("CRLF")));
    assert_eq!(opened.i64("lines"), Some(100_001));
    let t1 = std::time::Instant::now();
    let first_raw = pipe::handle_request(&format!(r#"{{"cmd":"edit_line_range","doc":{doc},"start_line":0,"count":120}}"#), &db, &running);
    let first_ms = t1.elapsed().as_secs_f64() * 1000.0;
    // 응답 한 줄이 Python 브리지 한도(64 KiB) 안인지: 화면은 120줄씩 가져온다.
    assert!(first_raw.len() < 64 * 1024, "응답 {} 바이트", first_raw.len());
    let first = json::parse(&first_raw).unwrap();
    ok(&first);
    assert_eq!(first.i64("total_lines"), Some(100_001));
    assert_eq!(first.arr("lines").unwrap().len(), 120);
    let t2 = std::time::Instant::now();
    let tail = call(format!(r#"{{"cmd":"edit_line_range","doc":{doc},"start_line":99950,"count":120}}"#));
    let tail_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let rows = tail.arr("lines").unwrap();
    assert_eq!(rows.len(), 51); // 마지막 빈 줄 포함, 범위를 넘으면 잘린다
    assert!(rows[0].str("text").unwrap().starts_with("099950번째"));
    println!("[EDIT_STAGE3] 10만 줄 파일({} 바이트) 열기 {open_ms:.1} ms, 첫 화면 120줄 {first_ms:.2} ms, 끝 화면 {tail_ms:.2} ms", bytes.len());

    // 한 줄 편집(줄 50001 맨 앞에 이모지) → 상태·변환 불가 목록.
    let line = call(format!(r#"{{"cmd":"edit_line_range","doc":{doc},"start_line":50000,"count":1}}"#));
    let start = line.arr("lines").unwrap()[0].i64("start").unwrap();
    ok(&call(format!(r#"{{"cmd":"edit_replace","doc":{doc},"start":{start},"end":{start},"text":"😀"}}"#)));
    let st = call(format!(r#"{{"cmd":"edit_status","doc":{doc},"pos":{}}}"#, start + 4));
    assert_eq!((st.i64("line"), st.i64("col"), st.str("encoding")), (Some(50001), Some(2), Some("CP949")));
    let un = call(format!(r#"{{"cmd":"edit_unencodable","doc":{doc},"encoding":"CP949"}}"#));
    assert_eq!((un.bool("ok_to_save"), un.i64("total")), (Some(false), Some(1)));
    assert_eq!(un.arr("first").unwrap()[0].i64("line"), Some(50001));
    // CP949로는 거절(파일·설정 그대로), UTF-8 BOM + LF로는 저장.
    let out = ctx.dir.join("out.txt");
    let o = out.to_str().unwrap().replace('\\', "/");
    let refused = call(format!(r#"{{"cmd":"edit_save","doc":{doc},"path":"{o}","encoding":"CP949"}}"#));
    assert_eq!(refused.bool("ok"), Some(false));
    assert!(!out.exists());
    assert_eq!(call(format!(r#"{{"cmd":"edit_info","doc":{doc}}}"#)).str("encoding"), Some("CP949"));
    let saved = call(format!(r#"{{"cmd":"edit_save","doc":{doc},"path":"{o}","encoding":"UTF-8 BOM","line_ending":"LF"}}"#));
    ok(&saved);
    assert_eq!((saved.str("encoding"), saved.str("line_ending"), saved.bool("dirty")), (Some("UTF-8 BOM"), Some("LF"), Some(false)));
    let written = fs::read(&out).unwrap();
    assert_eq!(&written[..3], b"\xEF\xBB\xBF");
    assert!(!written.contains(&b'\r'));
    let lc = call(format!(r#"{{"cmd":"edit_char_to_line_col","doc":{doc},"char":0}}"#));
    assert_eq!((lc.i64("line"), lc.i64("col")), (Some(1), Some(1)));
    ok(&call(format!(r#"{{"cmd":"edit_close","doc":{doc}}}"#)));
}

#[test]
fn test_managed_area_copies_preserve_origin_move_delete_and_collision() {
    let ctx = TestContext::new("managed_copies");
    let db = Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let bid = db.box_create("복사본", "").unwrap();
    let original = ctx.dir.join("보고서.txt");
    fs::write(&original, b"original").unwrap();
    let before = fs::metadata(&original).unwrap().modified().unwrap();
    let first = db.item_add(bid, "file", original.to_str().unwrap(), "보고서.txt", "core", 0., 0., None).unwrap();
    let second = db.item_add(bid, "file", original.to_str().unwrap(), "보고서.txt", "core", 0., 0., None).unwrap();
    let one = db.item_get(first).unwrap().unwrap();
    let two = db.item_get(second).unwrap().unwrap();
    assert_eq!(one.origin_path, original.to_str().unwrap());
    assert_eq!(one.source_path, one.managed_path);
    assert_eq!(Path::new(&one.managed_path).parent().unwrap(), ctx.dir.join(bid.to_string()).join("core"));
    assert_ne!(one.managed_path, two.managed_path);
    assert!(two.managed_path.ends_with("보고서 (1).txt"));
    assert_eq!(db.item_search(Some(bid), original.to_str().unwrap()).unwrap().len(), 2);
    fs::write(&one.managed_path, b"edited copy").unwrap();
    db.item_move(first, "basis", 2., 3.).unwrap();
    let moved = db.item_get(first).unwrap().unwrap();
    assert!(!Path::new(&one.managed_path).exists());
    assert_eq!(fs::read(&moved.managed_path).unwrap(), b"edited copy");
    assert_eq!(Path::new(&moved.managed_path).parent().unwrap(), ctx.dir.join(bid.to_string()).join("basis"));
    assert!(db.item_delete(first).unwrap());
    assert!(!Path::new(&moved.managed_path).exists());
    assert_eq!(fs::read(&original).unwrap(), b"original");
    assert_eq!(fs::metadata(&original).unwrap().modified().unwrap(), before);
    assert!(db.box_delete(bid).unwrap());
    assert!(!ctx.dir.join(bid.to_string()).exists());
    assert!(original.exists());
}

#[test]
fn test_legacy_prepare_is_lazy_and_missing_copy_never_restores_original() {
    let ctx = TestContext::new("legacy_prepare");
    let path = ctx.dir.join("library.db");
    // 이전 버전 DB에는 새 열이 없다.
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch("CREATE TABLE item(id INTEGER PRIMARY KEY, box_id INTEGER NOT NULL, kind TEXT NOT NULL, source_path TEXT NOT NULL, display_name TEXT NOT NULL, space TEXT NOT NULL, x REAL NOT NULL, y REAL NOT NULL, created_at REAL NOT NULL)").unwrap();
    drop(old);
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let bid = db.box_create("기존", "").unwrap();
    let original = ctx.dir.join("legacy.txt");
    fs::write(&original, "unchanged").unwrap();
    db.connection().execute("INSERT INTO item VALUES (1,?1,'file',?2,'legacy.txt','reference',0,0,0,'','')", rusqlite::params![bid, original.to_str().unwrap()]).unwrap();
    assert!(db.item_get(1).unwrap().unwrap().managed_path.is_empty());
    assert!(!ctx.dir.join(bid.to_string()).exists());
    let nb = db.notebook_create("기존 출처", "").unwrap();
    let sid = notebook::add_source(db.connection(), nb, "txt", original.to_str().unwrap(), "기존 파일").unwrap();
    db.connection().execute("UPDATE nb_source SET item_id=1 WHERE id=?1", [sid]).unwrap();
    let prepared = db.item_prepare(1).unwrap();
    assert_eq!(notebook::source_by_id(db.connection(), sid).unwrap().unwrap().origin, prepared.managed_path);
    assert_eq!(fs::read_to_string(&prepared.managed_path).unwrap(), "unchanged");
    assert_eq!(prepared.origin_path, original.to_str().unwrap());
    assert_eq!(db.item_prepare(1).unwrap().managed_path, prepared.managed_path);
    let backup: String = db.connection().query_row("SELECT source_path FROM item_path_backup WHERE item_id=1", [], |r| r.get(0)).unwrap();
    assert_eq!(backup, original.to_str().unwrap());
    fs::remove_file(&prepared.managed_path).unwrap();
    assert!(db.item_prepare(1).is_err());
    assert!(!Path::new(&prepared.managed_path).exists());
    assert_eq!(fs::read_to_string(&original).unwrap(), "unchanged");
    db.connection().execute("INSERT INTO item VALUES (2,?1,'file',?2,'missing.txt','temp',0,0,0,'','')", rusqlite::params![bid, ctx.dir.join("missing.txt").to_str().unwrap()]).unwrap();
    assert!(db.item_prepare(2).is_err());
    assert!(db.item_get(2).unwrap().unwrap().managed_path.is_empty());
}

#[test]
fn test_copy_dir_rejects_overlapping_targets_before_creation() {
    let ctx = TestContext::new("copy_overlap");
    let src = ctx.dir.join("source");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("nested/document.txt"), "preserved").unwrap();
    for dst in [src.clone(), src.join("export"), src.join("nested/../export"), ctx.dir.clone()] {
        assert_eq!(db::copy_dir_recursive(&src, &dst).unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    }
    assert!(!src.join("export").exists());
    assert_eq!(fs::read_to_string(src.join("nested/document.txt")).unwrap(), "preserved");
    // A sibling sharing only a string prefix is safe.
    let sibling = ctx.dir.join("source-copy");
    db::copy_dir_recursive(&src, &sibling).unwrap();
    assert_eq!(fs::read_to_string(sibling.join("nested/document.txt")).unwrap(), "preserved");
}

#[cfg(windows)]
#[test]
fn test_copy_and_export_reject_windows_path_aliases() {
    let ctx = TestContext::new("copy_windows_alias");
    let src = ctx.dir.join("source");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("document.txt"), "preserved").unwrap();
    // Win32 trailing-dot aliases, like 8.3 names, differ lexically but identify
    // the same directory. The old comparison would copy into its own output.
    let alias = ctx.dir.join("source.");
    assert_eq!(alias.canonicalize().unwrap(), src.canonicalize().unwrap());
    assert!(db::copy_dir_recursive(&alias, &src.join("export")).is_err());
    assert!(!src.join("export").exists());

    let database = Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let bid = database.box_create("alias", "").unwrap();
    let iid = database.item_add(bid, "folder", src.to_str().unwrap(), "folder", "core", 0., 0., None).unwrap();
    let item = database.item_get(iid).unwrap().unwrap();
    let alias_root = ctx.dir.with_file_name(format!("{}.", ctx.dir.file_name().unwrap().to_str().unwrap()));
    let dst = alias_root.join(Path::new(&item.managed_path).strip_prefix(&ctx.dir).unwrap()).join("export");
    assert!(database.box_export(bid, dst.to_str().unwrap()).is_err());
    assert!(!Path::new(&item.managed_path).join("export").exists());
    assert_eq!(database.box_get(bid).unwrap().unwrap().0.export_path, None);

    let link = ctx.dir.join("symlink");
    std::os::windows::fs::symlink_dir(&src, &link).unwrap();
    let junction = ctx.dir.join("junction");
    let created = std::process::Command::new("cmd.exe").args(["/C", "mklink", "/J"])
        .arg(&junction).arg(&src).output().unwrap();
    assert!(created.status.success(), "{created:?}");
    for alias in [&link, &junction] {
        assert!(db::copy_dir_recursive(&src, &alias.join("export")).is_err());
        assert!(database.box_export(bid, alias.join("export").to_str().unwrap()).is_err());
    }
    assert!(!src.join("export").exists());
}

#[test]
fn test_export_uses_edited_managed_folder_and_rejects_unsafe_targets() {
    let ctx = TestContext::new("managed_export");
    let db = Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let bid = db.box_create("내보내기", "").unwrap();
    let original = ctx.dir.join("folder");
    fs::create_dir_all(original.join("nested")).unwrap();
    fs::write(original.join("nested/document.txt"), "original").unwrap();
    let iid = db.item_add(bid, "folder", original.to_str().unwrap(), "자료폴더", "core", 0., 0., None).unwrap();
    let it = db.item_get(iid).unwrap().unwrap();
    fs::write(Path::new(&it.managed_path).join("nested/document.txt"), "managed edit").unwrap();
    assert!(db.box_export(bid, original.join("export").to_str().unwrap()).is_err());
    assert!(db.box_export(bid, Path::new(&it.managed_path).join("export").to_str().unwrap()).is_err());
    #[cfg(windows)]
    assert!(db.box_export(bid, Path::new(&it.managed_path).canonicalize().unwrap().join("export").to_str().unwrap()).is_err());
    let other = db.box_create("다른 상자", "").unwrap();
    assert!(db.box_export(bid, ctx.dir.join(other.to_string()).join("temp/export").to_str().unwrap()).is_err());
    assert!(db.box_export(bid, ctx.dir.join("boxes").join(other.to_string()).join("export").to_str().unwrap()).is_err());
    // 원본을 옮겨도 전환은 이미 보관된 사본만 읽는다.
    fs::rename(&original, ctx.dir.join("origin_moved")).unwrap();
    let export = ctx.dir.join("export");
    // test root는 storage root이므로 별도 형제 폴더로 내보낸다.
    let export = export.with_file_name(format!("vfcopy-export-{}", std::process::id()));
    let export = ctx.dir.parent().unwrap().join(export.file_name().unwrap());
    let _ = fs::remove_dir_all(&export);
    assert_eq!(db.box_export(bid, export.to_str().unwrap()).unwrap(), 1);
    assert_eq!(fs::read_to_string(export.join("1_핵심자료/자료폴더/nested/document.txt")).unwrap(), "managed edit");
    assert!(Path::new(&it.managed_path).exists());
    assert_eq!(fs::read_to_string(ctx.dir.join("origin_moved/nested/document.txt")).unwrap(), "original");
    for area in ["1_핵심자료", "2_기초자료", "3_참고자료", "4_임시저장소"] { assert!(export.join(area).is_dir()); }
    fs::remove_dir_all(export).unwrap();
}

#[test]
fn test_managed_path_tampering_missing_delete_and_origin_clone() {
    let ctx = TestContext::new("copy_validation");
    let db = Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let bid = db.box_create("검증", "").unwrap();
    let original = ctx.dir.join("원본.txt");
    fs::write(&original, "original").unwrap();
    let iid = db.item_add(bid, "file", original.to_str().unwrap(), "원본.txt", "core", 0., 0., None).unwrap();
    let one = db.item_prepare(iid).unwrap();
    let next_box = db.box_create("다른 상자", "").unwrap();
    let cloned = db.item_add(next_box, "file", &one.managed_path, "다른 복사본", "temp", 0., 0., None).unwrap();
    assert_eq!(db.item_get(cloned).unwrap().unwrap().origin_path, original.to_str().unwrap());
    db.connection().execute("UPDATE item SET space='../..',source_path=?1,managed_path=?1 WHERE id=?2", rusqlite::params![original.to_str().unwrap(), iid]).unwrap();
    assert!(db.item_prepare(iid).is_err());
    assert!(db.item_delete(iid).is_err());
    assert_eq!(fs::read_to_string(&original).unwrap(), "original");
    db.connection().execute("UPDATE item SET space='core',source_path=?1,managed_path=?1 WHERE id=?2", rusqlite::params![one.managed_path, iid]).unwrap();
    fs::remove_file(&one.managed_path).unwrap();
    assert!(db.item_delete(iid).unwrap());
    assert!(original.exists());
}

#[test]
fn test_prepare_pipe_returns_managed_paths_and_distinct_missing_codes() {
    let ctx = TestContext::new("prepare_contract");
    let database = Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let bid = database.box_create("이전 열기", "").unwrap();
    let origin = ctx.dir.join("original.png");
    fs::write(&origin, b"image bytes").unwrap();
    database.connection().execute("INSERT INTO item(box_id,kind,source_path,display_name,space,x,y,created_at) VALUES(?1,'image',?2,'사진','reference',0,0,0)", rusqlite::params![bid, origin.to_str().unwrap()]).unwrap();
    let iid = database.connection().last_insert_rowid();
    let db = Mutex::new(database);
    let running = AtomicBool::new(true);
    let call = |request: String| json::parse(&pipe::handle_request(&request, &db, &running)).unwrap();
    let before = call(format!(r#"{{"cmd":"item_get","id":{iid}}}"#));
    assert_eq!(before.str("managed_path"), Some(""));
    let prepared = call(format!(r#"{{"cmd":"item_prepare","id":{iid}}}"#));
    assert_eq!(prepared.bool("ok"), Some(true), "{prepared:?}");
    assert_eq!(prepared.str("origin_path"), origin.to_str());
    assert_eq!(prepared.str("source_path"), prepared.str("managed_path"));
    let managed = prepared.str("managed_path").unwrap();
    assert_ne!(managed, origin.to_str().unwrap());
    assert_eq!(fs::read(managed).unwrap(), b"image bytes");
    fs::remove_file(managed).unwrap();
    let missing = call(format!(r#"{{"cmd":"item_prepare","id":{iid}}}"#));
    assert_eq!(missing.bool("ok"), Some(false));
    assert_eq!(missing.str("code"), Some("MANAGED_MISSING"));
    assert_eq!(fs::read(&origin).unwrap(), b"image bytes");
    {
        let guard = db.lock().unwrap();
        guard.connection().execute("UPDATE item SET source_path=?1,managed_path='',origin_path='' WHERE id=?2", rusqlite::params![ctx.dir.join("missing.png").to_str().unwrap(), iid]).unwrap();
    }
    let missing = call(format!(r#"{{"cmd":"item_prepare","id":{iid}}}"#));
    assert_eq!(missing.str("code"), Some("SOURCE_MISSING"));
    let unknown = call(r#"{"cmd":"item_prepare","id":999999}"#.into());
    assert_eq!(unknown.str("code"), Some("NOT_REGISTERED"));
}

// REL2255B: independent cases, normal test-thread stack, no panic-catching.
fn rel2255b_copy_fixture(ctx: &TestContext) -> PathBuf {
    let source = ctx.dir.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("document.txt"), "original").unwrap();
    source
}

fn rel2255b_assert_copy_rejected(source: &Path, target: &Path) {
    let error = db::copy_dir_recursive(source, target).expect_err("unsafe copy must be rejected before traversal");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{error}");
    assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
}

#[test]
fn test_rel2255b_copy_target_inside_source() {
    let ctx = TestContext::new("rel2255b_copy_child");
    let source = rel2255b_copy_fixture(&ctx);
    rel2255b_assert_copy_rejected(&source, &source.join("new/child"));
    assert!(!source.join("new").exists());
}

#[test]
fn test_rel2255b_copy_source_inside_target() {
    let ctx = TestContext::new("rel2255b_copy_parent");
    let source = rel2255b_copy_fixture(&ctx);
    rel2255b_assert_copy_rejected(&source, &ctx.dir);
    assert!(!ctx.dir.join("document.txt").exists());
}

#[test]
fn test_rel2255b_copy_same_path() {
    let ctx = TestContext::new("rel2255b_copy_same");
    let source = rel2255b_copy_fixture(&ctx);
    rel2255b_assert_copy_rejected(&source, &source);
    assert_eq!(fs::read_dir(&source).unwrap().count(), 1);
}

#[test]
fn test_rel2255b_copy_similar_prefix_is_sibling() {
    let ctx = TestContext::new("rel2255b_copy_prefix");
    for (from, to) in [("source", "source-backup"), ("archive-backup", "archive")] {
        let source = ctx.dir.join(from);
        let target = ctx.dir.join(to);
        fs::create_dir(&source).unwrap();
        fs::write(source.join("document.txt"), "original").unwrap();
        db::copy_dir_recursive(&source, &target).expect("component-distinct siblings are safe");
        assert_eq!(fs::read_to_string(target.join("document.txt")).unwrap(), "original");
        assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
    }
}

fn rel2255b_export_fixture(ctx: &TestContext) -> (Database, i64, PathBuf) {
    let database = Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let bid = database.box_create("REL2255B", "").unwrap();
    let source = rel2255b_copy_fixture(ctx);
    database.item_add(bid, "folder", source.to_str().unwrap(), "folder", "core", 0., 0., None).unwrap();
    (database, bid, source)
}

#[test]
fn test_rel2255b_export_target_inside_original() {
    let ctx = TestContext::new("rel2255b_export_child");
    let (database, bid, source) = rel2255b_export_fixture(&ctx);
    let error = database.box_export(bid, source.join("new/export").to_str().unwrap()).expect_err("original descendant must be rejected");
    assert!(error.contains("폴더"), "{error}");
    assert!(!source.join("new").exists());
    assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
}

#[test]
fn test_rel2255b_export_same_as_original() {
    let ctx = TestContext::new("rel2255b_export_same");
    let (database, bid, source) = rel2255b_export_fixture(&ctx);
    let error = database.box_export(bid, source.to_str().unwrap()).expect_err("same original must be rejected");
    assert!(error.contains("폴더"), "{error}");
    assert_eq!(fs::read_dir(&source).unwrap().count(), 1);
    assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
}

// Windows junctions need no Developer Mode/symlink privilege; no silent skip.
fn rel2255b_directory_alias(target: &Path, alias: &Path) {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"]).arg(alias).arg(target).status().unwrap();
        assert!(status.success(), "junction fixture must be created");
    }
    #[cfg(not(windows))]
    std::os::unix::fs::symlink(target, alias).unwrap();
    assert!(util::is_or_has_reparse_point(alias));
}

fn rel2255b_remove_alias(alias: &Path) {
    #[cfg(windows)]
    fs::remove_dir(alias).unwrap();
    #[cfg(not(windows))]
    fs::remove_file(alias).unwrap();
}

#[test]
fn test_rel2255b_copy_target_through_directory_alias() {
    let ctx = TestContext::new("rel2255b_copy_alias");
    let source = rel2255b_copy_fixture(&ctx);
    let alias = ctx.dir.join("alias");
    rel2255b_directory_alias(&source, &alias);
    let result = db::copy_dir_recursive(&source, &alias.join("new/child"));
    rel2255b_remove_alias(&alias);
    let error = result.expect_err("alias descendant must be rejected before traversal");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData, "{error}");
    assert!(!source.join("new").exists());
    assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
}

#[test]
fn test_rel2255b_export_target_through_directory_alias() {
    let ctx = TestContext::new("rel2255b_export_alias");
    let (database, bid, source) = rel2255b_export_fixture(&ctx);
    let alias = ctx.dir.join("alias");
    rel2255b_directory_alias(&source, &alias);
    let result = database.box_export(bid, alias.join("new/export").to_str().unwrap());
    rel2255b_remove_alias(&alias);
    let error = result.expect_err("alias export must be rejected before traversal");
    assert!(error.contains("링크") || error.contains("정션"), "{error}");
    assert!(!source.join("new").exists());
    assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
}

#[test]
fn test_rel2255b_export_original_inside_target() {
    let ctx = TestContext::new("rel2255b_export_parent");
    let database = Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap();
    let bid = database.box_create("REL2255B parent", "").unwrap();
    let target = ctx.dir.join("original-parent");
    let source = target.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("document.txt"), "original").unwrap();
    database.item_add(bid, "folder", source.to_str().unwrap(), "folder", "core", 0., 0., None).unwrap();
    let error = database.box_export(bid, target.to_str().unwrap()).expect_err("original ancestor must be rejected");
    assert!(error.contains("원본") || error.contains("상위"), "must reject overlap before generic nonempty check: {error}");
    assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
    assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
}

#[test]
fn test_rel2255b_copy_source_through_directory_alias() {
    let ctx = TestContext::new("rel2255b_copy_source_alias");
    let source = rel2255b_copy_fixture(&ctx);
    let alias = ctx.dir.join("alias");
    let target = ctx.dir.join("output");
    rel2255b_directory_alias(&source, &alias);
    let result = db::copy_dir_recursive(&alias, &target);
    rel2255b_remove_alias(&alias);
    let error = result.expect_err("source alias must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData, "{error}");
    assert!(!target.exists());
    assert_eq!(fs::read_to_string(source.join("document.txt")).unwrap(), "original");
}

// ---- 파이프 서버(T1111-PIPE): 이벤트 대기 읽기, 한 연결 여러 요청, 대기 인스턴스 ----

#[test]
fn test_pipe_take_line_limits() {
    use pipe::{Line, take_line};
    let mut data = b"{\"cmd\":\"ping\"}\n{\"cmd\":".to_vec();
    assert_eq!(take_line(&mut data), Line::Ready("{\"cmd\":\"ping\"}".into()));
    assert_eq!(data, b"{\"cmd\":");
    assert_eq!(take_line(&mut data), Line::More);
    // 줄바꿈 전 길이가 정확히 64 KiB면 허용, 1바이트 넘으면 거부.
    let mut exact = vec![b'a'; 64 * 1024];
    assert_eq!(take_line(&mut exact), Line::More);
    exact.push(b'\n');
    assert!(matches!(take_line(&mut exact), Line::Ready(s) if s.len() == 64 * 1024));
    let mut over = vec![b'a'; 64 * 1024 + 1];
    assert_eq!(take_line(&mut over), Line::Reject);
    over.push(b'\n');
    assert_eq!(take_line(&mut over), Line::Reject);
    // UTF-8이 아닌 요청은 거부.
    let mut bad = vec![0xff, 0xfe, b'\n'];
    assert_eq!(take_line(&mut bad), Line::Reject);
    let mut empty = b"\nx".to_vec();
    assert_eq!(take_line(&mut empty), Line::Ready(String::new()));
}

/// 시험용 파이프 서버. 사용자 파이프와 겹치지 않는 이름으로 serve_on을 스레드에 띄운다.
struct PipeServer {
    name: String,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PipeServer {
    fn start(ctx: &TestContext, tag: &str) -> Self {
        let name = format!("\\\\.\\pipe\\HwpAutoDocFit.klib.test.pipew-{tag}-{}.{}", std::process::id(), pipe::user_sid());
        let db = Arc::new(Mutex::new(Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
        let running = Arc::new(AtomicBool::new(true));
        let (n, r) = (name.clone(), running.clone());
        let thread = std::thread::spawn(move || {
            let last_activity = std::sync::atomic::AtomicU64::new(0);
            pipe::serve_on(&n, &db, &r, &last_activity);
        });
        PipeServer { name, running, thread: Some(thread) }
    }
    fn connect(&self) -> fs::File {
        for _ in 0..400 {
            if let Ok(f) = fs::OpenOptions::new().read(true).write(true).open(&self.name) { return f; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("파이프에 연결하지 못했습니다: {}", self.name);
    }
    /// 기존 클라이언트 방식: 연결 → 요청 1개 → 응답 1줄 → 닫기.
    fn once(&self, request: &str) -> String {
        let mut f = self.connect();
        pipe_send(&mut f, request);
        pipe_read_line(&mut f).expect("응답 한 줄")
    }
}

impl Drop for PipeServer {
    fn drop(&mut self) {
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

fn pipe_send(f: &mut fs::File, request: &str) {
    use std::io::Write;
    f.write_all(format!("{request}\n").as_bytes()).unwrap();
}

/// 응답 한 줄(줄바꿈 제외). 서버가 연결을 끊으면 None.
fn pipe_read_line(f: &mut fs::File) -> Option<String> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut b = [0u8; 1];
    loop {
        match f.read(&mut b) {
            Ok(1) if b[0] == b'\n' => return String::from_utf8(out).ok(),
            Ok(1) => out.push(b[0]),
            _ => return None,
        }
    }
}

#[test]
fn test_pipe_server_legacy_client_roundtrip_without_polling_delay() {
    let ctx = TestContext::new("pipe_legacy");
    let server = PipeServer::start(&ctx, "legacy");
    let direct = { let db = Mutex::new(Database::open(ctx.dir.join("direct.db").to_str().unwrap()).unwrap()); pipe::handle_request(r#"{"cmd":"ping"}"#, &db, &AtomicBool::new(true)) };
    assert_eq!(server.once(r#"{"cmd":"ping"}"#), direct);
    let mut times = Vec::new();
    for _ in 0..60 {
        let t = std::time::Instant::now();
        assert_eq!(server.once(r#"{"cmd":"ping"}"#), direct);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[times.len() / 2];
    println!("파이프 왕복(연결마다 1요청) 중앙값 {median:.3} ms, 최대 {:.3} ms", times[times.len() - 1]);
    // 예전 폴링(요청마다 sleep 20ms, 실제 약 31ms)이 되살아나면 걸린다.
    assert!(median < 10.0, "왕복 중앙값 {median:.3} ms");
    // 잘못된 요청도 예전과 같은 응답 한 줄.
    assert_eq!(json::parse(&server.once("not json")).unwrap().bool("ok"), Some(false));
}

#[test]
fn test_pipe_server_many_requests_on_one_connection() {
    let ctx = TestContext::new("pipe_keep");
    let server = PipeServer::start(&ctx, "keep");
    let mut f = server.connect();
    pipe_send(&mut f, r#"{"cmd":"box_create","name":"한 연결","description":""}"#);
    let created = json::parse(&pipe_read_line(&mut f).unwrap()).unwrap();
    let id = created.i64("id").unwrap();
    for i in 0..20 {
        pipe_send(&mut f, &format!(r#"{{"cmd":"box_update","id":{id},"name":"이름{i}","description":"d"}}"#));
        assert_eq!(json::parse(&pipe_read_line(&mut f).unwrap()).unwrap().bool("ok"), Some(true));
    }
    // 두 요청을 한 번에 써도 응답 두 줄이 차례로 온다.
    pipe_send(&mut f, &format!("{{\"cmd\":\"ping\"}}\n{{\"cmd\":\"box_get\",\"id\":{id}}}"));
    assert_eq!(json::parse(&pipe_read_line(&mut f).unwrap()).unwrap().str("message"), Some("pong"));
    let got = json::parse(&pipe_read_line(&mut f).unwrap()).unwrap();
    assert_eq!(got.get("box").and_then(|b| b.str("name")), Some("이름19"));
    // 요청을 나눠 써도(줄바꿈이 나중에 와도) 한 요청으로 처리한다.
    {
        use std::io::Write;
        f.write_all(b"{\"cmd\":").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        f.write_all(b"\"ping\"}\n").unwrap();
    }
    assert_eq!(json::parse(&pipe_read_line(&mut f).unwrap()).unwrap().str("message"), Some("pong"));
}

#[test]
fn test_pipe_server_second_client_while_first_stays_connected() {
    let ctx = TestContext::new("pipe_two");
    let server = PipeServer::start(&ctx, "two");
    let mut first = server.connect();
    pipe_send(&mut first, r#"{"cmd":"ping"}"#);
    assert!(pipe_read_line(&mut first).is_some());
    // 첫 연결이 열린 채(요청을 쓰다 만 상태 포함)여도 다른 클라이언트가 바로 처리된다.
    { use std::io::Write; first.write_all(b"{\"cmd\":\"pi").unwrap(); }
    let t = std::time::Instant::now();
    for _ in 0..5 {
        assert_eq!(json::parse(&server.once(r#"{"cmd":"ping"}"#)).unwrap().str("message"), Some("pong"));
    }
    assert!(t.elapsed().as_millis() < 2000, "두 번째 클라이언트가 막혔습니다: {:?}", t.elapsed());
    { use std::io::Write; first.write_all(b"ng\"}\n").unwrap(); }
    assert_eq!(json::parse(&pipe_read_line(&mut first).unwrap()).unwrap().str("message"), Some("pong"));
}

#[test]
fn test_pipe_server_survives_broken_and_oversized_clients() {
    let ctx = TestContext::new("pipe_bad");
    let server = PipeServer::start(&ctx, "bad");
    // 줄바꿈 없이 끊은 클라이언트.
    { use std::io::Write; let mut f = server.connect(); f.write_all(b"{\"cmd\":\"ping\"").unwrap(); }
    // 연결만 하고 끊은 클라이언트.
    drop(server.connect());
    // 64 KiB를 넘는 요청: 응답 없이 끊긴다.
    {
        use std::io::Write;
        let mut f = server.connect();
        let big = format!("{{\"cmd\":\"ping\",\"pad\":\"{}\"}}\n", "a".repeat(70 * 1024));
        let _ = f.write_all(big.as_bytes());
        assert_eq!(pipe_read_line(&mut f), None);
    }
    // UTF-8이 아닌 요청: 응답 없이 끊긴다.
    {
        use std::io::Write;
        let mut f = server.connect();
        f.write_all(&[0xff, 0xfe, b'\n']).unwrap();
        assert_eq!(pipe_read_line(&mut f), None);
    }
    assert_eq!(json::parse(&server.once(r#"{"cmd":"ping"}"#)).unwrap().str("message"), Some("pong"));
}

#[test]
fn test_pipe_server_quit_replies_then_stops() {
    let ctx = TestContext::new("pipe_quit");
    let mut server = PipeServer::start(&ctx, "quit");
    let idle = server.connect(); // 열려만 있는 다른 연결이 종료를 막지 않아야 한다
    let reply = json::parse(&server.once(r#"{"cmd":"quit"}"#)).unwrap();
    assert_eq!(reply.bool("ok"), Some(true));
    let t = std::time::Instant::now();
    server.thread.take().unwrap().join().unwrap();
    assert!(t.elapsed().as_millis() < 3000, "quit 뒤 서버가 끝나지 않았습니다");
    assert!(!server.running.load(std::sync::atomic::Ordering::Relaxed));
    drop(idle);
}

/// edit_* 명령은 klib가 파싱한 값을 그대로 편집 코어에 넘긴다(이중 파싱 제거). 응답은 요청 문자열을 코어에 직접 넘긴 것과 같아야 한다.
#[test]
fn test_edit_requests_reuse_parsed_value_same_response() {
    let ctx = TestContext::new("edit_parsed");
    let db = Mutex::new(Database::open(ctx.dir.join("library.db").to_str().unwrap()).unwrap());
    let running = AtomicBool::new(true);
    let mut core = editor_core::api::Engine::new();
    let open = r#"{"cmd":"edit_open","text":"가나다\nabc😀\n","line_ending":"CRLF"}"#;
    let via_pipe = json::parse(&pipe::handle_request(open, &db, &running)).unwrap().i64("doc").unwrap();
    let via_core = json::parse(&core.handle(open)).unwrap().i64("doc").unwrap();
    let big = "붙여넣기 큰 글 ".repeat(2500);
    let templates = [
        r#"{"cmd":"edit_ping"}"#.to_string(),
        r#"{"cmd":"edit_insert","doc":DOC,"pos":0,"text":"\"따옴표\"\t\\ \/ é😀\u0000\n"}"#.to_string(),
        format!(r#"{{"cmd":"edit_insert","doc":DOC,"pos":0,"text":"{big}"}}"#),
        r#" { "cmd" : "edit_text" , "doc" : DOC } "#.to_string(),
        r#"{"cmd":"edit_line_range","doc":DOC,"start_line":0,"count":1e1}"#.to_string(),
        r#"{"cmd":"edit_find_all","doc":DOC,"query":"가","case_sensitive":true,"whole_word":false}"#.to_string(),
        r#"{"cmd":"edit_delete","doc":DOC,"start":0,"end":3.0}"#.to_string(),
        r#"{"cmd":"edit_delete","doc":DOC,"start":-0,"end":1.5}"#.to_string(),
        r#"{"cmd":"edit_undo","doc":DOC,"cmd":"edit_redo"}"#.to_string(),
        r#"{"cmd":"edit_status","doc":DOC,"pos":0,"extra":[1,{"a":null},[]]}"#.to_string(),
        r#"{"cmd":"edit_text","doc":999999}"#.to_string(),
        r#"{"cmd":"edit_nope","doc":DOC}"#.to_string(),
        r#"{"cmd":"edit_text"}"#.to_string(),
        r#"{"cmd":"edit_close","doc":DOC}"#.to_string(),
    ];
    for t in templates {
        let a = pipe::handle_request(&t.replace("DOC", &via_pipe.to_string()), &db, &running);
        let b = core.handle(&t.replace("DOC", &via_core.to_string()));
        assert_eq!(a.replace(&format!("문서 {via_pipe}"), "문서 N"), b.replace(&format!("문서 {via_core}"), "문서 N"), "{}", t.chars().take(80).collect::<String>());
    }
}

/// 색인을 다시 만들 일이 없으면 상태는 ready이고, 검색 응답에 '색인 중' 표시가 붙지 않는다.
#[test]
fn test_index_status_ready_and_search_without_indexing_flag() {
    let ctx = TestContext::new("index_status");
    let db = Arc::new(Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));
    let call = |req: &str| json::parse(&pipe::handle_request(req, &db, &running)).unwrap();
    let status = call(r#"{"cmd":"index_status"}"#);
    assert_eq!(status.bool("ok"), Some(true), "{status:?}");
    assert_eq!(status.str("state"), Some("ready"), "{status:?}");
    assert_eq!(status.bool("indexing"), Some(false));
    assert_eq!(status.i64("schema_version"), Some(search::SCHEMA_VERSION as i64));
    let found = call(r#"{"cmd":"search","query":"없는검색어xyz"}"#);
    assert_eq!(found.bool("ok"), Some(true), "{found:?}");
    assert!(found.get("indexing").is_none(), "{found:?}");
    reindex::wait();
    assert_eq!(call(r#"{"cmd":"index_status"}"#).str("state"), Some("ready"));
}

/// NBIDX1: 출처 다시 색인은 같은 출처 번호로 문단을 바꾸고(인용 연결 유지), 원본 변경 검사·나눠 보내기·상태 명령이 맞물린다.
#[test]
fn test_nb_source_refresh_check_and_status() {
    let ctx = TestContext::new("nb_refresh");
    let db = Arc::new(Mutex::new(Database::open(&ctx.dir.join("library.db").to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let status = call(r#"{"cmd":"nb_status"}"#.into());
    assert_eq!(status.bool("ready"), Some(true), "{status:?}");
    assert_eq!(status.get("migration").and_then(|m| m.str("state")), Some("not_needed"));
    assert!(ctx.dir.join("notebook").join("notebook.db").is_file());
    assert!(ctx.dir.join("notebook").join("tantivy").join("meta.json").is_file());

    let nb = call(r#"{"cmd":"nb_create","name":"노트"}"#.into()).i64("id").unwrap();
    let md = ctx.dir.join("계획.md");
    fs::write(&md, "# 추진 계획\n복지 예산은 120억 원이다.\n도로 정비\n").unwrap();
    let md_path = md.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/");
    let sid = call(format!(r#"{{"cmd":"nb_source_add","notebook_id":{nb},"source_path":"{md_path}"}}"#)).i64("id").unwrap();
    let note = call(format!(r#"{{"cmd":"nb_note_save","notebook_id":{nb},"title":"메모","body":"","citations":[{{"source_id":{sid},"paragraph_ordinal":1,"quote":"복지 예산은 120억 원이다."}}]}}"#)).i64("id").unwrap();
    let checked = call(r#"{"cmd":"nb_source_check"}"#.into());
    assert_eq!(checked.arr("refresh").unwrap().len(), 0, "{checked:?}");

    // 원본이 바뀌면 stale → 다시 색인하면 같은 번호로 ready, 인용은 옮겨진 문단을 가리킨다.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(&md, "# 추진 계획\n새로 넣은 교통 대책\n복지 예산은 120억 원이다.\n도로 정비\n").unwrap();
    let checked = call(format!(r#"{{"cmd":"nb_source_check","notebook_id":{nb}}}"#));
    let refresh = checked.arr("refresh").unwrap();
    assert_eq!(refresh.iter().map(|r| (r.i64("source_id").unwrap(), r.str("status").unwrap().to_string())).collect::<Vec<_>>(), vec![(sid, "stale".to_string())]);
    assert_eq!(refresh[0].bool("needs_paragraphs"), Some(false));
    let done = call(format!(r#"{{"cmd":"nb_source_refresh","notebook_id":{nb},"source_id":{sid}}}"#));
    assert_eq!(done.bool("ok"), Some(true), "{done:?}");
    assert_eq!((done.i64("source_id"), done.i64("paragraph_count"), done.i64("relinked"), done.i64("unresolved")), (Some(sid), Some(4), Some(1), Some(0)));
    let notes = call(format!(r#"{{"cmd":"nb_note_list","notebook_id":{nb}}}"#));
    let first_note = &notes.arr("notes").unwrap()[0];
    let cite = &first_note.arr("citations").unwrap()[0];
    assert_eq!((first_note.i64("id"), cite.i64("source_id"), cite.i64("paragraph_ordinal")), (Some(note), Some(sid), Some(2)));
    let hits = call(format!(r#"{{"cmd":"nb_search","notebook_id":{nb},"query":"교통 대책"}}"#));
    assert_eq!(hits.arr("results").unwrap().iter().map(|h| (h.i64("source_id").unwrap(), h.i64("ordinal").unwrap())).collect::<Vec<_>>(), vec![(sid, 1)]);
    let list = call(format!(r#"{{"cmd":"nb_source_list","notebook_id":{nb}}}"#));
    assert_eq!(list.arr("sources").unwrap()[0].str("status"), Some("ready"));
    assert_eq!(call(r#"{"cmd":"nb_source_check"}"#.into()).arr("refresh").unwrap().len(), 0);

    // hwp·pdf는 Python이 문단을 나눠 보낸다(part 0 … last). 마지막 묶음 전까지는 옛 문단으로 검색된다.
    let pdf = ctx.dir.join("자료.pdf");
    fs::write(&pdf, b"%PDF-1.4 a").unwrap();
    let pdf_path = pdf.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/");
    let pid = call(format!(r#"{{"cmd":"nb_source_add","notebook_id":{nb},"source_path":"{pdf_path}","paragraphs":[{{"text":"옛 보고서 문단"}}]}}"#)).i64("id").unwrap();
    let needs = call(format!(r#"{{"cmd":"nb_source_refresh","notebook_id":{nb},"source_id":{pid}}}"#));
    assert_eq!((needs.bool("ok"), needs.str("code")), (Some(false), Some("NEEDS_PARAGRAPHS")));
    let first = call(format!(r#"{{"cmd":"nb_source_refresh","notebook_id":{nb},"source_id":{pid},"part":0,"last":false,"paragraphs":[{{"text":"새 보고서 첫 문단"}}]}}"#));
    assert_eq!((first.i64("staged"), first.str("status")), (Some(1), Some("refreshing")), "{first:?}");
    assert_eq!(call(format!(r#"{{"cmd":"nb_search","notebook_id":{nb},"query":"옛 보고서"}}"#)).arr("results").unwrap().len(), 1);
    let last = call(format!(r#"{{"cmd":"nb_source_refresh","notebook_id":{nb},"source_id":{pid},"part":1,"last":true,"paragraphs":[{{"text":"새 보고서 둘째 문단"}}]}}"#));
    assert_eq!(last.i64("paragraph_count"), Some(2), "{last:?}");
    assert!(call(format!(r#"{{"cmd":"nb_search","notebook_id":{nb},"query":"옛 보고서 문단"}}"#)).arr("results").unwrap().iter().all(|h| h.str("body") != Some("옛 보고서 문단")));
    assert_eq!(call(format!(r#"{{"cmd":"nb_search","notebook_id":{nb},"query":"보고서","source_ids":[{pid}]}}"#)).arr("results").unwrap().len(), 2);
    // 순서가 어긋난 묶음은 거절.
    assert_eq!(call(format!(r#"{{"cmd":"nb_source_refresh","notebook_id":{nb},"source_id":{pid},"part":3,"paragraphs":[{{"text":"x"}}]}}"#)).bool("ok"), Some(false));

    // 원본이 없어지면 missing, 다시 색인하면 MISSING 코드.
    fs::remove_file(&md).unwrap();
    assert_eq!(call(r#"{"cmd":"nb_source_check"}"#.into()).get("counts").and_then(|c| c.i64("missing")), Some(1));
    assert_eq!(call(format!(r#"{{"cmd":"nb_source_refresh","notebook_id":{nb},"source_id":{sid}}}"#)).str("code"), Some("MISSING"));
    // 다른 노트북의 출처는 거절.
    let other = call(r#"{"cmd":"nb_create","name":"다른"}"#.into()).i64("id").unwrap();
    assert_eq!(call(format!(r#"{{"cmd":"nb_source_refresh","notebook_id":{other},"source_id":{pid}}}"#)).bool("ok"), Some(false));
    let status = call(r#"{"cmd":"nb_status"}"#.into());
    assert_eq!(status.i64("index_paragraphs"), Some(6), "{status:?}");
    assert_eq!(status.get("sources").and_then(|s| s.i64("missing")), Some(1));
}

/// NBIDX1: 옛 버전 자료함 DB(main에 업무자료함 표)를 열면 별도 저장소로 한 번 옮기고, 공용 색인의 업무자료함 문단은 지운다.
#[test]
fn test_nb_legacy_library_moves_on_open_and_cleans_shared_index() {
    let ctx = TestContext::new("nb_legacy_move");
    let path = ctx.dir.join("library.db");
    let (nb, sid) = {
        let c = rusqlite::Connection::open(&path).unwrap();
        notebook::init_schema(&c).unwrap();
        let nb = notebook::create(&c, "옛 노트", "").unwrap();
        let sid = notebook::add_source(&c, nb, "txt", "옛.txt", "옛 출처").unwrap();
        let ps = vec![notebook::Paragraph { source_id: sid, ordinal: 0, location: String::new(), level: 0, kind: "text".into(), body: "옛 자료함에 있던 정책 문단".into() }];
        notebook::replace_paragraphs(&c, sid, &ps).unwrap();
        // 옛 버전은 업무자료함 문단을 공용 색인에 넣었다.
        let s = notebook::source_by_id(&c, sid).unwrap().unwrap();
        search_index().unwrap().replace_notebook_source(nb, &s, &ps).unwrap();
        (nb, sid)
    };
    assert_eq!(search_index().unwrap().search_notebook(nb, "정책", None, 5).unwrap().len(), 1);
    let db = Arc::new(Mutex::new(Database::open(path.to_str().unwrap()).unwrap()));
    let running = Arc::new(AtomicBool::new(true));
    let call = |req: String| json::parse(&pipe::handle_request(&req, &db, &running)).unwrap();
    let status = call(r#"{"cmd":"nb_status"}"#.into());
    let m = status.get("migration").unwrap();
    assert_eq!((m.str("state"), m.i64("attempts")), (Some("done"), Some(1)), "{status:?}");
    assert!(Path::new(m.str("backup").unwrap()).is_file());
    let hits = call(format!(r#"{{"cmd":"nb_search","notebook_id":{nb},"query":"정책"}}"#));
    assert_eq!(hits.arr("results").unwrap().iter().map(|h| h.i64("source_id").unwrap()).collect::<Vec<_>>(), vec![sid]);
    assert!(search_index().unwrap().search_notebook(nb, "정책", None, 5).unwrap().is_empty(), "공용 색인에서 업무자료함 문단이 지워져야 한다");
    let state: String = fs::read_to_string(ctx.dir.join("notebook").join("migration.json")).unwrap();
    assert!(state.contains("\"state\":\"done\""), "{state}");
}
