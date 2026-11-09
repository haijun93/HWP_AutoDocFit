//! Excel CF_HTML target conversion. Conservative initial implementation: validated inputs
//! are returned byte-for-byte unchanged until parity coverage enables transformation.
//! The caller's Python app-service fallback remains authoritative.
pub const MAX_HTML_BYTES: usize = 8 * 1024 * 1024;

pub fn convert(html: &[u8], _shading: &str) -> Vec<u8> {
    if html.len() > MAX_HTML_BYTES || std::str::from_utf8(html).is_err() {
        return html.to_vec();
    }
    // No HTML mutation is enabled until the regression suite reports zero byte differences.
    html.to_vec()
}
