# mini_http

An HTTP/1.1 server, and a small client, written in Rust directly on TCP
sockets. The server uses nothing beyond the standard library. The client has
one dependency: the sibling `mini_dns_resolver` project, which it uses to turn
host names into addresses.

The server implements:

- **request parsing** from a byte stream: request line, headers, and bodies
  sent with `Content-Length` or in chunks
- **keep-alive** connections carrying many requests, with a quota per
  connection
- **routing** by method and path, with `:name` parameters and `*rest`
  wildcards, and the right answer for an unknown path (404) versus a known
  path with the wrong method (405 with `Allow`)
- **static files** with content types, index pages and directory listings
- **conditional requests** (`ETag`, `Last-Modified`, 304) and **byte ranges**
  (206, 416)
- **chunked responses** for bodies whose length is not known in advance
- a **thread pool**, so a slow request does not hold up the others
- **limits and timeouts** on everything a client controls: header size, body
  size, requests per connection, and how long it may stay silent
- **defences** against path traversal, request smuggling, header injection
  and markup injection in listings

## Running the server

```sh
cargo build --release
target/release/mini_http serve --list
```

```text
serving public at http://127.0.0.1:8080/  (Ctrl-C to stop)
```

Open that address in a browser, or use `curl` from another terminal. The
`Date`, `ETag` and `Last-Modified` values below will differ on your machine.

```text
$ curl -i http://127.0.0.1:8080/api/hello/world
HTTP/1.1 200 OK
Content-Type: text/plain; charset=utf-8
Date: Thu, 19 Nov 2026 17:02:00 GMT
Content-Length: 14
Connection: keep-alive

Hello, world!
```

A range request returns part of a file:

```text
$ curl -i -r 0-14 http://127.0.0.1:8080/docs/notes.txt
HTTP/1.1 206 Partial Content
Content-Range: bytes 0-14/91
Content-Type: text/plain; charset=utf-8
Accept-Ranges: bytes
ETag: "5b-6aff2b65"
Last-Modified: Thu, 19 Nov 2026 17:01:25 GMT
Date: Thu, 19 Nov 2026 17:02:00 GMT
Content-Length: 15
Connection: keep-alive

This directory 
```

`/api/stream` has no known length, so it is sent in chunks. `curl --raw` shows
them undecoded: each chunk is its size in hexadecimal, the bytes, and a line
break, with a zero-sized chunk at the end.

```text
$ curl -s --raw http://127.0.0.1:8080/api/stream
5
5...

5
4...

5
3...

5
2...

5
1...

0

```

A path that tries to climb out of the served directory is refused before it
reaches the file system:

```text
$ curl -s -o /dev/null -w '%{http_code}\n' --path-as-is http://127.0.0.1:8080/../Cargo.toml
400
```

Meanwhile the server logs one line per request: method, path, status, body
size in bytes, and time taken.

```text
GET /api/hello/world 200 14 0.0ms
GET /api/stream 200 - 0.0ms
GET /docs/notes.txt 206 15 0.2ms
```

### Built-in routes

| Route | What it does |
| --- | --- |
| `GET /api/hello/:name` | greets whatever name is in the path |
| `GET /api/time` | the current time, in the HTTP date format |
| `GET /api/headers` | the headers the client sent |
| `POST /api/echo` | sends the request body back |
| `GET /api/stream` | a short countdown, sent in chunks |
| `GET /*path` | a file from the served directory |

### Server options

| Option | Meaning | Default |
| --- | --- | --- |
| `--root <DIR>` | directory to serve | `public` |
| `--addr <IP>` | address to listen on | `127.0.0.1` |
| `--port <N>` | port to listen on | 8080 |
| `--threads <N>` | worker threads | 8 |
| `--list` | list directories that have no `index.html` | off |
| `--quiet` | do not log requests | off |

The default address is reachable only from the same machine. Use
`--addr 0.0.0.0` to accept connections from the network.

## Running the client

`fetch` takes a URL, looks the host up, connects, and prints what comes back.

```text
$ target/release/mini_http fetch -I http://example.com/
HTTP/1.1 200 OK
Date: Thu, 19 Nov 2026 17:01:42 GMT
Content-Type: text/html; charset=utf-8
Connection: close
Server: cloudflare
...
```

Three separate things happened there, and they are worth keeping apart:

1. **DNS** found the address of `example.com`, by asking a root server, then a
   `.com` server, then the domain's own server. That is `mini_dns_resolver`.
2. **TCP** opened a connection to that address on port 80.
3. **HTTP** was spoken over the connection: a `HEAD` request went out and the
   headers above came back.

The server side of this project never uses DNS. It listens on a port and
answers whoever connects.

| Option | Meaning |
| --- | --- |
| `-i`, `--include` | print the status line and headers before the body |
| `-I`, `--head` | ask for the headers only |

Only `http://` URLs work. There is no TLS, so `https://` is refused with a
message saying so.

## Source layout

| File | Role |
| --- | --- |
| `src/error.rs` | the error type |
| `src/status.rs`, `src/method.rs` | status codes and request methods |
| `src/headers.rs` | case-insensitive header list |
| `src/percent.rs`, `src/target.rs` | URL decoding, path normalisation, query strings |
| `src/date.rs` | the HTTP date format |
| `src/request.rs` | request line, headers, and how the body is delimited |
| `src/stream.rs` | buffered reading from a byte stream, with limits |
| `src/reader.rs` | reading a whole request: head, then fixed or chunked body |
| `src/response.rs` | responses, and writing them with the right framing |
| `src/service.rs`, `src/connection.rs` | the `Service` trait and the keep-alive loop |
| `src/router.rs` | routes, parameters, wildcards, 404 and 405 |
| `src/pool.rs`, `src/server.rs` | thread pool, accept loop, timeouts, shutdown |
| `src/mime.rs`, `src/files.rs` | static files, listings, conditional and range requests |
| `src/log.rs` | access log |
| `src/url.rs`, `src/client.rs` | the client |
| `src/dns.rs` | name resolution through `mini_dns_resolver` |
| `src/main.rs` | command-line interface |
| `public/` | a small site to serve |

## Tests

```sh
cargo test
```

None of the tests uses the internet.

- Parsing is tested on text held in memory, including every malformed and
  hostile input the parser must reject.
- The connection loop is tested on in-memory streams, so its behaviour is
  exact and repeatable.
- `tests/server.rs` and `tests/client.rs` use real sockets on localhost:
  concurrent clients, a slow request beside a fast one, idle and stalled
  connections, shutdown.
- `tests/files.rs` writes a small site to disk and attacks it with traversal
  paths, a symbolic link pointing outside, and hostile file names.
- `tests/dns.rs` resolves a name through a fake DNS server in memory and then
  fetches it from this project's own server: the full path a browser takes.

## Limits

This is a learning project, not a server to put on the internet.

- No TLS, so no `https://`, in either direction.
- HTTP/1.1 only: no HTTP/2 or HTTP/3.
- One thread per connection being served, from a fixed pool. Many idle
  keep-alive connections can occupy every worker.
- Files are read into memory before being sent, so very large files are
  costly.
- No compression, no cookies or sessions, no multi-range responses.
- Requests rejected before they reach a handler, such as a 400, are answered
  but not logged.
- The client makes one request per connection and does not follow redirects.
