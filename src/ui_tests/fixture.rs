//! Read-only Docker responses for exercising populated mobile views through the
//! existing model refresh methods. No production setters or daemon are needed.

use std::io::Read;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;

use serde_json::Value;
use serde_json::json;

use super::*;

const IMAGE_ID: &str = "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const IMAGE_NAME: &str = "registry.example.test/mobile/long-application-image-name:development";
const VOLUME_NAME: &str = "mobile-application-persistent-data-with-a-long-volume-name";
const CONTAINER_IDS: [&str; 3] = [
    "1111111111111111111111111111111111111111111111111111111111111111",
    "2222222222222222222222222222222222222222222222222222222222222222",
    "3333333333333333333333333333333333333333333333333333333333333333",
];

pub(super) struct Fixture {
    client: model::Client,
    manager: model::ConnectionManager,
    _connection: model::Connection,
    _directory: tempfile::TempDir,
    requests: Arc<Mutex<Vec<String>>>,
    unexpected: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Fixture {
    pub(super) fn new(directory: &Path) -> Self {
        Self::with_empty_lists(directory, false)
    }

    pub(super) fn empty(directory: &Path) -> Self {
        Self::with_empty_lists(directory, true)
    }

    fn with_empty_lists(directory: &Path, empty: bool) -> Self {
        let directory = tempfile::tempdir_in(directory).unwrap();
        let socket_path = directory.path().join("engine.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let unexpected = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let requests = requests.clone();
            let unexpected = unexpected.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut socket, _)) => {
                            socket
                                .set_read_timeout(Some(Duration::from_secs(2)))
                                .unwrap();
                            if let Some((method, target)) = read_request(&mut socket) {
                                let request = format!("{method} {target}");
                                requests.lock().unwrap().push(request.clone());
                                let path = target.split('?').next().unwrap();
                                let path = if path.starts_with("/v1.") {
                                    &path[path[1..].find('/').unwrap() + 1..]
                                } else {
                                    path
                                };
                                let response = (method == "GET")
                                    .then(|| response(path, &target, empty))
                                    .flatten();
                                let (status, body) = match response {
                                    Some(body) => (200, body),
                                    None => {
                                        unexpected.lock().unwrap().push(request);
                                        (
                                            404,
                                            br#"{"message":"Unexpected UI fixture request"}"#
                                                .to_vec(),
                                        )
                                    }
                                };
                                // A view may be closed while its read finishes.
                                let _ = write!(
                                    socket,
                                    "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                let _ = socket.write_all(&body);
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("UI fixture accept: {error}"),
                    }
                }
            })
        };
        let manager = model::ConnectionManager::default();
        let connection = model::Connection::new(
            "ui-test-populated",
            "本机 Docker — a deliberately long connection name",
            &format!("unix://{}", socket_path.display()),
            None,
            &manager,
        );
        let client: model::Client = glib::Object::builder()
            .property("connection", &connection)
            .property(
                "engine",
                model::Engine::from(engine::Engine::Docker(
                    bollard::Docker::connect_with_host(connection.url().as_str()).unwrap(),
                )),
            )
            .build();
        client
            .images()
            .refresh(|error| panic!("UI images fixture: {error}"));
        client
            .containers()
            .refresh(|error| panic!("UI containers fixture: {error}"));
        client
            .volumes()
            .refresh(|error| panic!("UI volumes fixture: {error}"));
        let deadline = Instant::now() + Duration::from_secs(10);
        while !client.images().initialized()
            || !client.containers().initialized()
            || !client.volumes().initialized()
        {
            assert!(Instant::now() < deadline, "UI fixture refresh timed out");
            settle();
        }
        assert_eq!(client.images().n_items(), if empty { 0 } else { 1 });
        assert_eq!(client.containers().n_items(), if empty { 0 } else { 3 });
        assert_eq!(client.volumes().n_items(), if empty { 0 } else { 1 });
        if !empty {
            for (index, id) in CONTAINER_IDS.iter().enumerate() {
                let inspection = serde_json::from_value(container_inspection(index)).unwrap();
                client
                    .containers()
                    .get_container(id)
                    .unwrap()
                    .update_from_inspection(engine::dto::ContainerInspection::from_docker(
                        Some(IMAGE_NAME.into()),
                        inspection,
                    ));
            }
            let image = client.images().get_image(IMAGE_ID).unwrap();
            image.update_from_inspection(engine::dto::ImageInspection::from(
                serde_json::from_value::<bollard::plugin::ImageInspect>(image_inspection())
                    .unwrap(),
            ));
        }
        Self {
            client,
            manager,
            _connection: connection,
            _directory: directory,
            requests,
            unexpected,
            stop,
            worker: Some(worker),
        }
    }

    pub(super) fn client(&self) -> &model::Client {
        &self.client
    }

    pub(super) fn manager(&self) -> &model::ConnectionManager {
        &self.manager
    }

    pub(super) fn containers(&self) -> Vec<model::Container> {
        CONTAINER_IDS
            .iter()
            .map(|id| self.client.containers().get_container(id).unwrap())
            .collect()
    }

    pub(super) fn container(&self) -> model::Container {
        self.containers().remove(0)
    }

    pub(super) fn image(&self) -> model::Image {
        self.client.images().get_image(IMAGE_ID).unwrap()
    }

    pub(super) fn volume(&self) -> model::Volume {
        self.client.volumes().get_volume(VOLUME_NAME).unwrap()
    }

    pub(super) fn assert_requests(&self) {
        assert!(
            self.unexpected.lock().unwrap().is_empty(),
            "Unexpected UI fixture requests: {:?}",
            self.unexpected.lock().unwrap()
        );
        eprintln!(
            "Populated UI fixture handled {} read-only requests",
            self.requests.lock().unwrap().len()
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn read_request(socket: &mut UnixStream) -> Option<(String, String)> {
    let mut bytes = Vec::new();
    while !bytes.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        let mut buffer = [0; 4096];
        let length = socket.read(&mut buffer).ok()?;
        if length == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..length]);
        assert!(bytes.len() <= 64 * 1024, "oversized UI fixture request");
    }
    let headers = String::from_utf8(bytes).unwrap();
    let mut parts = headers.lines().next().unwrap().split_whitespace();
    Some((parts.next().unwrap().into(), parts.next().unwrap().into()))
}

