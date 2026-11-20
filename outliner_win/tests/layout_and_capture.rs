//! 레이아웃 계산·접힘·파싱 일치·캡처 PNG 시험(저장소 안 샘플 문서 사용, 사용자 문서는 건드리지 않는다).
use outliner_win::capture::{palette_of, render};
use outliner_win::paint::Metrics;
use outliner_win::view::*;
use outliner_win::{SAMPLE_MD, png};
use ui_theme::Kind;

fn md(n: usize) -> String {
    (0..n).map(|i| format!("- 블록 {i}\n")).collect()
}

#[test]
fn n_blocks_give_n_rows_and_scroll_clamps() {
    for n in [0usize, 1, 5, 100, 1000] {
        let v = View::from_markdown(&md(n)).unwrap();
        assert_eq!(v.rows.len(), n, "블록 {n}개 → 화면 줄 {n}개");
        assert_eq!(v.block_count(), n);
    }
    let page = 10;
    assert_eq!(clamp_first(999, 100, page), 90);
    assert_eq!(clamp_first(5, 3, page), 0, "줄이 한 화면보다 적으면 맨 위 0");
    assert_eq!(rows_per_page(520, 36, 28), 17);
    assert_eq!(rows_per_page(10, 36, 28), 1, "작은 창도 최소 1줄");
}

#[test]
fn selection_moves_and_keeps_visible() {
    let mut v = View::from_markdown(&md(50)).unwrap();
    let page = 10;
    for _ in 0..12 {
        v.select(Nav::Down, page);
    }
    assert_eq!(v.selected, 12);
    assert_eq!(v.first, 3, "12번 줄이 화면 맨 아래 줄이 되도록 스크롤");
    v.select(Nav::End, page);
    assert_eq!((v.selected, v.first), (49, 40));
    v.select(Nav::Down, page);
    assert_eq!(v.selected, 49, "끝에서 ↓ 는 그대로");
    v.select(Nav::Home, page);
    assert_eq!((v.selected, v.first), (0, 0));
    v.select(Nav::Up, page);
    assert_eq!(v.selected, 0);
    v.select(Nav::PageDown, page);
    assert_eq!(v.selected, 10);
    assert_eq!(row_at(36 + 28 * 2 + 3, 36, 28, 5, 50), Some(7));
    assert_eq!(row_at(10, 36, 28, 0, 50), None, "머리글 클릭은 줄이 아님");
    assert_eq!(row_at(36 + 28 * 9, 36, 28, 45, 50), None, "목록 끝 아래 빈 곳");
}

#[test]
fn folded_children_are_hidden_and_unfold_restores() {
    let src = "- 가\n - 가1\n  - 가1a\n - 가2\n- 나\n - 나1\n";
    let mut v = View::from_markdown(src).unwrap();
    assert_eq!(v.rows.len(), 6);
    assert_eq!(v.rows.iter().map(|r| r.depth).collect::<Vec<_>>(), vec![0, 1, 2, 1, 0, 1]);
    v.set_selected(0, 10);
    assert!(v.set_fold(true, 10));
    assert_eq!(v.rows.iter().map(|r| v.doc.blocks[r.block].text.as_str()).collect::<Vec<_>>(), vec!["가", "나", "나1"]);
    assert!(v.rows[0].folded && v.rows[0].has_children);
    assert_eq!(v.block_count(), 6, "블록은 그대로, 화면 줄만 줄어듦");
    assert!(!v.set_fold(true, 10), "이미 접힘");
    assert!(v.set_fold(false, 10));
    assert_eq!(v.rows.len(), 6);
    // 하위가 없는 블록은 접히지 않는다.
    v.set_selected(5, 10);
    assert!(!v.set_fold(true, 10));
}

#[test]
fn depth_is_ancestor_count_even_with_mixed_indent_widths() {
    // 1칸·2칸·탭(=4칸)이 섞인 문서.
    let src = "- a\n  - b\n\t- c\n - d\n- e\n";
    let v = View::from_markdown(src).unwrap();
    let d: Vec<usize> = v.rows.iter().map(|r| r.depth).collect();
    assert_eq!(d, vec![0, 1, 2, 1, 0]);
}

#[test]
fn sample_rows_match_parse_result() {
    let doc = outliner_core::parse(SAMPLE_MD).unwrap();
    let v = View::from_markdown(SAMPLE_MD).unwrap();
    assert_eq!(v.block_count(), doc.blocks.len());
    assert_eq!(doc.blocks.len(), 14, "샘플 문서 블록 수");
    // collapsed:: true 인 한 블록의 하위 2개만 숨는다.
    assert_eq!(v.rows.len(), 12);
    let hidden: Vec<&str> = doc.blocks.iter().filter(|b| !v.rows.iter().any(|r| doc.blocks[r.block].id == b.id)).map(|b| b.text.as_str()).collect();
    assert_eq!(hidden, vec!["숨겨져야 하는 하위 1", "숨겨져야 하는 하위 2"]);
    let folded = v.rows.iter().find(|r| r.folded).unwrap();
    assert!(!folded.text.contains("collapsed"), "속성 글자는 화면에서 뺀다: {}", folded.text);
    assert_eq!(v.rows.iter().filter(|r| r.status.as_deref() == Some("TODO")).count(), 2);
    assert_eq!(v.rows.iter().filter(|r| r.status.as_deref() == Some("DONE")).count(), 1);
}

#[test]
fn empty_document_has_no_rows_and_selection_is_safe() {
    let mut v = View::from_markdown("").unwrap();
    assert!(v.rows.is_empty());
    v.select(Nav::Down, 5);
    v.select(Nav::End, 5);
    assert_eq!((v.selected, v.first), (0, 0));
    assert!(!v.set_fold(true, 5));
}

