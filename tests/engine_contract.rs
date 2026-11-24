//! Scoped mobile adapter contracts: engine detection, socket access, bounded
//! statistics requests and unchanged tar upload bytes. No daemon or display is required.
#![allow(dead_code, unused_imports)]

#[path = "../src/engine/mod.rs"]
mod engine;
#[path = "../src/gettext.rs"]
mod gettext;

use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;
use serde_json::json;

#[derive(Clone, Debug)]
struct Request {
    method: String,
    target: String,
    body: Vec<u8>,
}

impl Request {
    fn path(&self) -> &str {
        let path = self.target.split('?').next().unwrap();
        if path.starts_with("/v1.") {
            &path[path[1..].find('/').unwrap() + 1..]
        } else {
            path
        }
    }
}

struct Response {
    status: u16,
    body: Vec<u8>,
}

impl Response {
    fn json(value: Value) -> Self {
        Self {
            status: 200,
            body: serde_json::to_vec(&value).unwrap(),
        }
    }

    fn empty() -> Self {
        Self {
            status: 204,
            body: Vec::new(),
        }
    }
}

struct Daemon {
    _dir: tempfile::TempDir,
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Daemon {
    fn new(handler: impl Fn(&Request) -> Response + Send + Sync + 'static) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docker.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let stop = stop.clone();
            let requests = requests.clone();
            let handler = Arc::new(handler);
            thread::spawn(move || {
                let mut workers = Vec::new();
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut socket, _)) => {
                            let requests = requests.clone();
                            let handler = handler.clone();
                            workers.push(thread::spawn(move || {
                                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                                // The socket access probe closes without an HTTP request.
                                let Some(request) = read_request(&mut socket) else { return };
                                requests.lock().unwrap().push(request.clone());
                                let response = handler(&request);
                                write!(socket,
                                    "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    response.status, response.body.len(),
                                ).unwrap();
                                socket.write_all(&response.body).unwrap();
                            }));
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(err) => panic!("mock accept: {err}"),
                    }
                }
                for worker in workers {
                    worker.join().unwrap();
                }
            })
        };
        Self {
            _dir: dir,
            url: format!("unix://{}", path.display()),
            requests,
            stop,
            worker: Some(worker),
        }
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn read_request(socket: &mut UnixStream) -> Option<Request> {
    let mut buf = Vec::new();
    let header_end = loop {
        let mut chunk = [0; 4096];
        match socket.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(err) => panic!("reading mock request: {err}"),
        }
        if let Some(end) = buf.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            break end + 4;
        }
        assert!(buf.len() < 64 * 1024, "oversized mock request headers");
    };
    let headers = String::from_utf8(buf[..header_end].to_vec()).unwrap();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let chunked = headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value.trim().eq_ignore_ascii_case("chunked")
        })
    });
    let body = if chunked {
        let mut offset = header_end;
        let mut body = Vec::new();
        loop {
            let line = read_line(socket, &mut buf, &mut offset);
            let length = usize::from_str_radix(line.split(';').next().unwrap(), 16).unwrap();
            if length == 0 {
                while !read_line(socket, &mut buf, &mut offset).is_empty() {}
                break;
            }
            assert!(
                body.len() + length <= 16 * 1024 * 1024,
                "oversized mock body"
            );
            read_to(socket, &mut buf, offset + length + 2);
            body.extend_from_slice(&buf[offset..offset + length]);
            offset += length;
            assert_eq!(&buf[offset..offset + 2], b"\r\n");
            offset += 2;
        }
        body
    } else {
        read_to(socket, &mut buf, header_end + content_length);
        buf[header_end..header_end + content_length].to_vec()
    };
    let mut line = headers.lines().next().unwrap().split_whitespace();
    Some(Request {
        method: line.next().unwrap().to_owned(),
        target: line.next().unwrap().to_owned(),
        body,
    })
}

fn read_to(socket: &mut UnixStream, buf: &mut Vec<u8>, length: usize) {
    while buf.len() < length {
        let mut chunk = [0; 4096];
        let n = socket.read(&mut chunk).unwrap();
        assert_ne!(n, 0, "truncated request body");
        buf.extend_from_slice(&chunk[..n]);
    }
}