fn response(path: &str, target: &str, empty: bool) -> Option<Vec<u8>> {
    let value = match path {
        "/images/json" | "/containers/json" if empty => json!([]),
        "/volumes" if empty => json!({"Volumes": [], "Warnings": []}),
        "/images/json" => json!([{
            "Id": IMAGE_ID, "ParentId": "", "RepoTags": tags(), "RepoDigests": [],
            "Created": 1700000000, "Size": 268435456, "SharedSize": -1, "Labels": {}, "Containers": 3
        }]),
        "/containers/json" => Value::Array(
            (0..3)
                .filter(|index| {
                    !target.contains("filters=") || target.contains(CONTAINER_IDS[*index])
                })
                .map(container_summary)
                .collect(),
        ),
        "/volumes" => json!({"Volumes": [volume()], "Warnings": []}),
        "/images/search" => json!([]),
        _ if path.starts_with("/images/") && path.ends_with("/json") => image_inspection(),
        _ if path.starts_with("/volumes/") => volume(),
        _ if path.starts_with("/containers/") && path.ends_with("/json") => {
            let index = CONTAINER_IDS
                .iter()
                .position(|id| path.contains(id))
                .unwrap();
            container_inspection(index)
        }
        _ if path.starts_with("/containers/") && path.ends_with("/top") => json!({
            "Titles": ["USER", "PID", "PPID", "%CPU", "ELAPSED", "TTY", "TIME", "COMMAND"],
            "Processes": [
                ["mobile-user-with-a-long-name", "101", "1", "1.5", "01:23", "pts/0", "00:02", "/usr/bin/service-with-a-long-command --description=手机服务"],
                ["root", "202", "1", "0.0", "10:00", "?", "00:00", "/usr/bin/secondary-service"]
            ]
        }),
        _ if path.starts_with("/containers/") && path.ends_with("/logs") => {
            if target
                .split_once('?')
                .map(|(_, query)| query)
                .unwrap_or("")
                .split('&')
                .any(|part| {
                    part == "follow=true"
                        || part
                            .strip_prefix("until=")
                            .is_some_and(|until| until != "0")
                })
            {
                return Some(Vec::new());
            }
            let mut body = Vec::new();
            for index in 0..24 {
                let line = format!(
                    "2026-11-24T08:00:{index:02}.000000000Z 日志 {index}: long application output for portrait scrolling and search\n"
                );
                body.extend_from_slice(&[1, 0, 0, 0]);
                body.extend_from_slice(&(line.len() as u32).to_be_bytes());
                body.extend_from_slice(line.as_bytes());
            }
            return Some(body);
        }
        _ => return None,
    };
    Some(serde_json::to_vec(&value).unwrap())
}

