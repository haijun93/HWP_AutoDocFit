//! 화소 계산(창·OS 와 무관): 방향 보정·회전·자르기·밝기·대비, 그리고 dHash 와 비슷한 사진 묶기.
//! 계산 규칙은 앱 화면의 Python 구현(`photo_album_service.edit_copy`·`find_duplicates`, Pillow)을 따른다.

/// 위에서 아래로의 BGRA 그림.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Image {
    pub fn new(width: u32, height: u32, bgra: Vec<u8>) -> Option<Image> {
        (width > 0 && height > 0 && bgra.len() == width as usize * height as usize * 4).then_some(Image { width, height, bgra })
    }

    /// 새 크기의 각 화소가 원본 어디서 오는지로 다시 배치한다.
    fn remap(&self, width: u32, height: u32, source: impl Fn(u32, u32) -> (u32, u32)) -> Image {
        let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                let (sx, sy) = source(x, y);
                let at = (sy as usize * self.width as usize + sx as usize) * 4;
                bgra.extend_from_slice(&self.bgra[at..at + 4]);
            }
        }
        Image { width, height, bgra }
    }

    /// EXIF Orientation(1~8)을 정방향으로 편다. 그 밖의 값은 그대로 둔다.
    pub fn oriented(self, orientation: u16) -> Image {
        let (w, h) = (self.width, self.height);
        match orientation {
            2 => self.remap(w, h, |x, y| (w - 1 - x, y)),
            3 => self.remap(w, h, |x, y| (w - 1 - x, h - 1 - y)),
            4 => self.remap(w, h, |x, y| (x, h - 1 - y)),
            5 => self.remap(h, w, |x, y| (y, x)),
            6 => self.remap(h, w, |x, y| (y, h - 1 - x)),
            7 => self.remap(h, w, |x, y| (w - 1 - y, h - 1 - x)),
            8 => self.remap(h, w, |x, y| (w - 1 - y, x)),
            _ => self,
        }
    }

    /// 시계 방향 90° 단위 회전.
    pub fn rotated(self, quarter_turns: u32) -> Image {
        match quarter_turns % 4 {
            1 => self.oriented(6),
            2 => self.oriented(3),
            3 => self.oriented(8),
            _ => self,
        }
    }

    /// (왼, 위, 오른, 아래)를 그림 안으로 맞춰 자른다. 남는 영역이 없으면 None.
    pub fn cropped(&self, crop: [i64; 4]) -> Option<Image> {
        let clamp = |v: i64, max: u32| v.clamp(0, max as i64) as u32;
        let (l, t) = (clamp(crop[0], self.width), clamp(crop[1], self.height));
        let (r, b) = (clamp(crop[2], self.width), clamp(crop[3], self.height));
        if r <= l || b <= t {
            return None;
        }
        Some(self.remap(r - l, b - t, |x, y| (x + l, y + t)))
    }

    /// Pillow `ImageEnhance.Brightness`: 검정과 섞는다(알파는 그대로).
    pub fn brighten(&mut self, factor: f64) {
        self.blend_from(0.0, factor);
    }

    /// Pillow `ImageEnhance.Contrast`: 전체 밝기 평균 회색과 섞는다.
    pub fn contrast(&mut self, factor: f64) {
        let pixels = self.width as u64 * self.height as u64;
        let sum: u64 = self.bgra.chunks_exact(4).map(|p| luma(p) as u64).sum();
        let mean = (sum as f64 / pixels as f64 + 0.5).floor();
        self.blend_from(mean, factor);
    }

    fn blend_from(&mut self, base: f64, factor: f64) {
        for pixel in self.bgra.chunks_exact_mut(4) {
            for v in &mut pixel[..3] {
                *v = (base + factor * (*v as f64 - base)).clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// 회색조(Pillow `convert("L")` 의 ITU-R 601 계수).
    pub fn gray(&self) -> Vec<u8> {
        self.bgra.chunks_exact(4).map(luma).collect()
    }
}

fn luma(bgra: &[u8]) -> u8 {
    ((bgra[2] as u32 * 19595 + bgra[1] as u32 * 38470 + bgra[0] as u32 * 7471 + 0x8000) >> 16) as u8
}

/// 한 방향 Lanczos3 축소·확대(Pillow `Image.Resampling.LANCZOS` 와 같은 창 규칙).
fn resample_line(src: &[f64], out: usize) -> Vec<f64> {
    let scale = src.len() as f64 / out as f64;
    let filter_scale = scale.max(1.0);
    let support = 3.0 * filter_scale;
    let sinc = |x: f64| if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
    (0..out)
        .map(|i| {
            let center = (i as f64 + 0.5) * scale;
            let from = ((center - support + 0.5).floor().max(0.0)) as usize;
            let to = ((center + support + 0.5).floor() as usize).min(src.len());
            let (mut sum, mut total) = (0.0, 0.0);
            for (j, value) in src.iter().enumerate().take(to).skip(from) {
                let x = (j as f64 - center + 0.5) / filter_scale;
                let weight = if x.abs() < 3.0 { sinc(x) * sinc(x / 3.0) } else { 0.0 };
                sum += weight * value;
                total += weight;
            }
            if total == 0.0 { 0.0 } else { sum / total }
        })
        .collect()
}

/// 회색조 그림을 9x8 로 줄인다.
pub fn shrink_9x8(gray: &[u8], width: usize, height: usize) -> [u8; 72] {
    let rows: Vec<Vec<f64>> = (0..height)
        .map(|y| {
            let line: Vec<f64> = gray[y * width..(y + 1) * width].iter().map(|v| *v as f64).collect();
            resample_line(&line, 9).into_iter().map(|v| v.round().clamp(0.0, 255.0)).collect()
        })
        .collect();
    let mut out = [0u8; 72];
    for x in 0..9 {
        let column: Vec<f64> = rows.iter().map(|r| r[x]).collect();
        for (y, v) in resample_line(&column, 8).into_iter().enumerate() {
            out[y * 9 + x] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// 64비트 차이 해시: 9x8 에서 왼쪽 화소가 오른쪽보다 밝으면 1.
pub fn dhash(small: &[u8; 72]) -> u64 {
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            hash = (hash << 1) | u64::from(small[y * 9 + x] > small[y * 9 + x + 1]);
        }
    }
    hash
}

/// 비슷한 사진으로 보는 해시 거리(앱 화면 기본값).
pub const THRESHOLD: u32 = 6;

#[derive(Clone, Debug)]
pub struct Hashed {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub hash: u64,
}

/// 거리 `threshold` 이하로 이어지는 사진 묶음(2장 이상). 장수가 많은 묶음부터,
/// 묶음 안에서는 대표(가장 큰 파일)와 가까운 것부터. 반환값은 `items` 의 번호.
pub fn duplicate_groups(items: &[Hashed], threshold: u32) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..items.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            if (items[i].hash ^ items[j].hash).count_ones() <= threshold {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a] = b;
                }
            }
        }
    }
    let mut clusters = std::collections::BTreeMap::<usize, Vec<usize>>::new();
    for i in 0..items.len() {
        let root = find(&mut parent, i);
        clusters.entry(root).or_default().push(i);
    }
    let representative = |group: &[usize]| {
        *group.iter().max_by_key(|&&i| (items[i].size, std::cmp::Reverse(i))).expect("묶음은 비지 않는다")
    };
    let mut groups: Vec<(usize, Vec<usize>)> = clusters
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            let rep = representative(&g);
            g.sort_by_key(|&i| ((items[i].hash ^ items[rep].hash).count_ones(), items[i].name.clone()));
            (rep, g)
        })
        .collect();
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| items[a.0].path.cmp(&items[b.0].path)));
    groups.into_iter().map(|(_, g)| g).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 파란 채널에 번호를 담은 w x h 그림.
    fn numbered(w: u32, h: u32) -> Image {
        let bgra = (0..w * h).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        Image::new(w, h, bgra).unwrap()
    }
    fn ids(image: &Image) -> Vec<u8> {
        image.bgra.chunks_exact(4).map(|p| p[0]).collect()
    }

    #[test]
    fn exif_orientations_and_rotation_move_pixels_like_pillow() {
        // 원본 3x2:  0 1 2 / 3 4 5
        let expected: [(u16, u32, &[u8]); 8] = [
            (1, 3, &[0, 1, 2, 3, 4, 5]),
            (2, 3, &[2, 1, 0, 5, 4, 3]),
            (3, 3, &[5, 4, 3, 2, 1, 0]),
            (4, 3, &[3, 4, 5, 0, 1, 2]),
            (5, 2, &[0, 3, 1, 4, 2, 5]),
            (6, 2, &[3, 0, 4, 1, 5, 2]),
            (7, 2, &[5, 2, 4, 1, 3, 0]),
            (8, 2, &[2, 5, 1, 4, 0, 3]),
        ];
        for (orientation, width, pixels) in expected {
            let got = numbered(3, 2).oriented(orientation);
            assert_eq!((got.width, ids(&got)), (width, pixels.to_vec()), "orientation {orientation}");
        }
        assert_eq!(ids(&numbered(3, 2).rotated(1)), [3, 0, 4, 1, 5, 2], "시계 방향 90°");
        assert_eq!(ids(&numbered(3, 2).rotated(4)), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn crop_is_clamped_and_empty_area_is_rejected() {
        let image = numbered(3, 2);
        assert_eq!(ids(&image.cropped([1, 0, 99, 99]).unwrap()), [1, 2, 4, 5]);
        assert!(image.cropped([2, 0, 2, 2]).is_none());
        assert!(image.cropped([9, 9, 12, 12]).is_none());
    }

    #[test]
    fn brightness_and_contrast_follow_pillow_blend() {
        let mut image = Image::new(2, 1, vec![100, 100, 100, 7, 200, 200, 200, 7]).unwrap();
        image.brighten(1.5);
        assert_eq!(image.bgra, [150, 150, 150, 7, 255, 255, 255, 7], "알파는 그대로");
        let mut image = Image::new(2, 1, vec![100, 100, 100, 255, 200, 200, 200, 255]).unwrap();
        image.contrast(2.0);
        assert_eq!(image.bgra, [50, 50, 50, 255, 250, 250, 250, 255]);
    }

    #[test]
    fn dhash_marks_falling_brightness_only() {
        let width = 90;
        let falling: Vec<u8> = (0..width * 80).map(|i| 250 - (i % width) as u8 * 2).collect();
        let rising: Vec<u8> = (0..width * 80).map(|i| (i % width) as u8 * 2).collect();
        assert_eq!(dhash(&shrink_9x8(&falling, width, 80)), u64::MAX);
        assert_eq!(dhash(&shrink_9x8(&rising, width, 80)), 0);
        assert_eq!(shrink_9x8(&vec![77; width * 80], width, 80), [77u8; 72]);
    }

    #[test]
    fn similar_hashes_group_and_largest_file_leads_its_group() {
        let item = |name: &str, size: u64, hash: u64| Hashed { path: format!(r"C:\a\{name}"), name: name.into(), size, hash };
        let items = [
            item("small.jpg", 10, 0b0000),
            item("alone.jpg", 99, u64::MAX),
            item("large.jpg", 90, 0b0111),
            item("x.jpg", 5, 0xFFFF_0000_0000),
            item("y.jpg", 6, 0xFFFF_0000_0000),
        ];
        let groups = duplicate_groups(&items, THRESHOLD);
        assert_eq!(groups, [vec![2, 0], vec![3, 4]], "거리가 같으면 이름 순");
        assert!(duplicate_groups(&items, 0).len() == 1, "거리 0 이면 똑같은 것만");
    }
}
