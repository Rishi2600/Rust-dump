use std::path::Path;

/// The `Content-Type` for a file, judged by its extension.
///
/// A response is only bytes. This header is what tells a browser whether to
/// render them as a page, apply them as a stylesheet, show them as an image
/// or offer them as a download.
pub fn mime_type(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|extension| extension.to_str());
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        Some("json") => "application/json",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("pdf") => "application/pdf",
        Some("wasm") => "application/wasm",
        Some("woff2") => "font/woff2",
        Some("zip") => "application/zip",
        // "Some bytes": a browser will download this instead of guessing.
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_extensions_have_content_types() {
        let type_of = |name: &str| mime_type(Path::new(name));
        assert_eq!(type_of("index.html"), "text/html; charset=utf-8");
        assert_eq!(type_of("css/site.css"), "text/css; charset=utf-8");
        assert_eq!(type_of("app.js"), "text/javascript; charset=utf-8");
        assert_eq!(type_of("photo.JPG"), "image/jpeg");
        assert_eq!(type_of("data.json"), "application/json");
        // Only the last extension counts.
        assert_eq!(type_of("archive.tar.zip"), "application/zip");
    }

    #[test]
    fn anything_unrecognised_is_plain_bytes() {
        let type_of = |name: &str| mime_type(Path::new(name));
        assert_eq!(type_of("program.exe"), "application/octet-stream");
        assert_eq!(type_of("Makefile"), "application/octet-stream");
        assert_eq!(type_of(".html"), "application/octet-stream");
    }
}
