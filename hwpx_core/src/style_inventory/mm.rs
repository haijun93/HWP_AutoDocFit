/// Convert HWP units to millimeters, rounded to one decimal like Python round.
pub fn mm(value: Option<&str>) -> f64 {
    let Some(value) = value else { return 0.0 };
    let Ok(number) = value.trim().parse::<f64>() else { return 0.0 };
    let scaled = number / (7200.0 / 25.4) * 10.0;
    // Python round uses ties-to-even; Rust's round uses ties-away-from-zero.
    let floor = scaled.floor();
    let fraction = scaled - floor;
    let rounded = if (fraction - 0.5).abs() < 1e-12 {
        if floor as i64 % 2 == 0 { floor } else { floor + 1.0 }
    } else {
        scaled.round()
    };
    rounded / 10.0
}

#[cfg(test)]
mod tests {
    use super::mm;

    #[test]
    fn converts_and_defaults_invalid_input() {
        assert_eq!(mm(Some("7200")), 25.4);
        assert_eq!(mm(Some("invalid")), 0.0);
        assert_eq!(mm(None), 0.0);
    }
}
