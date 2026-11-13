//! 줄 단위 영역 평균 축소(KCAP3). 원본을 위에서 아래로 한 줄씩 넣으면 썸네일 크기 누적 버퍼만으로 줄인다
//! — 원본 전체 해상도 버퍼가 필요 없다(PNG 줄 단위 풀기·클립보드 비트맵·WIC 줄 묶음 복사에 쓴다).
//!
//! 결과는 예전 `thumb::downscale`과 **같은 값**이다: 칸 경계 `x0=floor(tx*sw/tw)`, `x1=ceil((tx+1)*sw/tw)`
//! (세로도 같음, 이웃 칸과 한 줄·한 열을 함께 쓸 수 있음), 화소마다 흰 바탕 합성 `(c*a+255*(255-a))/255`
//! (불투명이면 그대로), 칸 평균은 버림 나눗셈. 순수 계산이라 따로 시험한다.

/// 원본 화소 배열 형식.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Gray,
    GrayA,
    Rgb,
    Rgba,
    Bgr,
    Bgra,
    /// 32비트 BGR, 넷째 바이트는 무시(불투명).
    Bgrx,
}

impl Layout {
    pub fn bytes(self) -> usize {
        match self {
            Layout::Gray => 1,
            Layout::GrayA => 2,
            Layout::Rgb | Layout::Bgr => 3,
            Layout::Rgba | Layout::Bgra | Layout::Bgrx => 4,
        }
    }
}

/// 한 칸(목표 화소)이 덮는 원본 범위 [a, b).
fn span(t: u32, src: u32, dst: u32) -> (u32, u32) {
    let a = (t as u64 * src as u64 / dst as u64) as u32;
    let b = (((t + 1) as u64 * src as u64).div_ceil(dst as u64) as u32).clamp(a + 1, src);
    (a, b)
}

pub struct BoxScaler {
    sw: u32,
    sh: u32,
    th: u32,
    layout: Layout,
    /// 목표 열마다 원본 열 범위.
    cols: Vec<(u32, u32)>,
    /// 지금 받는 원본 줄 번호.
    y: u32,
    /// 지금 채우는 목표 줄(ty)과 그 다음 줄(겹치는 원본 줄용)의 누적(B·G·R).
    acc: Vec<u32>,
    acc_next: Vec<u32>,
    ty: u32,
    /// 한 줄의 목표 열별 가로 합.
    row: Vec<u32>,
    out: Vec<u8>,
}

impl BoxScaler {
    /// 키우기(tw>sw·th>sh)와 0 크기는 거절.
    pub fn new(sw: u32, sh: u32, tw: u32, th: u32, layout: Layout) -> Option<Self> {
        if sw == 0 || sh == 0 || tw == 0 || th == 0 || tw > sw || th > sh {
            return None;
        }
        let n = tw as usize * 3;
        Some(BoxScaler {
            sw, sh, th, layout,
            cols: (0..tw).map(|t| span(t, sw, tw)).collect(),
            y: 0,
            acc: vec![0; n],
            acc_next: vec![0; n],
            ty: 0,
            row: vec![0; n],
            out: Vec::with_capacity(tw as usize * th as usize * 4),
        })
    }

    /// 원본 한 줄(위에서 아래 순서). 길이가 모자라거나 줄이 넘치면 false.
    pub fn push_row(&mut self, line: &[u8]) -> bool {
        if self.y >= self.sh || line.len() < self.sw as usize * self.layout.bytes() {
            return false;
        }
        match self.layout {
            Layout::Gray => self.sum_row::<1, 0, 0, 0, 255>(line),
            Layout::GrayA => self.sum_row::<2, 0, 0, 0, 1>(line),
            Layout::Rgb => self.sum_row::<3, 2, 1, 0, 255>(line),
            Layout::Rgba => self.sum_row::<4, 2, 1, 0, 3>(line),
            Layout::Bgr => self.sum_row::<3, 0, 1, 2, 255>(line),
            Layout::Bgra => self.sum_row::<4, 0, 1, 2, 3>(line),
            Layout::Bgrx => self.sum_row::<4, 0, 1, 2, 255>(line),
        }
        let y = self.y;
        // 이 원본 줄이 속한 목표 줄: 지금 줄(ty)과, 겹치면 다음 줄(ty+1).
        let (_, cur_end) = span(self.ty, self.sh, self.th);
        if y < cur_end {
            for (a, r) in self.acc.iter_mut().zip(&self.row) {
                *a += r;
            }
        }
        if self.ty + 1 < self.th {
            let (next_start, _) = span(self.ty + 1, self.sh, self.th);
            if y >= next_start {
                for (a, r) in self.acc_next.iter_mut().zip(&self.row) {
                    *a += r;
                }
            }
        }
        self.y += 1;
        // 지금 목표 줄이 끝났으면 내보내고 다음 줄로.
        while self.ty < self.th && self.y >= span(self.ty, self.sh, self.th).1 {
            self.emit();
        }
        true
    }

