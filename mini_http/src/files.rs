use std::fs;
use std::path::{Path, PathBuf};

use crate::method::Method;
use crate::mime::mime_type;
use crate::percent::percent_encode;
use crate::request::Request;
use crate::response::Response;
use crate::service::Service;
use crate::status::StatusCode;

/// Serves the files under one directory, and nothing outside it.
pub struct StaticFiles {
    root: PathBuf,
    listing: bool,
}

impl StaticFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            listing: false,
        }
    }

    /// Whether a directory without an `index.html` shows a list of its files.
    pub fn with_listing(mut self, listing: bool) -> Self {
        self.listing = listing;
        self
    }

    /// Find what a request path refers to on disk, or `None` if it does not
    /// exist or must not be served.
    ///
    /// The path arrives already normalised, so it holds no `..`. That is not
    /// relied on here: this function is the last thing between a URL and the
    /// file system, and it checks for itself.
    pub fn locate(&self, path: &str) -> Option<PathBuf> {
        let mut candidate = self.root.clone();
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            // Names starting with a dot are hidden files such as `.git` or
            // `.env`, which tend to hold exactly what should not be public.
            if segment.starts_with('.') || segment.contains(['\\', ':']) {
                return None;
            }
            candidate.push(segment);
        }
        // A symbolic link inside the root can point anywhere. Resolving both
        // paths fully and comparing them catches a link that leads outside.
        let root = self.root.canonicalize().ok()?;
        let resolved = candidate.canonicalize().ok()?;
        resolved.starts_with(&root).then_some(resolved)
    }

    /// Answer a request for `path` from the files under the root.
    pub fn serve(&self, request: &Request, path: &str) -> Response {
        if !matches!(request.method, Method::Get | Method::Head) {
            return Response::error(StatusCode::METHOD_NOT_ALLOWED).header("Allow", "GET, HEAD");
        }
        match self.locate(path) {
            Some(found) if found.is_file() => self.send_file(&found),
            Some(found) if found.is_dir() => self.send_directory(path, &found),
            _ => Response::error(StatusCode::NOT_FOUND),
        }
    }

    fn send_file(&self, file: &Path) -> Response {
        match fs::read(file) {
            Ok(contents) => Response::new(StatusCode::OK)
                .header("Content-Type", mime_type(file))
                .body(contents),
            // It was there a moment ago. Gone or unreadable now is the same
            // to the client as never having existed.
            Err(_) => Response::error(StatusCode::NOT_FOUND),
        }
    }
}

impl StaticFiles {
    fn send_directory(&self, path: &str, directory: &Path) -> Response {
        // A page's relative links are resolved against its URL. Seen from
        // `/docs`, a link to `guide.txt` means `/guide.txt`; seen from
        // `/docs/` it means `/docs/guide.txt`. So a directory is only ever
        // served under the name that ends in a slash.
        if !path.ends_with('/') {
            let encoded: Vec<String> = path.split('/').map(percent_encode).collect();
            let location = format!("{}/", encoded.join("/"));
            return Response::redirect(StatusCode::MOVED_PERMANENTLY, &location);
        }
        let index = directory.join("index.html");
        if index.is_file() {
            return self.send_file(&index);
        }
        if !self.listing {
            return Response::error(StatusCode::FORBIDDEN);
        }
        match list_directory(path, directory) {
            Ok(page) => Response::html(StatusCode::OK, page),
            Err(_) => Response::error(StatusCode::NOT_FOUND),
        }
    }
}

/// Build a page linking to everything in a directory.
fn list_directory(path: &str, directory: &Path) -> std::io::Result<String> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            entries.push((name, entry.path().is_dir()));
        }
    }
    entries.sort();

    let title = escape_html(path);
    let mut page = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>Index of {title}</title></head>\n\
         <body>\n<h1>Index of {title}</h1>\n<ul>\n"
    );
    if path != "/" {
        page.push_str("<li><a href=\"../\">../</a></li>\n");
    }
    for (name, is_dir) in entries {
        let slash = if is_dir { "/" } else { "" };
        // The name is written twice, into two different languages, and each
        // needs its own escaping: percent-encoding where it is a URL, and
        // HTML escaping where it is text.
        let (link, label) = (percent_encode(&name), escape_html(&name));
        page.push_str(&format!(
            "<li><a href=\"{link}{slash}\">{label}{slash}</a></li>\n"
        ));
    }
    page.push_str("</ul>\n</body></html>\n");
    Ok(page)
}

/// Make text safe to place inside an HTML page.
///
/// A file can be called `<script>...`. Written into a page as it is, that
/// name would run as code in the browser of whoever views the listing.
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

impl Service for StaticFiles {
    fn call(&self, request: &Request) -> Response {
        self.serve(request, request.path())
    }
}
