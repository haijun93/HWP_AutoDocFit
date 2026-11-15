//! scan-hwp 합성 폴더·파일 분류 및 제외 규칙 통합 테스트.
//! 로컬 경로를 하드코딩하지 않고 temp_dir 기반 임시 디렉터리를 사용합니다.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use klib::hwp5::SyntheticHwp5Builder;
use klib::scan_hwp::{scan_roots, HwpKind, ScanItem, OLE_SIGNATURE, HWP3_SIGNATURE};

struct TempTestDir {
    path: PathBuf,
}

impl TempTestDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("klib-test-scan-hwp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("임시 디렉터리 생성");
        Self { path }
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn create_file(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("상위 디렉터리 생성");
    }
    let mut f = File::create(path).expect("파일 생성");
    f.write_all(content).expect("파일 쓰기");
}

#[test]
fn test_scan_hwp_synthetic_classification() {
    let test_dir = TempTestDir::new("classification");
    let root = &test_dir.path;

    // 1. 정상 HWP5
    let normal_bytes = SyntheticHwp5Builder::new()
        .version(0x05000300)
        .compressed(true)
        .build()
        .expect("HWP5 생성");
    create_file(&root.join("normal.hwp"), &normal_bytes);

    // 2. 암호 HWP5
    let enc_bytes = SyntheticHwp5Builder::new()
        .version(0x05000300)
        .password_encrypted(true)
        .build()
        .expect("암호 HWP5 생성");
    create_file(&root.join("encrypted.hwp"), &enc_bytes);

    // 3. 배포용 HWP5
    let dist_bytes = SyntheticHwp5Builder::new()
        .version(0x05000300)
        .distributed(true)
        .build()
        .expect("배포용 HWP5 생성");
    create_file(&root.join("distributed.hwp"), &dist_bytes);

    // 4. FileHeader 플래그 DRM HWP5
    let drm_flag_bytes = SyntheticHwp5Builder::new()
        .version(0x05000300)
        .drm(true)
        .build()
        .expect("DRM HWP5 생성");
    create_file(&root.join("drm_flag.hwp"), &drm_flag_bytes);

    // 5. 외부 DRM 래퍼 파일 (비-OLE, 비-HWP3)
    let drm_wrapper_bytes = b"Fasoo Secure Document Wrapper Content Here.... 1234567890";
    create_file(&root.join("drm_wrapper.hwp"), drm_wrapper_bytes);

    // 6. 0바이트 빈 파일 (손상)
    create_file(&root.join("empty.hwp"), &[]);

    // 7. 손상된 OLE 파일 (시그니처만 있고 FileHeader 없음)
    let mut corrupt_ole = OLE_SIGNATURE.to_vec();
    corrupt_ole.extend_from_slice(&[0u8; 128]);
    create_file(&root.join("corrupt_ole.hwp"), &corrupt_ole);

    // 8. 이미 같은 이름의 .hwpx 있는 파일
    create_file(&root.join("existing.hwp"), &normal_bytes);
    create_file(&root.join("existing.hwpx"), b"PK\x03\x04fake hwpx content");

    // 9. 구형 HWP 3.0 파일
    let mut hwp3_bytes = HWP3_SIGNATURE.to_vec();
    hwp3_bytes.extend_from_slice(b" version 3.0 header and text...");
    create_file(&root.join("legacy.hwp"), &hwp3_bytes);

    // 스캔 실행
    let results = scan_roots(&[root.clone()]);

    let find = |name: &str| -> Option<&ScanItem> {
        results.iter().find(|item| {
            item.path.file_name().and_then(|n| n.to_str()) == Some(name)
        })
    };

    // 검증 1: 정상 HWP5
    let item_norm = find("normal.hwp").expect("normal.hwp 발견");
    assert_eq!(item_norm.kind, HwpKind::Normal);
    assert_eq!(item_norm.label, "정상 HWP5");
    assert!(!item_norm.hwpx_exists);
    assert!(item_norm.version.is_some());

    // 검증 2: 암호
    let item_enc = find("encrypted.hwp").expect("encrypted.hwp 발견");
    assert_eq!(item_enc.kind, HwpKind::Password);
    assert_eq!(item_enc.label, "암호");
    assert!(!item_enc.hwpx_exists);

    // 검증 3: 배포용
    let item_dist = find("distributed.hwp").expect("distributed.hwp 발견");
    assert_eq!(item_dist.kind, HwpKind::Distributed);
    assert_eq!(item_dist.label, "배포용");
    assert!(!item_dist.hwpx_exists);

    // 검증 4: DRM (플래그)
    let item_drm_flag = find("drm_flag.hwp").expect("drm_flag.hwp 발견");
    assert_eq!(item_drm_flag.kind, HwpKind::Drm);
    assert_eq!(item_drm_flag.label, "DRM");

    // 검증 5: DRM (외부 래퍼)
    let item_drm_wrap = find("drm_wrapper.hwp").expect("drm_wrapper.hwp 발견");
    assert_eq!(item_drm_wrap.kind, HwpKind::Drm);
    assert_eq!(item_drm_wrap.label, "DRM");

    // 검증 6: 0바이트 빈 파일 (손상)
    let item_empty = find("empty.hwp").expect("empty.hwp 발견");
    assert_eq!(item_empty.kind, HwpKind::Corrupt);
    assert_eq!(item_empty.label, "손상");

    // 검증 7: 손상된 OLE (손상)
    let item_corrupt = find("corrupt_ole.hwp").expect("corrupt_ole.hwp 발견");
    assert_eq!(item_corrupt.kind, HwpKind::Corrupt);
    assert_eq!(item_corrupt.label, "손상");

    // 검증 8: 이미 같은 이름의 .hwpx 있음
    let item_existing = find("existing.hwp").expect("existing.hwp 발견");
    assert_eq!(item_existing.kind, HwpKind::Existing);
    assert_eq!(item_existing.label, "이미 같은 이름 .hwpx 있음");
    assert!(item_existing.hwpx_exists);
    assert_eq!(item_existing.file_kind, HwpKind::Normal);

    // 검증 9: HWP 3.0
    let item_hwp3 = find("legacy.hwp").expect("legacy.hwp 발견");
    assert_eq!(item_hwp3.kind, HwpKind::Hwp3);
    assert_eq!(item_hwp3.label, "HWP3");

    // JSON 출력 형식 검증
    for item in &results {
        let val = item.to_json();
        assert!(val.str("path").is_some());
        assert!(val.str("kind").is_some());
        assert!(val.str("label").is_some());
        assert!(val.bool("hwpx_exists").is_some());
        assert!(val.num("size").is_some());
    }
}