    /// 모든 줄을 받았으면 BGRA(알파 255) 썸네일.
    pub fn finish(self) -> Option<Vec<u8>> {
        (self.ty == self.th && self.y == self.sh).then_some(self.out)
    }

    fn emit(&mut self) {
        let (y0, y1) = span(self.ty, self.sh, self.th);
        let rows = y1 - y0;
        for (tx, &(x0, x1)) in self.cols.iter().enumerate() {
            let n = rows * (x1 - x0);
            let s = &self.acc[tx * 3..tx * 3 + 3];
            self.out.extend_from_slice(&[(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8, 255]);
        }
        std::mem::swap(&mut self.acc, &mut self.acc_next);
        self.acc_next.iter_mut().for_each(|a| *a = 0);
        self.ty += 1;
        // 새 지금 줄(옛 다음 줄)에는 겹치는 줄만 들어 있다. 한 원본 줄은 이웃한 두 목표 줄에만 속할 수
        // 있으므로(줄이기에서 칸 겹침은 이웃 칸 사이 한 줄뿐) 그 다음 줄 몫은 아직 없다.
    }

    /// 한 줄 가로 합. C=바이트 수, B·G·R=채널 위치, A=알파 위치(255면 불투명).
    #[inline(always)]
    fn sum_row<const C: usize, const B: usize, const G: usize, const R: usize, const A: usize>(&mut self, line: &[u8]) {
        for (tx, &(x0, x1)) in self.cols.iter().enumerate() {
            let (mut sb, mut sg, mut sr) = (0u32, 0u32, 0u32);
            let px = &line[x0 as usize * C..x1 as usize * C];
            for p in px.chunks_exact(C) {
                let (b, g, r) = (p[B] as u32, p[G] as u32, p[R] as u32);
                if A < C && p[A] != 255 {
                    let a = p[A] as u32;
                    let bg = 255 * (255 - a);
                    sb += (b * a + bg) / 255;
                    sg += (g * a + bg) / 255;
                    sr += (r * a + bg) / 255;
                } else {
                    sb += b;
                    sg += g;
                    sr += r;
                }
            }
            self.row[tx * 3] = sb;
            self.row[tx * 3 + 1] = sg;
            self.row[tx * 3 + 2] = sr;
        }
    }
}

/// 메모리에 다 있는 그림을 한 번에(줄 간격 `stride`). `bottom_up`이면 아래 줄부터 저장된 것(DIB).
pub fn scale(src: &[u8], sw: u32, sh: u32, stride: usize, layout: Layout, bottom_up: bool, tw: u32, th: u32) -> Option<Vec<u8>> {
    let line = sw as usize * layout.bytes();
    if stride < line {
        return None;
    }
    let need = stride.checked_mul(sh as usize - 1)?.checked_add(line)?;
    if src.len() < need {
        return None;
    }
    let mut s = BoxScaler::new(sw, sh, tw, th, layout)?;
    for i in 0..sh as usize {
        let y = if bottom_up { sh as usize - 1 - i } else { i };
        if !s.push_row(&src[y * stride..y * stride + line]) {
            return None;
        }
    }
    s.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 예전 `thumb::downscale`(화소마다 나눗셈, 칸마다 다시 훑기)를 그대로 옮긴 기준.
    fn reference(src: &[u8], sw: u32, sh: u32, channels: usize, tw: u32, th: u32) -> Vec<u8> {
        let mut out = vec![0u8; tw as usize * th as usize * 4];
        for ty in 0..th {
            let (y0, y1) = span(ty, sh, th);
            for tx in 0..tw {
                let (x0, x1) = span(tx, sw, tw);
                let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let o = (y * sw + x) as usize * channels;
                        let p = &src[o..o + channels];
                        let (pr, pg, pb, pa) = match channels {
                            1 => (p[0], p[0], p[0], 255),
                            2 => (p[0], p[0], p[0], p[1]),
                            3 => (p[0], p[1], p[2], 255),
                            _ => (p[0], p[1], p[2], p[3]),
                        };
                        let a = pa as u64;
                        r += (pr as u64 * a + 255 * (255 - a)) / 255;
                        g += (pg as u64 * a + 255 * (255 - a)) / 255;
                        b += (pb as u64 * a + 255 * (255 - a)) / 255;
                        n += 1;
                    }
                }
                let o = (ty as usize * tw as usize + tx as usize) * 4;
                out[o..o + 4].copy_from_slice(&[(b / n) as u8, (g / n) as u8, (r / n) as u8, 255]);
            }
        }
        out
    }

    fn noise(n: usize, mut seed: u32) -> Vec<u8> {
        (0..n).map(|_| { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); (seed >> 24) as u8 }).collect()
    }

    #[test]
    fn same_as_old_downscale_for_all_layouts_and_odd_sizes() {
        let sizes = [(7, 5, 3, 2), (1920, 1080, 320, 180), (1000, 3, 320, 1), (5, 5, 5, 5), (641, 359, 320, 179), (3, 9, 1, 3)];
        for &(sw, sh, tw, th) in &sizes {
            for (layout, ch) in [(Layout::Gray, 1), (Layout::GrayA, 2), (Layout::Rgb, 3), (Layout::Rgba, 4)] {
                let src = noise(sw as usize * sh as usize * ch, sw * 31 + ch as u32);
                let got = scale(&src, sw, sh, sw as usize * ch, layout, false, tw, th).unwrap();
                assert_eq!(got, reference(&src, sw, sh, ch, tw, th), "{sw}×{sh}→{tw}×{th} {layout:?}");
            }
        }
    }

    #[test]
    fn bgr_orders_bottom_up_and_stride() {
        // 2×2 BGRX, 아래에서 위로, 줄 끝 여백 4바이트: 아래 줄(먼저 저장) 파랑, 위 줄 빨강.
        let src = [255, 0, 0, 9, 255, 0, 0, 9, 0, 0, 0, 0, 0, 0, 255, 9, 0, 0, 255, 9, 0, 0, 0, 0];
        let t = scale(&src, 2, 2, 12, Layout::Bgrx, true, 1, 2).unwrap();
        assert_eq!(t, [0, 0, 255, 255, 255, 0, 0, 255], "위 빨강, 아래 파랑");
        // BGRA 반투명 검정 → 흰 바탕 합성
        assert_eq!(scale(&[0, 0, 0, 0], 1, 1, 4, Layout::Bgra, false, 1, 1).unwrap(), [255, 255, 255, 255]);
        assert_eq!(scale(&[10, 20, 30], 1, 1, 3, Layout::Bgr, false, 1, 1).unwrap(), [10, 20, 30, 255]);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(scale(&[0; 3], 2, 2, 6, Layout::Rgb, false, 1, 1).is_none(), "짧은 버퍼");
        assert!(scale(&[0; 12], 2, 2, 5, Layout::Rgb, false, 1, 1).is_none(), "줄 간격이 한 줄보다 짧음");
        assert!(BoxScaler::new(2, 2, 3, 1, Layout::Rgb).is_none(), "키우기");
        let mut s = BoxScaler::new(2, 2, 1, 1, Layout::Rgb).unwrap();
        assert!(s.push_row(&[0; 6]));
        assert!(!s.push_row(&[0; 5]), "짧은 줄");
        assert!(s.push_row(&[0; 6]));
        assert!(!s.push_row(&[0; 6]), "줄 넘침");
        assert!(BoxScaler::new(2, 2, 1, 1, Layout::Rgb).unwrap().finish().is_none(), "줄 모자람");
    }
}