fn read_line(socket: &mut UnixStream, buf: &mut Vec<u8>, offset: &mut usize) -> String {
    loop {
        if let Some(end) = buf[*offset..].windows(2).position(|bytes| bytes == b"\r\n") {
            let line = String::from_utf8(buf[*offset..*offset + end].to_vec()).unwrap();
            *offset += end + 2;
            return line;
        }
        let length = buf.len() + 1;
        read_to(socket, buf, length);
    }
}

fn version(engine_name: &str, api: &str) -> Response {
    Response::json(json!({
        "ApiVersion": api,
        "MinAPIVersion": "1.24",
        "Version": "test",
        "Components": [{ "Name": engine_name, "Version": "test" }],
    }))
}

fn run(future: impl std::future::Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
}

#[test]
fn identifies_docker_and_preserves_podman_capabilities() {
    run(async {
        for (name, is_podman) in [("Engine", false), ("Podman Engine", true)] {
            let daemon = Daemon::new(move |request| match request.path() {
                "/version" => version(name, "1.53"),
                "/containers/json" => Response::json(json!([])),
                _ => panic!("unexpected request: {request:?}"),
            });
            let engine = engine::Engine::new(&daemon.url).await.unwrap();
            assert_eq!(engine.capabilities().pods, is_podman);
            assert_eq!(engine.capabilities().kube_generation, is_podman);
            if matches!(engine, engine::Engine::Docker(_)) {
                engine.containers().list().await.unwrap();
            }
            let requests = daemon.requests();
            assert_eq!(
                requests
                    .iter()
                    .filter(|request| request.path() == "/version")
                    .count(),
                1
            );
        }
    });
}

#[test]
fn rejects_unknown_engine_without_creating_a_client() {
    run(async {
        let daemon = Daemon::new(|_| version("Unrelated server", "1.53"));
        assert!(
            engine::Engine::new(&daemon.url)
                .await
                .unwrap_err()
                .to_string()
                .contains("no suitable engine")
        );
    });
}

#[test]
fn limits_docker_statistics_requests_to_four() {
    run(async {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let daemon = Daemon::new({
            let active = active.clone();
            let peak = peak.clone();
            move |request| match request.path() {
                "/version" => version("Engine", "1.53"),
                "/containers/json" => Response::json(Value::Array(
                    (0..7)
                        .map(|id| json!({ "Id": format!("container-{id}") }))
                        .collect(),
                )),
                path if path.ends_with("/stats") => {
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(60));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Response::json(json!({
                        "cpu_stats": { "cpu_usage": { "total_usage": 100 }, "system_cpu_usage": 1000, "online_cpus": 2 },
                        "memory_stats": { "usage": 128, "limit": 1024 },
                    }))
                }
                _ => panic!("unexpected request: {request:?}"),
            }
        });
        let engine = engine::Engine::new(&daemon.url).await.unwrap();
        let containers = engine.containers();
        let mut stream = containers.stats_stream(1);
        assert_eq!(
            stream
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_single_stats()
                .len(),
            7
        );
        assert_eq!(peak.load(Ordering::SeqCst), 4);
    });
}

#[test]
fn streams_file_upload_body_unchanged_for_both_engines() {
    run(async {
        let payload = vec![b'x'; 128 * 1024];
        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_path("payload.txt").unwrap();
        header.set_mode(0o644);
        header.set_size(payload.len() as u64);
        header.set_cksum();
        tar.append(&header, payload.as_slice()).unwrap();
        let expected = tar.into_inner().unwrap();
        for name in ["Engine", "Podman Engine"] {
            let daemon = Daemon::new(move |request| match request.path() {
                "/version" => version(name, "1.53"),
                path if path.ends_with("/containers/upload/archive") => Response::empty(),
                _ => panic!("unexpected request: {request:?}"),
            });
            let engine = engine::Engine::new(&daemon.url).await.unwrap();
            let mut file = tempfile::tempfile().unwrap();
            file.write_all(&expected).unwrap();
            file.rewind().unwrap();
            engine
                .containers()
                .get("upload")
                .copy_to("/tmp".into(), tokio::fs::File::from_std(file))
                .await
                .unwrap();
            let request = daemon
                .requests()
                .into_iter()
                .find(|request| request.path().ends_with("/containers/upload/archive"))
                .unwrap();
            assert_eq!(request.method, "PUT");
            assert_eq!(request.body, expected);
        }
    });
}