#[test]
fn test_scan_hwp_exclusion_rules() {
    let test_dir = TempTestDir::new("exclusions");
    let root = &test_dir.path;

    let valid_bytes = SyntheticHwp5Builder::new().build().unwrap();

    // 정상 루트 파일
    create_file(&root.join("root_valid.hwp"), &valid_bytes);

    // 하위 정상 폴더 내 파일
    create_file(&root.join("subfolder/sub_valid.hwp"), &valid_bytes);

    // 제외 폴더 목록 테스트 (SKIP_DIRS 및 $ 접두사)
    let skip_folders = [
        "windows",
        "program files",
        "program files (x86)",
        "programdata",
        "$recycle.bin",
        "$windows.~bt",
        "system volume information",
        "recovery",
        "appdata",
        "node_modules",
        ".git",
        ".svn",
        "__pycache__",
        "onedrivetemp",
        "hnc",
        "hancom",
        "서식",
        "서식예시",
        "template",
        "templates",
    ];

    for folder in &skip_folders {
        create_file(&root.join(folder).join("ignored.hwp"), &valid_bytes);
        create_file(&root.join(folder.to_uppercase()).join("ignored_upper.hwp"), &valid_bytes);
    }

    // 제외 파일명 테스트 (SKIP_FILES)
    create_file(&root.join("hanwp.hwp"), &valid_bytes);
    create_file(&root.join("HANWP.HWP"), &valid_bytes);

    // 비 HWP 파일
    create_file(&root.join("memo.txt"), b"some text");
    create_file(&root.join("data.xlsx"), b"some excel");
    create_file(&root.join("archive.zip"), b"some zip");

    // 스캔 실행
    let results = scan_roots(&[root.clone()]);

    let found_names: Vec<String> = results
        .iter()
        .filter_map(|i| i.path.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
        .collect();

    // 반드시 포함되어야 하는 파일
    assert!(found_names.contains(&"root_valid.hwp".to_string()));
    assert!(found_names.contains(&"sub_valid.hwp".to_string()));

    // 제외되어야 하는 파일이 결과에 없어야 함
    assert!(!found_names.contains(&"hanwp.hwp".to_string()));
    assert!(!found_names.contains(&"HANWP.HWP".to_string()));
    assert!(!found_names.contains(&"ignored.hwp".to_string()));
    assert!(!found_names.contains(&"ignored_upper.hwp".to_string()));
    assert!(!found_names.contains(&"memo.txt".to_string()));
    assert!(!found_names.contains(&"data.xlsx".to_string()));
    assert!(!found_names.contains(&"archive.zip".to_string()));

    // 결과 개수는 정확히 2개여야 함
    assert_eq!(results.len(), 2);
}

#[test]
fn test_scan_hwp_single_file_root() {
    let test_dir = TempTestDir::new("single_file");
    let file_path = test_dir.path.join("single.hwp");
    let valid_bytes = SyntheticHwp5Builder::new().version(0x05010001).build().unwrap();
    create_file(&file_path, &valid_bytes);

    let results = scan_roots(&[file_path.clone()]);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].kind, HwpKind::Normal);
    assert_eq!(results[0].label, "정상 HWP5");
    assert_eq!(results[0].path, file_path);
}
