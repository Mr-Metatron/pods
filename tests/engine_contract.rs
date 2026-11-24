//! Unix HTTP contract tests against the real engine adapter. No daemon or display
//! is required; real Docker and device acceptance are documented separately.
#![allow(dead_code, unused_imports)]

#[path = "../src/engine/mod.rs"]
mod engine;
#[path = "../src/gettext.rs"]
mod gettext;

use std::io::Read;
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

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
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
    while buf.len() < header_end + content_length {
        let mut chunk = [0; 4096];
        let n = socket.read(&mut chunk).unwrap();
        assert_ne!(n, 0, "truncated request body");
        buf.extend_from_slice(&chunk[..n]);
    }
    let mut line = headers.lines().next().unwrap().split_whitespace();
    Some(Request {
        method: line.next().unwrap().to_owned(),
        target: line.next().unwrap().to_owned(),
        body: buf[header_end..header_end + content_length].to_vec(),
    })
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
fn negotiates_docker_api_and_keeps_podman_capabilities() {
    run(async {
        for (name, is_podman) in [("Engine", false), ("Podman Engine", true)] {
            let daemon = Daemon::new(move |request| match request.path() {
                "/version" => version(name, "1.41"),
                "/containers/json" => Response::json(json!([])),
                _ => panic!("unexpected request: {request:?}"),
            });
            let engine = engine::Engine::new(&daemon.url).await.unwrap();
            assert_eq!(engine.capabilities().pods, is_podman);
            assert_eq!(engine.capabilities().kube_generation, is_podman);
            if let engine::Engine::Docker(docker) = &engine {
                assert_eq!(docker.client_version().minor_version, 41);
                engine.containers().list().await.unwrap();
            }
            let requests = daemon.requests();
            assert_eq!(
                requests
                    .iter()
                    .filter(|req| req.path() == "/version")
                    .count(),
                2
            );
            // Record actual SDK requests, including whether it emits an API prefix.
            eprintln!(
                "{name}: {:?}",
                requests.iter().map(|req| &req.target).collect::<Vec<_>>()
            );
        }
    });
}

#[test]
fn rejects_unknown_engine_without_creating_a_client() {
    run(async {
        let daemon = Daemon::new(|_| version("Unrelated server", "1.53"));
        let result = engine::Engine::new(&daemon.url).await;
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("no suitable engine")
        );
    });
}

