//! Static file serving, against a small site written to disk for each test.

use std::fs;
use std::path::{Path, PathBuf};

use mini_http::files::{ByteRange, StaticFiles, parse_range};
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

#[test]
fn a_directory_serves_its_index_page() {
    let files = StaticFiles::new(site("index"));
    let home = get(&files, "/");
    assert_eq!(home.status, StatusCode::OK);
    assert_eq!(text(&home), "<h1>home</h1>");
    assert_eq!(
        home.headers.get("Content-Type"),
        Some("text/html; charset=utf-8")
    );
}

#[test]
fn a_directory_without_its_slash_is_redirected_to_the_one_with() {
    let root = site("slash");
    fs::create_dir_all(root.join("my docs")).unwrap();
    let files = StaticFiles::new(&root);
    let redirect = get(&files, "/docs");
    assert_eq!(redirect.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(redirect.headers.get("Location"), Some("/docs/"));
    // The new address is a URL, so it is encoded again.
    assert_eq!(
        get(&files, "/my%20docs").headers.get("Location"),
        Some("/my%20docs/")
    );
}

#[test]
fn a_directory_without_an_index_is_private_unless_listing_is_on() {
    let root = site("listing");
    let closed = StaticFiles::new(&root);
    assert_eq!(get(&closed, "/docs/").status, StatusCode::FORBIDDEN);

    let open = StaticFiles::new(&root).with_listing(true);
    let listing = get(&open, "/docs/");
    assert_eq!(listing.status, StatusCode::OK);
    let page = text(&listing);
    assert!(page.contains("<h1>Index of /docs/</h1>"));
    assert!(page.contains("<a href=\"guide.txt\">guide.txt</a>"));
    assert!(page.contains("<a href=\"../\">../</a>"));
    // An index page still wins over a listing where there is one.
    assert_eq!(text(&get(&open, "/")), "<h1>home</h1>");
}

#[test]
fn a_listing_marks_directories_and_hides_dot_files() {
    let root = site("mixed");
    fs::remove_file(root.join("index.html")).unwrap();
    let files = StaticFiles::new(&root).with_listing(true);
    let page = text(&get(&files, "/"));
    assert!(page.contains("<a href=\"css/\">css/</a>"));
    assert!(page.contains("<a href=\"my%20file.txt\">my file.txt</a>"));
    assert!(!page.contains(".secret"));
    assert!(!page.contains("../"), "the root has no parent to link to");
    // Entries come out sorted by name.
    assert!(page.find("about.html") < page.find("notes.txt"));
}

#[test]
fn a_hostile_file_name_cannot_inject_markup_into_a_listing() {
    let root = site("xss");
    fs::write(root.join("docs/<img src=x onerror=alert(1)>.txt"), "x").unwrap();
    fs::write(root.join("docs/a&b \"q\".txt"), "x").unwrap();
    let files = StaticFiles::new(&root).with_listing(true);
    let page = text(&get(&files, "/docs/"));
    assert!(!page.contains("<img"));
    assert!(page.contains("&lt;img src=x onerror=alert(1)&gt;.txt"));
    assert!(page.contains("a&amp;b &quot;q&quot;.txt"));
    assert!(page.contains("href=\"a%26b%20%22q%22.txt\""));
}

#[test]
fn every_file_response_carries_validators() {
    let files = StaticFiles::new(site("validators"));
    let response = get(&files, "/notes.txt");
    let etag = response.headers.get("ETag").unwrap();
    assert!(etag.starts_with("\"a-") && etag.ends_with('"'), "{etag}");
    assert!(
        response
            .headers
            .get("Last-Modified")
            .unwrap()
            .ends_with(" GMT")
    );
}

#[test]
fn a_client_with_a_current_copy_is_told_not_modified() {
    let files = StaticFiles::new(site("etag"));
    let first = get(&files, "/notes.txt");
    let etag = first.headers.get("ETag").unwrap().to_string();

    let again = request_with(
        &files,
        "GET",
        "/notes.txt",
        &format!("If-None-Match: {etag}\r\n"),
    );
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert_eq!(again.headers.get("ETag"), Some(etag.as_str()));
    assert_eq!(again.bytes(), Some(&b""[..]));

    // One of several tags, or a weak one, still counts as a match.
    let several = format!("If-None-Match: \"other\", W/{etag}\r\n");
    assert_eq!(
        request_with(&files, "GET", "/notes.txt", &several).status,
        StatusCode::NOT_MODIFIED
    );
    let stale = request_with(&files, "GET", "/notes.txt", "If-None-Match: \"stale\"\r\n");
    assert_eq!(stale.status, StatusCode::OK);
    assert_eq!(text(&stale), "0123456789");
}

#[test]
fn a_changed_file_gets_a_new_tag_and_is_sent_again() {
    let root = site("changed");
    let files = StaticFiles::new(&root);
    let etag = get(&files, "/notes.txt")
        .headers
        .get("ETag")
        .unwrap()
        .to_string();
    fs::write(root.join("notes.txt"), "a longer second version").unwrap();

    let after = request_with(
        &files,
        "GET",
        "/notes.txt",
        &format!("If-None-Match: {etag}\r\n"),
    );
    assert_eq!(after.status, StatusCode::OK);
    assert_eq!(text(&after), "a longer second version");
    assert_ne!(after.headers.get("ETag"), Some(etag.as_str()));
}

#[test]
fn the_date_of_the_clients_copy_works_as_a_validator_too() {
    let files = StaticFiles::new(site("since"));
    let modified = get(&files, "/notes.txt")
        .headers
        .get("Last-Modified")
        .unwrap()
        .to_string();

    let same = format!("If-Modified-Since: {modified}\r\n");
    assert_eq!(
        request_with(&files, "GET", "/notes.txt", &same).status,
        StatusCode::NOT_MODIFIED
    );
    let old = "If-Modified-Since: Thu, 01 Jan 1970 00:00:00 GMT\r\n";
    assert_eq!(
        request_with(&files, "GET", "/notes.txt", old).status,
        StatusCode::OK
    );
    let garbled = "If-Modified-Since: last tuesday\r\n";
    assert_eq!(
        request_with(&files, "GET", "/notes.txt", garbled).status,
        StatusCode::OK
    );

    // When both are sent, the tag is believed and the date ignored.
    let both = format!("If-None-Match: \"stale\"\r\nIf-Modified-Since: {modified}\r\n");
    assert_eq!(
        request_with(&files, "GET", "/notes.txt", &both).status,
        StatusCode::OK
    );
}

#[test]
fn the_three_forms_of_range_are_understood() {
    let part = |first, last| ByteRange::Part { first, last };
    assert_eq!(parse_range("bytes=0-499", 1000), part(0, 499));
    assert_eq!(parse_range("bytes=500-", 1000), part(500, 999));
    assert_eq!(parse_range("bytes=-200", 1000), part(800, 999));
    // Asking for more than there is gives what there is.
    assert_eq!(parse_range("bytes=900-5000", 1000), part(900, 999));
    assert_eq!(parse_range("bytes=-5000", 1000), part(0, 999));
    assert_eq!(parse_range("bytes=0-0", 1000), part(0, 0));
}

#[test]
fn ranges_beyond_the_file_cannot_be_satisfied() {
    assert_eq!(parse_range("bytes=1000-", 1000), ByteRange::Unsatisfiable);
    assert_eq!(
        parse_range("bytes=2000-3000", 1000),
        ByteRange::Unsatisfiable
    );
    assert_eq!(parse_range("bytes=-0", 1000), ByteRange::Unsatisfiable);
    assert_eq!(parse_range("bytes=0-", 0), ByteRange::Unsatisfiable);
}

#[test]
fn ranges_that_make_no_sense_are_ignored() {
    for header in [
        "bytes=",
        "bytes=-",
        "bytes=abc-def",
        "bytes=5-2",
        "lines=1-2",
        "bytes=0-1,5-6",
        "0-499",
    ] {
        assert_eq!(parse_range(header, 1000), ByteRange::Whole, "{header}");
    }
}

#[test]
fn a_range_request_gets_just_those_bytes() {
    let files = StaticFiles::new(site("range"));
    let whole = get(&files, "/notes.txt");
    assert_eq!(whole.headers.get("Accept-Ranges"), Some("bytes"));

    let middle = request_with(&files, "GET", "/notes.txt", "Range: bytes=2-5\r\n");
    assert_eq!(middle.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(text(&middle), "2345");
    assert_eq!(middle.headers.get("Content-Range"), Some("bytes 2-5/10"));

    let tail = request_with(&files, "GET", "/notes.txt", "Range: bytes=-3\r\n");
    assert_eq!(
        (text(&tail).as_str(), tail.headers.get("Content-Range")),
        ("789", Some("bytes 7-9/10"))
    );
    // Resuming a download: everything from byte 6 on.
    let rest = request_with(&files, "GET", "/notes.txt", "Range: bytes=6-\r\n");
    assert_eq!(text(&rest), "6789");
}

#[test]
fn an_impossible_range_is_refused_with_the_real_length() {
    let files = StaticFiles::new(site("range416"));
    let beyond = request_with(&files, "GET", "/notes.txt", "Range: bytes=50-60\r\n");
    assert_eq!(beyond.status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(beyond.headers.get("Content-Range"), Some("bytes */10"));
    // A range that cannot be parsed is answered with the whole file.
    let odd = request_with(&files, "GET", "/notes.txt", "Range: bytes=x-y\r\n");
    assert_eq!(
        (odd.status, text(&odd).as_str()),
        (StatusCode::OK, "0123456789")
    );
}