fn is_bg(px: &[u8], bg: u32) -> bool {
    // bg 는 COLORREF(0x00BBGGRR); BGRA 화소와 비교.
    px[2] as u32 == (bg & 0xFF) && px[1] as u32 == ((bg >> 8) & 0xFF) && px[0] as u32 == ((bg >> 16) & 0xFF)
}

/// 줄 `slot`(0부터) 띠 안에서 배경이 아닌 가장 왼쪽 화소 x 와 그 띠의 비배경 화소 수.
fn row_ink(shot: &outliner_win::capture::Shot, bg: u32, m: &Metrics, slot: usize, skip_left: usize) -> (Option<usize>, usize) {
    let top = m.header_h as usize + slot * m.row_h as usize;
    let (mut left, mut count) = (None, 0);
    for y in top..top + m.row_h as usize {
        for x in skip_left..shot.width {
            let i = (y * shot.width + x) * 4;
            if !is_bg(&shot.bgra[i..i + 4], bg) {
                count += 1;
                left = Some(left.map_or(x, |l: usize| l.min(x)));
            }
        }
    }
    (left, count)
}

#[test]
fn capture_pixels_show_text_and_indent_in_light_and_dark() {
    let mut v = View::from_markdown(SAMPLE_MD).unwrap();
    v.set_selected(100, 17); // 마지막 줄 선택 → 위쪽 줄은 선택 색 없이 배경만
    for kind in [Kind::Light, Kind::Dark] {
        let pal = palette_of(kind);
        let shot = render(&v, &pal, 720, 520, 96, "샘플 문서").unwrap();
        assert_eq!((shot.width, shot.height, shot.bgra.len()), (720, 520, 720 * 520 * 4));
        let m = Metrics::new(96);
        let non_bg = shot.bgra.chunks(4).filter(|p| !is_bg(p, pal.bg)).count();
        assert!(non_bg > 5_000, "{kind:?}: 비배경 화소가 너무 적음({non_bg}) — 빈 화면");
        let distinct: std::collections::HashSet<[u8; 3]> = shot.bgra.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
        assert!(distinct.len() > 8, "{kind:?}: 색이 거의 없음");
        // 줄 0(깊이 0), 줄 3(깊이 2: "TODO 부서별…" 줄 — 체크 칸 때문에 글자 시작이 더 오른쪽)
        // 안내선(왼쪽 세로선)이 들어가므로 불릿 위치 기준으로 비교: 깊이가 깊을수록 불릿 x 가 크다.
        // 안내선 폭(마커 x ± 1)을 피하려고 깊이 d 줄에서 마커 x+3 이후부터 찾는다.
        let mut prev_left = 0usize;
        for (slot, depth) in [(0usize, 0usize), (2, 1), (3, 2)] {
            let from = m.marker_x(depth) as usize - 4;
            let (left, count) = row_ink(&shot, pal.bg, &m, slot, from);
            assert!(count > 40, "{kind:?}: 줄 {slot} 에 글자가 그려지지 않음({count})");
            let left = left.unwrap();
            assert!(left > prev_left || slot == 0, "{kind:?}: 줄 {slot} 들여쓰기가 앞 줄보다 오른쪽이어야 함({left} <= {prev_left})");
            prev_left = left;
        }
        // 머리글 아래 목록 맨 끝(블록 없는 아래 빈 영역)은 배경색만 있다.
        let (left, count) = row_ink(&shot, pal.bg, &m, 15, 0);
        assert_eq!((left, count), (None, 0), "{kind:?}: 내용 아래 빈 줄은 배경색이어야 함");
    }
    // 밝게와 어둡게는 서로 다른 그림이어야 한다.
    let light = render(&v, &palette_of(Kind::Light), 720, 520, 96, "샘플 문서").unwrap();
    let dark = render(&v, &palette_of(Kind::Dark), 720, 520, 96, "샘플 문서").unwrap();
    assert_ne!(light.bgra, dark.bgra);
}

#[test]
fn capture_at_192_dpi_scales_geometry() {
    let v = View::from_markdown(SAMPLE_MD).unwrap();
    let pal = palette_of(Kind::Light);
    let shot = render(&v, &pal, 1440, 1040, 192, "샘플 문서").unwrap();
    let m = Metrics::new(192);
    assert_eq!(m.row_h, 56);
    let (_, count) = row_ink(&shot, pal.bg, &m, 0, 0);
    assert!(count > 160, "192dpi 첫 줄에 글자가 없음({count})");
}

#[test]
fn png_encoding_is_valid_and_nonblank() {
    let v = View::from_markdown(SAMPLE_MD).unwrap();
    let shot = render(&v, &palette_of(Kind::Light), 360, 260, 96, "샘플 문서").unwrap();
    let bytes = png::encode(shot.width, shot.height, &shot.bgra);
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&bytes[12..16], b"IHDR");
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 360);
    assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 260);
    assert!(bytes.len() > 360 * 260 * 3, "압축 없는 PNG 는 화소 수 이상의 크기");
    assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND");
}

#[test]
fn capture_cli_writes_png_files() {
    let exe = env!("CARGO_BIN_EXE_outliner_win");
    let dir = std::env::temp_dir().join(format!("outliner_win_cap_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (flag, name) in [(None, "light"), (Some("--dark"), "dark")] {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--capture-dir").arg(&dir);
        if let Some(f) = flag {
            cmd.arg(f);
        }
        assert!(cmd.status().unwrap().success());
        let file = dir.join(format!("outliner_{name}_96dpi.png"));
        let bytes = std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 720);
        assert!(bytes.len() > 720 * 520 * 3);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
