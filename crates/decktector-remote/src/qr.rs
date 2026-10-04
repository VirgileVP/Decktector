//! QR code for the remote view URL.
//!
//! Rendered Rust-side rather than pulled in as an npm dependency: the
//! renderer bundle stays as it is, and the SVG string drops straight into
//! the settings panel — the CSP already allows `img-src ... data:`, so it
//! inlines as a `data:` URL in both the webview and a plain browser.

use qrcode::render::svg;
use qrcode::QrCode;

/// Side length, in modules, the SVG is scaled to — legible at the settings
/// panel's size without dominating it.
const SVG_DIMENSION: u32 = 200;

/// Renders `url` as a QR code, as an inline SVG string.
pub fn qr_svg(url: &str) -> anyhow::Result<String> {
    let code = QrCode::new(url)?;
    Ok(code
        .render()
        .min_dimensions(SVG_DIMENSION, SVG_DIMENSION)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_renders_to_an_svg_string() {
        let svg = qr_svg("http://192.168.1.1:7420/?token=abcd1234ef567890").expect("renders");
        assert!(svg.contains("<svg"));
    }
}
