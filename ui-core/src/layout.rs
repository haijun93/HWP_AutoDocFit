//! 레이아웃(순수 함수): 행·열 나누기, 여백, 간격. 모두 실제 px로 받고 실제 px로 돌려준다
//! (논리 px는 [`crate::geom::scale`]로 먼저 바꾼다).

use crate::geom::Rect;

/// 한 칸의 크기 규칙.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// 고정 px.
    Px(i32),
    /// 남은 공간을 비율로 나눔(가중치).
    Fill(u32),
}

/// 길이 `total`을 `sizes`대로 나눈 (시작, 끝) 목록. 칸 사이 간격 `gap`.
/// 고정 칸을 먼저 주고 남는 공간을 Fill 가중치로 나눈다(나머지 px는 앞 칸부터 1px씩). 모자라면 Fill은 0.
pub fn split(start: i32, total: i32, sizes: &[Size], gap: i32) -> Vec<(i32, i32)> {
    let n = sizes.len() as i32;
    if n == 0 {
        return Vec::new();
    }
    let gaps = gap.max(0) * (n - 1);
    let fixed: i32 = sizes.iter().map(|s| if let Size::Px(p) = s { (*p).max(0) } else { 0 }).sum();
    let weights: u32 = sizes.iter().map(|s| if let Size::Fill(w) = s { *w } else { 0 }).sum();
    let free = (total - fixed - gaps).max(0);
    let mut fill_sizes: Vec<i32> = Vec::new();
    if weights > 0 {
        let mut given = 0;
        for s in sizes {
            if let Size::Fill(w) = s {
                let v = (free as i64 * *w as i64 / weights as i64) as i32;
                fill_sizes.push(v);
                given += v;
            }
        }
        let mut rest = free - given;
        for v in fill_sizes.iter_mut() {
            if rest == 0 {
                break;
            }
            *v += 1;
            rest -= 1;
        }
    }
    let mut fi = 0;
    let mut x = start;
    let mut out = Vec::with_capacity(sizes.len());
    for (i, s) in sizes.iter().enumerate() {
        let len = match s {
            Size::Px(p) => (*p).max(0),
            Size::Fill(_) => {
                fi += 1;
                fill_sizes[fi - 1]
            }
        };
        out.push((x, x + len));
        x += len;
        if (i as i32) < n - 1 {
            x += gap.max(0);
        }
    }
    out
}

/// 가로로 나눈 칸들(행).
pub fn row(r: Rect, sizes: &[Size], gap: i32) -> Vec<Rect> {
    split(r.left, r.width(), sizes, gap).into_iter().map(|(l, rr)| Rect::new(l, r.top, rr, r.bottom)).collect()
}

/// 세로로 나눈 칸들(열).
pub fn column(r: Rect, sizes: &[Size], gap: i32) -> Vec<Rect> {
    split(r.top, r.height(), sizes, gap).into_iter().map(|(t, b)| Rect::new(r.left, t, r.right, b)).collect()
}

/// 위에서 h만큼 떼어 (위, 나머지).
pub fn take_top(r: Rect, h: i32) -> (Rect, Rect) {
    let y = (r.top + h.max(0)).min(r.bottom);
    (Rect::new(r.left, r.top, r.right, y), Rect::new(r.left, y, r.right, r.bottom))
}

/// 아래에서 h만큼 떼어 (나머지, 아래).
pub fn take_bottom(r: Rect, h: i32) -> (Rect, Rect) {
    let y = (r.bottom - h.max(0)).max(r.top);
    (Rect::new(r.left, r.top, r.right, y), Rect::new(r.left, y, r.right, r.bottom))
}

/// 오른쪽에서 w만큼 떼어 (나머지, 오른쪽).
pub fn take_right(r: Rect, w: i32) -> (Rect, Rect) {
    let x = (r.right - w.max(0)).max(r.left);
    (Rect::new(r.left, r.top, x, r.bottom), Rect::new(x, r.top, r.right, r.bottom))
}

/// 같은 높이 줄을 위에서부터 쌓는다(설정 폼·목록 머리 등).
pub fn stack(r: Rect, count: usize, row_h: i32, gap: i32) -> Vec<Rect> {
    (0..count)
        .map(|i| {
            let top = r.top + i as i32 * (row_h + gap);
            Rect::new(r.left, top, r.right, top + row_h)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Insets;

    #[test]
    fn split_fixed_fill_and_gap() {
        assert_eq!(split(0, 100, &[Size::Px(20), Size::Fill(1), Size::Px(30)], 0), vec![(0, 20), (20, 70), (70, 100)]);
        assert_eq!(split(10, 100, &[Size::Fill(1), Size::Fill(1), Size::Fill(1)], 5), vec![(10, 40), (45, 75), (80, 110)]);
        // 가중치 2:1, 나머지 px는 앞 칸부터.
        assert_eq!(split(0, 10, &[Size::Fill(2), Size::Fill(1)], 0), vec![(0, 7), (7, 10)]);
        // 공간이 모자라면 Fill은 0, 고정은 그대로.
        assert_eq!(split(0, 30, &[Size::Px(20), Size::Fill(1), Size::Px(20)], 0), vec![(0, 20), (20, 20), (20, 40)]);
        assert!(split(0, 100, &[], 4).is_empty());
    }

    #[test]
    fn rows_columns_and_takes() {
        let r = Rect::new(0, 0, 200, 100).inset(Insets::all(10));
        let cols = row(r, &[Size::Px(50), Size::Fill(1)], 10);
        assert_eq!(cols, vec![Rect::new(10, 10, 60, 90), Rect::new(70, 10, 190, 90)]);
        let rows = column(r, &[Size::Px(30), Size::Fill(1)], 0);
        assert_eq!(rows[0], Rect::new(10, 10, 190, 40));
        assert_eq!(rows[1], Rect::new(10, 40, 190, 90));
        assert_eq!(take_top(r, 500).1.height(), 0);
        assert_eq!(take_bottom(r, 20).1, Rect::new(10, 70, 190, 90));
        assert_eq!(take_right(r, 30).1, Rect::new(160, 10, 190, 90));
        let s = stack(r, 3, 20, 4);
        assert_eq!(s[2], Rect::new(10, 58, 190, 78));
    }
}