fn tags() -> Value {
    json!([
        IMAGE_NAME,
        "registry.example.test/mobile/another-long-repository:stable",
        "localhost/mobile:test"
    ])
}

fn mounts() -> Value {
    json!([{"Type": "volume", "Name": VOLUME_NAME,
        "Source": format!("/var/lib/docker/volumes/{VOLUME_NAME}/_data"),
        "Destination": "/var/lib/application/persistent-data", "Mode": "rw", "RW": true}])
}

fn container_summary(index: usize) -> Value {
    json!({
        "Id": CONTAINER_IDS[index], "Names": [format!("/mobile-application-with-a-long-container-name-{index}")],
        "Image": IMAGE_NAME, "ImageID": IMAGE_ID, "Created": 1700000000,
        "State": (["running", "exited", "dead"][index]), "Mounts": mounts(),
        "Ports": [{"IP": "127.0.0.1", "PrivatePort": 8080, "PublicPort": 18080, "Type": "tcp"}],
        "Health": {"Status": "healthy"}
    })
}

fn container_inspection(index: usize) -> Value {
    json!({
        "Id": CONTAINER_IDS[index], "Name": format!("/mobile-application-with-a-long-container-name-{index}"),
        "Image": IMAGE_ID, "Created": "2023-11-14T22:13:20Z", "Mounts": mounts(),
        "Config": {"Image": IMAGE_NAME, "Labels": {"description": "用于测试移动端详情与检查页面的长中文说明"},
            "Healthcheck": {"Test": ["CMD", "true"], "Interval": 30000000000i64, "Timeout": 5000000000i64, "Retries": 3}},
        "HostConfig": {"RestartPolicy": {"Name": "unless-stopped", "MaximumRetryCount": 0}},
        "NetworkSettings": {"Ports": {"8080/tcp": [{"HostIp": "127.0.0.1", "HostPort": "18080"}]}},
        "State": {"Status": (["running", "exited", "dead"][index]), "StartedAt": "2026-11-24T08:00:00Z",
            "Health": {"Status": "healthy", "FailingStreak": 0, "Log": [{"Start": "2026-11-24T08:00:00Z",
                "End": "2026-11-24T08:00:01Z", "ExitCode": 0, "Output": "健康检查通过 — long health check output"}]}},
        "SizeRootFs": 268435456, "SizeRw": 4096
    })
}

fn image_inspection() -> Value {
    json!({
        "Id": IMAGE_ID, "RepoTags": tags(), "Created": "2023-11-14T22:13:20Z", "Size": 268435456,
        "Architecture": "aarch64", "Author": "移动应用测试 / an author with a long display name",
        "Comment": "用于验证长说明、命令与仓库标签在手机竖屏下的布局。A long image description for mobile layout checks.",
        "Config": {"Cmd": ["/usr/bin/application", "--a-long-command-line-option"],
            "Entrypoint": ["/usr/local/bin/container-entrypoint"], "ExposedPorts": {"8080/tcp": {}, "8443/tcp": {}}}
    })
}

fn volume() -> Value {
    json!({"Name": VOLUME_NAME, "Driver": "local",
        "Mountpoint": format!("/var/lib/docker/volumes/{VOLUME_NAME}/_data"),
        "CreatedAt": "2023-11-14T22:13:20Z", "Labels": {}, "Scope": "local", "Options": {}})
}
