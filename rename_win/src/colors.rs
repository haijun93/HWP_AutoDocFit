//! 파일명 변경 창의 색. 바탕·글자·테두리는 `ui_theme` 팔레트(밝게·어둡게·고대비)에서 가져오고,
//! 상태 글자색(변경 예정·이름 오류·충돌)만 이 창에서 테마별로 정한다. 모두 COLORREF(BGR). 순수 계산이라 모든 OS에서 시험한다.
use ui_theme::{Kind, Palette, rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    /// 창 바탕.
    pub window: u32,
    /// 목록·입력 칸 바탕.
    pub list: u32,
    /// 목록 머리글 바탕.
    pub header: u32,
    pub border: u32,
    pub title: u32,
    pub ink: u32,
    pub muted: u32,
    pub button: u32,
    pub button_hover: u32,
    pub button_down: u32,
    pub button_text: u32,
    pub button_off_text: u32,
    pub accent: u32,
    /// 변경 예정.
    pub ok: u32,
    /// 이름 오류.
    pub warn: u32,
    /// 충돌·입력 오류.
    pub bad: u32,
    /// 정규식 단추를 켠 상태의 표시와 선택 줄.
    pub selected: u32,
}

impl Colors {
    pub fn of(p: &Palette, kind: Kind) -> Colors {
        let (ok, warn, bad) = match kind {
            Kind::Light => (rgb(30, 110, 60), rgb(170, 90, 0), rgb(190, 40, 40)),
            Kind::Dark => (rgb(110, 200, 140), rgb(240, 170, 80), rgb(255, 130, 130)),
            // 고대비: 색으로 구분하지 않고 시스템 글자색 하나(문구가 뜻을 전한다)
            Kind::HighContrast => (p.text, p.text, p.text),
        };
        Colors {
            window: p.tab_bg,
            list: p.bg,
            header: p.head_bg,
            border: p.border,
            title: p.text,
            ink: p.text,
            muted: p.text_muted,
            button: p.head_bg,
            button_hover: p.head_button_hover,
            button_down: p.head_button_pressed,
            button_text: p.text,
            button_off_text: p.text_faint,
            accent: p.accent,
            ok,
            warn,
            bad,
            selected: p.selected,
        }
    }

    /// (이름, 글자색, 바탕색) — 실제로 겹쳐 그리는 쌍. 사용 중인 쌍만 시험한다.
    pub fn text_pairs(&self) -> Vec<(&'static str, u32, u32)> {
        vec![
            ("제목/창", self.title, self.window),
            ("본문/창", self.ink, self.window),
            ("안내/창", self.muted, self.window),
            ("본문/목록", self.ink, self.list),
            ("안내/목록", self.muted, self.list),
            ("머리글/머리글", self.ink, self.header),
            ("변경 예정/목록", self.ok, self.list),
            ("이름 오류/목록", self.warn, self.list),
            ("충돌/목록", self.bad, self.list),
            ("입력 오류/창", self.bad, self.window),
            ("단추/단추", self.button_text, self.button),
            ("단추/올림", self.button_text, self.button_hover),
            ("단추/누름", self.button_text, self.button_down),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_theme::{SysColors, contrast};

    #[test]
    fn all_used_text_pairs_reach_wcag_aa_in_light_and_dark() {
        for (kind, name) in [(Kind::Light, "밝게"), (Kind::Dark, "어둡게")] {
            let c = Colors::of(&Palette::of(kind, &SysColors::default()), kind);
            for (what, fg, bg) in c.text_pairs() {
                let r = contrast(fg, bg);
                assert!(r >= 4.5, "{name} {what}: 대비 {r:.2} < 4.5");
            }
        }
    }

    #[test]
    fn dark_and_light_really_differ_and_follow_the_palette() {
        let l = Colors::of(&Palette::LIGHT, Kind::Light);
        let d = Colors::of(&Palette::DARK, Kind::Dark);
        assert_ne!(l.window, d.window);
        assert!(ui_theme::luminance(d.window) < 0.1 && ui_theme::luminance(l.window) > 0.9);
        assert_eq!(d.list, Palette::DARK.bg);
        assert_eq!(l.ink, Palette::LIGHT.text);
    }

    #[test]
    fn high_contrast_uses_system_colors_only() {
        let sys = SysColors {
            window: 0x000000,
            window_text: 0xFFFFFF,
            highlight: 0xFFFF00,
            highlight_text: 0x000000,
            btn_face: 0x000000,
            gray_text: 0x00FF00,
            hotlight: 0xFFFF00,
        };
        let c = Colors::of(&Palette::of(Kind::HighContrast, &sys), Kind::HighContrast);
        assert_eq!((c.ok, c.warn, c.bad, c.ink), (0xFFFFFF, 0xFFFFFF, 0xFFFFFF, 0xFFFFFF));
        assert_eq!(c.list, 0x000000);
    }
}
