//! Direct2D 자리(Q15): **캔버스 창만** Direct2D, 나머지는 GDI. 한 창 안에서 GDI 글자와 DirectWrite 글자를 섞지 않는다.
//! 구현은 첫 Direct2D 창(UI7 S14 표 단위)에서 `windows` 크레이트 D2D·DWrite 기능으로 넣고,
//! 그때 GDI 대비 첫 그리기 ms·개인 메모리·exe 크기를 1회 측정해 카드에 남긴다. 지금은 선택 규칙만 둔다.

/// 그리기 방식.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Gdi,
    Direct2D,
}

/// 창 성격.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// 글·목록·폼·탭·메뉴·표.
    Widgets,
    /// 확대 변환·안티앨리어싱 선·많은 도형(S14 격자, S15 표그림판, 필요 시 S3).
    Canvas,
}

/// 창 단위 선택 규칙(순수 함수). Direct2D가 아직 없으면 캔버스도 GDI.
pub fn backend_for(surface: Surface, d2d_available: bool) -> Backend {
    match surface {
        Surface::Canvas if d2d_available => Backend::Direct2D,
        _ => Backend::Gdi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_canvas_windows_use_direct2d() {
        assert_eq!(backend_for(Surface::Widgets, true), Backend::Gdi);
        assert_eq!(backend_for(Surface::Canvas, true), Backend::Direct2D);
        assert_eq!(backend_for(Surface::Canvas, false), Backend::Gdi);
    }
}
