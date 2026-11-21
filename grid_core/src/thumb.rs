//! 썸네일 크기 슬라이더 범위(`docfit_app/thumb_size_setting.py` 와 같다).
pub const MIN_THUMB: f64 = 100.0;
pub const MAX_THUMB: f64 = 240.0;

/// `_clamp` 와 같다: NaN 이면 기본값, 아니면 100~240 으로 맞춘다.
pub fn clamp_thumb(value: f64, default: f64) -> f64 {
    if value.is_nan() { default } else { value.clamp(MIN_THUMB, MAX_THUMB) }
}