#[test]
fn docker_create_lifecycle_and_volume_requests_keep_user_options() {
    run(async {
        let daemon = Daemon::new(|request| match request.path() {
            "/version" => version("Engine", "1.56"),
            "/containers/create" => {
                Response::json(json!({ "Id": "test-container", "Warnings": [] }))
            }
            "/volumes/create" => Response::json(
                json!({ "Name": "test-volume", "Driver": "local", "Mountpoint": "/test" }),
            ),
            "/volumes" => Response::json(json!({ "Volumes": [], "Warnings": [] })),
            "/containers/test-container/exec" => Response::json(json!({ "Id": "test-exec" })),
            _ => Response::empty(),
        });
        let engine = engine::Engine::new(&daemon.url).await.unwrap();
        let opts = engine::opts::ContainerCreateOpts {
            name: "mobile-test".into(),
            image: "alpine:latest".into(),
            cmd: Some(vec!["sh".into(), "-c".into(), "echo ready".into()]),
            env: [("LANG".into(), "zh_CN.UTF-8".into())].into(),
            memory_limit: Some(128 * 1024 * 1024),
            ports: vec![engine::dto::PortBinding {
                host_ip: String::new(),
                host_port: Some(8080),
                target_port: 80,
                protocol: engine::dto::PortMappingProtocol::Tcp,
            }],
            mounts: vec![engine::opts::ContainerCreateMountOpts {
                host_path: "/home/mobile/data".into(),
                container_path: "/data".into(),
                read_only: true,
                selinux: Default::default(),
            }],
            volumes: vec![engine::opts::ContainerCreateVolumeOpts {
                volume: "test-volume".into(),
                container_path: "/cache".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let id = engine.containers().create(opts).await.unwrap();
        let container = engine.containers().get(id);
        container.start().await.unwrap();
        container.pause().await.unwrap();
        container.unpause().await.unwrap();
        container.restart(false).await.unwrap();
        container.stop().await.unwrap();
        container.rename("renamed").await.unwrap();
        let exec = container
            .create_exec(engine::opts::ExecCreateOpts {
                command: vec!["/bin/sh".into()],
                tty: Some(true),
                attach_stdin: Some(true),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
            })
            .await
            .unwrap();
        exec.resize(40, 12).await.unwrap();
        container.remove(false).await.unwrap();
        let volume = engine
            .volumes()
            .create(engine::opts::VolumeCreateOpts {
                name: Some("test-volume".into()),
            })
            .await
            .unwrap();
        engine.volumes().list().await.unwrap();
        engine.volumes().get(volume).remove(false).await.unwrap();

        let requests = daemon.requests();
        let create = requests
            .iter()
            .find(|req| req.path() == "/containers/create")
            .unwrap();
        let body = create.json();
        assert_eq!(create.method, "POST");
        assert!(create.target.contains("name=mobile-test"));
        assert_eq!(body["Image"], "alpine:latest");
        assert_eq!(body["HostConfig"]["Memory"], 134_217_728);
        assert_eq!(
            body["HostConfig"]["PortBindings"]["80/tcp"][0]["HostPort"],
            "8080"
        );
        assert_eq!(body["HostConfig"]["Mounts"][0]["ReadOnly"], true);
        assert_eq!(body["HostConfig"]["Mounts"][1]["Source"], "test-volume");
        assert_eq!(body["Env"][0], "LANG=zh_CN.UTF-8");
        let exec_request = requests
            .iter()
            .find(|req| req.path().ends_with("/exec"))
            .unwrap();
        assert_eq!(exec_request.json()["AttachStdin"], true);
        assert_eq!(exec_request.json()["Tty"], true);
        let resize = requests
            .iter()
            .find(|req| req.path() == "/exec/test-exec/resize")
            .unwrap();
        assert!(resize.target.contains("w=40"));
        assert!(resize.target.contains("h=12"));
        for suffix in ["start", "pause", "unpause", "restart", "stop", "rename"] {
            assert!(
                requests
                    .iter()
                    .any(|req| req.path() == format!("/containers/test-container/{suffix}"))
            );
        }
    });
}

#[test]
fn decodes_docker_logs_and_events() {
    run(async {
        let daemon = Daemon::new(|request| match request.path() {
            "/version" => version("Engine", "1.56"),
            "/containers/test/logs" => {
                let text = b"2026-11-24T00:00:00Z ready\n";
                let mut body = vec![1, 0, 0, 0];
                body.extend_from_slice(&(text.len() as u32).to_be_bytes());
                body.extend_from_slice(text);
                Response { status: 200, body }
            }
            "/events" => {
                let mut response = Response::json(json!({
                    "Type": "container", "Action": "stop",
                    "Actor": { "ID": "test", "Attributes": { "name": "test" } },
                    "time": 1,
                }));
                response.body.push(b'\n');
                response
            }
            _ => panic!("unexpected request: {request:?}"),
        });
        let engine = engine::Engine::new(&daemon.url).await.unwrap();
        let container = engine.containers().get("test");
        let mut logs = container.logs(engine::opts::LogsOpts {
            stdout: true,
            stderr: true,
            timestamps: true,
            tail: "512".into(),
            ..Default::default()
        });
        let chunk = logs.next().await.unwrap().unwrap();
        assert_eq!(chunk.as_ref(), b"2026-11-24T00:00:00Z ready\n");
        let mut events = engine.events();
        let event = events.next().await.unwrap().unwrap();
        assert!(matches!(event.type_(), engine::dto::EventType::Container));
        let request = daemon
            .requests()
            .into_iter()
            .find(|req| req.path().ends_with("/logs"))
            .unwrap();
        assert!(request.target.contains("tail=512"));
        assert!(request.target.contains("timestamps=true"));
    });
}

#[test]
fn limits_stats_concurrency_and_discards_stopped_container_counters() {
    run(async {
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let lists = Arc::new(AtomicUsize::new(0));
        let samples = AtomicUsize::new(0);
        let daemon = Daemon::new({
            let active = active.clone();
            let peak = peak.clone();
            move |request| match request.path() {
                "/version" => version("Engine", "1.56"),
                "/containers/json" => match lists.fetch_add(1, Ordering::Relaxed) {
                    0 => Response::json(Value::Array(
                        (0..7)
                            .map(|id| json!({ "Id": format!("container-{id}") }))
                            .collect(),
                    )),
                    1 => Response::json(json!([])),
                    _ => Response::json(json!([{ "Id": "container-0" }])),
                },
                path if path.ends_with("/stats") => {
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(60));
                    active.fetch_sub(1, Ordering::SeqCst);
                    let sample = samples.fetch_add(1, Ordering::Relaxed) + 1;
                    Response::json(json!({
                        "cpu_stats": { "cpu_usage": { "total_usage": sample * 100 }, "system_cpu_usage": sample * 1000, "online_cpus": 8 },
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
        assert!(
            stream
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_single_stats()
                .is_empty()
        );
        let resumed = stream.next().await.unwrap().unwrap().into_single_stats();
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].1.cpu, None);
    });
}
