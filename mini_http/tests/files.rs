//! Static file serving, against a small site written to disk for each test.

use std::fs;
use std::path::{Path, PathBuf};

use mini_http::files::StaticFiles;
use mini_http::reader::parse_request;
use mini_http::response::Response;
use mini_http::service::Service;
use mini_http::status::StatusCode;

/// Create a fresh site under the build directory and return its root.
///
/// ```text
/// index.html   about.html   notes.txt   my file.txt   .secret
/// css/site.css
/// docs/guide.txt            (a directory without an index page)
/// ```
fn site(name: &str) -> PathBuf {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-sites");
    let root = base.join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("css")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("index.html"), "<h1>home</h1>").unwrap();
    fs::write(root.join("about.html"), "<h1>about</h1>").unwrap();
    fs::write(root.join("notes.txt"), "0123456789").unwrap();
    fs::write(root.join("my file.txt"), "spaced").unwrap();
    fs::write(root.join(".secret"), "password").unwrap();
    fs::write(root.join("css/site.css"), "body { margin: 0 }").unwrap();
    fs::write(root.join("docs/guide.txt"), "the guide").unwrap();
    // Something outside the root that no request must ever reach.
    fs::write(base.join(format!("{name}-outside.txt")), "outside").unwrap();
    root
}

fn request_with(files: &StaticFiles, method: &str, target: &str, headers: &str) -> Response {
    let raw = format!("{method} {target} HTTP/1.1\r\nHost: x\r\n{headers}\r\n");
    match parse_request(raw.as_bytes()) {
        Ok(request) => files.call(&request),
        // A path refused by the parser never reaches the file server at all.
        Err(error) => Response::error(error.status().unwrap()),
    }
}

fn get(files: &StaticFiles, target: &str) -> Response {
    request_with(files, "GET", target, "")
}

fn text(response: &Response) -> String {
    String::from_utf8_lossy(response.bytes().unwrap()).into_owned()
}

#[test]
fn files_are_served_with_their_content_type() {
    let files = StaticFiles::new(site("basic"));
    let page = get(&files, "/about.html");
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(
        page.headers.get("Content-Type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(text(&page), "<h1>about</h1>");

    let style = get(&files, "/css/site.css");
    assert_eq!(
        style.headers.get("Content-Type"),
        Some("text/css; charset=utf-8")
    );
    // The name on disk has a space; the URL carries it encoded.
    assert_eq!(text(&get(&files, "/my%20file.txt")), "spaced");
    assert_eq!(text(&get(&files, "/notes.txt?download=1")), "0123456789");
}

#[test]
fn missing_and_hidden_files_look_the_same_from_outside() {
    let files = StaticFiles::new(site("hidden"));
    assert_eq!(get(&files, "/nope.html").status, StatusCode::NOT_FOUND);
    assert_eq!(get(&files, "/.secret").status, StatusCode::NOT_FOUND);
    assert_eq!(get(&files, "/css/.htaccess").status, StatusCode::NOT_FOUND);
}

#[test]
fn nothing_outside_the_root_can_be_reached() {
    let root = site("escape");
    let files = StaticFiles::new(&root);
    let attacks = [
        "/../escape-outside.txt",
        "/css/../../escape-outside.txt",
        "/%2e%2e/escape-outside.txt",
        "/..%2Fescape-outside.txt",
        "/css/%2e%2e/%2e%2e/escape-outside.txt",
        "/../../../../../../etc/passwd",
    ];
    for attack in attacks {
        let response = get(&files, attack);
        assert_ne!(response.status, StatusCode::OK, "{attack}");
        assert!(!text(&response).contains("outside"), "{attack}");
    }
    // The file server refuses these on its own too, without the parser.
    assert_eq!(files.locate("/../escape-outside.txt"), None);
    assert_eq!(files.locate("/css/..\\..\\escape-outside.txt"), None);
    assert!(files.locate("/notes.txt").is_some());
}

#[cfg(unix)]
#[test]
fn a_symlink_leading_out_of_the_root_is_not_followed() {
    let root = site("symlink");
    let outside = root.parent().unwrap().join("symlink-outside.txt");
    std::os::unix::fs::symlink(&outside, root.join("link.txt")).unwrap();
    std::os::unix::fs::symlink(root.join("notes.txt"), root.join("inside.txt")).unwrap();

    let files = StaticFiles::new(&root);
    assert_eq!(get(&files, "/link.txt").status, StatusCode::NOT_FOUND);
    // A link that stays inside the root is an ordinary file.
    assert_eq!(text(&get(&files, "/inside.txt")), "0123456789");
}

#[test]
fn only_reading_methods_are_allowed() {
    let files = StaticFiles::new(site("methods"));
    let post = request_with(&files, "POST", "/notes.txt", "Content-Length: 0\r\n");
    assert_eq!(post.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(post.headers.get("Allow"), Some("GET, HEAD"));
    assert_eq!(
        request_with(&files, "DELETE", "/notes.txt", "").status,
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        text(&get(&files, "/notes.txt")),
        "0123456789",
        "the file is untouched"
    );
    assert_eq!(
        request_with(&files, "HEAD", "/notes.txt", "").status,
        StatusCode::OK
    );
}
