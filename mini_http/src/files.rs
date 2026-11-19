use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::date::{http_date, parse_http_date};
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
            Some(found) if found.is_file() => self.send_file(request, &found),
            Some(found) if found.is_dir() => self.send_directory(request, path, &found),
            _ => Response::error(StatusCode::NOT_FOUND),
        }
    }

    fn send_file(&self, request: &Request, file: &Path) -> Response {
        // It was there a moment ago. Gone or unreadable now is the same to
        // the client as never having existed.
        let (Ok(contents), Ok(metadata)) = (fs::read(file), fs::metadata(file)) else {
            return Response::error(StatusCode::NOT_FOUND);
        };
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        let etag = entity_tag(contents.len() as u64, modified);
        // Both validators go on every answer, so the client can quote them
        // back the next time it wants this file.
        let validated = |response: Response| {
            response
                .header("ETag", &etag)
                .header("Last-Modified", &http_date(modified))
        };
        if is_unchanged(request, &etag, modified) {
            // The client's copy is still good. Saying so costs a few header
            // lines instead of the whole file.
            return validated(Response::new(StatusCode::NOT_MODIFIED));
        }
        let length = contents.len() as u64;
        let range = request
            .header("Range")
            .map_or(ByteRange::Whole, |header| parse_range(header, length));
        let response = match range {
            ByteRange::Whole => Response::new(StatusCode::OK).body(contents),
            ByteRange::Part { first, last } => Response::new(StatusCode::PARTIAL_CONTENT)
                .header("Content-Range", &format!("bytes {first}-{last}/{length}"))
                .body(&contents[first as usize..=last as usize]),
            ByteRange::Unsatisfiable => {
                // Telling the client the real length lets it ask again sensibly.
                return Response::error(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header("Content-Range", &format!("bytes */{length}"));
            }
        };
        let response = response.header("Content-Type", mime_type(file));
        validated(response.header("Accept-Ranges", "bytes"))
    }
}

impl StaticFiles {
    fn send_directory(&self, request: &Request, path: &str, directory: &Path) -> Response {
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
            return self.send_file(request, &index);
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

/// A short fingerprint of a file's current state, for the `ETag` header.
///
/// It only has to change whenever the file does. Size and modification time
/// together are a cheap approximation of that, with no need to hash the
/// contents on every request.
pub fn entity_tag(length: u64, modified: SystemTime) -> String {
    format!("\"{length:x}-{:x}\"", whole_seconds(modified))
}

fn whole_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Whether the copy the client says it already has is still current.
pub fn is_unchanged(request: &Request, etag: &str, modified: SystemTime) -> bool {
    // `If-None-Match: "abc", "def"` lists the tags of copies the client
    // holds. When it is sent, it decides the matter on its own.
    if let Some(known) = request.header("If-None-Match") {
        let mut tags = known
            .split(',')
            .map(|tag| tag.trim().trim_start_matches("W/"));
        return tags.any(|tag| tag == etag || tag == "*");
    }
    // `If-Modified-Since` is the older way: the date of the client's copy.
    // Header dates have no fractions of a second, so neither does this.
    let since = request
        .header("If-Modified-Since")
        .and_then(parse_http_date);
    since.is_some_and(|since| whole_seconds(modified) <= whole_seconds(since))
}

/// Which part of a file a `Range` header asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteRange {
    /// No usable range: send everything.
    Whole,
    /// Bytes `first` to `last`, both included.
    Part { first: u64, last: u64 },
    /// A range that lies entirely beyond the end of the file.
    Unsatisfiable,
}

/// Interpret a `Range` header for a file of `length` bytes.
///
/// This is what lets a download resume after a dropped connection, and a
/// video player jump to the middle of a file. Three forms exist:
/// `bytes=0-499` (a span), `bytes=500-` (from there to the end) and
/// `bytes=-500` (the last 500 bytes).
///
/// A header that cannot be understood is ignored, since sending the whole
/// file is always an acceptable answer to a range request.
pub fn parse_range(header: &str, length: u64) -> ByteRange {
    let Some(spec) = header.strip_prefix("bytes=") else {
        return ByteRange::Whole;
    };
    // Several ranges at once need a multipart reply. Not supported.
    let Some((first, last)) = spec.split_once('-').filter(|_| !spec.contains(',')) else {
        return ByteRange::Whole;
    };
    let (first, last) = (first.trim(), last.trim());
    let number = |text: &str| text.parse::<u64>().ok();
    match (first.is_empty(), last.is_empty()) {
        (true, false) => match number(last) {
            Some(count) if count == 0 || length == 0 => ByteRange::Unsatisfiable,
            Some(count) => ByteRange::Part {
                first: length.saturating_sub(count),
                last: length - 1,
            },
            None => ByteRange::Whole,
        },
        (false, true) => match number(first) {
            Some(first) if first < length => ByteRange::Part {
                first,
                last: length - 1,
            },
            Some(_) => ByteRange::Unsatisfiable,
            None => ByteRange::Whole,
        },
        (false, false) => match (number(first), number(last)) {
            (Some(first), Some(last)) if first > last => ByteRange::Whole,
            // A span that runs past the end is cut short, not refused.
            (Some(first), Some(last)) if first < length => ByteRange::Part {
                first,
                last: last.min(length - 1),
            },
            (Some(_), Some(_)) => ByteRange::Unsatisfiable,
            _ => ByteRange::Whole,
        },
        (true, true) => ByteRange::Whole,
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
