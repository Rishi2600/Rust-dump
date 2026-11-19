use std::fs;
use std::path::{Path, PathBuf};

use crate::method::Method;
use crate::mime::mime_type;
use crate::request::Request;
use crate::response::Response;
use crate::service::Service;
use crate::status::StatusCode;

/// Serves the files under one directory, and nothing outside it.
pub struct StaticFiles {
    root: PathBuf,
}

impl StaticFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
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

impl Service for StaticFiles {
    fn call(&self, request: &Request) -> Response {
        self.serve(request, request.path())
    }
}
