//! Opt-in Docker Engine acceptance through the same adapter used by the UI.
//!
//! PODS_TEST_DOCKER_HOST=unix:///path/to/test-docker.sock \
//! PODS_TEST_ISOLATED_DOCKER=1 cargo test --test engine_live \
//!   docker_engine_end_to_end -- --ignored --nocapture
//!
//! The daemon must see the test's current directory at the same absolute path
//! for bind-mount testing. PODS_TEST_SHARED_DIR can select another shared path.
//! Only uniquely named test objects are deleted. Global prune runs only with
//! PODS_TEST_ISOLATED_DOCKER=1. The pulled base image remains cached otherwise.
#![allow(dead_code, unused_imports)]

#[path = "../src/engine/mod.rs"]
mod engine;
#[path = "../src/gettext.rs"]
mod gettext;

use std::io::Read;
use std::io::Seek;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::ensure;
use futures::AsyncWriteExt;
use futures::StreamExt;

#[derive(Default)]
struct Resources {
    containers: Vec<String>,
    volumes: Vec<String>,
    image_tags: Vec<String>,
}

impl Resources {
    async fn cleanup(&self, engine: &engine::Engine) -> anyhow::Result<()> {
        let mut errors = Vec::new();
        for id in self.containers.iter().rev() {
            record_cleanup(
                &mut errors,
                &format!("container {id}"),
                tokio::time::timeout(
                    Duration::from_secs(10),
                    engine.containers().get(id).remove(true),
                )
                .await,
            );
        }
        for name in self.volumes.iter().rev() {
            record_cleanup(
                &mut errors,
                &format!("volume {name}"),
                tokio::time::timeout(
                    Duration::from_secs(10),
                    engine.volumes().get(name).remove(false),
                )
                .await,
            );
        }
        for tag in self.image_tags.iter().rev() {
            record_cleanup(
                &mut errors,
                &format!("image tag {tag}"),
                tokio::time::timeout(
                    Duration::from_secs(10),
                    engine.images().get(tag).remove(false),
                )
                .await,
            );
        }
        ensure!(
            errors.is_empty(),
            "test resource cleanup: {}",
            errors.join("; ")
        );
        Ok(())
    }
}

fn record_cleanup(
    errors: &mut Vec<String>,
    name: &str,
    result: Result<anyhow::Result<()>, tokio::time::error::Elapsed>,
) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error))
            if error.chain().any(|source| {
                matches!(
                    source.downcast_ref::<bollard::errors::Error>(),
                    Some(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }),
                )
            }) => {}
        Ok(Err(error)) => errors.push(format!("{name}: {error}")),
        Err(error) => errors.push(format!("{name}: {error}")),
    }
}

#[test]
#[ignore = "requires an explicitly selected real Docker daemon and registry access"]
fn docker_engine_end_to_end() {
    let host = std::env::var("PODS_TEST_DOCKER_HOST").expect(
        "set PODS_TEST_DOCKER_HOST explicitly; the default Docker socket is never selected",
    );
    assert!(
        host.starts_with("unix:///"),
        "local Unix Docker socket required"
    );
    let base_image =
        std::env::var("PODS_TEST_IMAGE").unwrap_or_else(|_| "alpine:latest".to_owned());
    let shared = std::env::var_os("PODS_TEST_SHARED_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let context = tempfile::Builder::new()
        .prefix("pods-engine-test-")
        .tempdir_in(shared)
        .unwrap();
    let prefix = format!(
        "pods-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let engine = engine::Engine::new(host)
            .await
            .expect("connect to selected Docker daemon");
        assert!(
            matches!(engine, engine::Engine::Docker(_)),
            "Docker required for this test"
        );
        let mut resources = Resources::default();
        let outcome = tokio::time::timeout(
            Duration::from_secs(180),
            exercise(
                &engine,
                &base_image,
                &prefix,
                context.path(),
                &mut resources,
            ),
        )
        .await
        .context("Docker acceptance exceeded 180 seconds")
        .and_then(|result| result);
        // Cleanup runs on success, API errors and the overall timeout.
        let cleanup = resources.cleanup(&engine).await;
        if let Err(error) = &cleanup {
            eprintln!("CLEANUP FAILED for {prefix}: {error}");
        }
        outcome.expect("Docker acceptance failed (cleanup was attempted)");
        cleanup.expect("Docker acceptance succeeded but resource cleanup failed");

        if std::env::var("PODS_TEST_ISOLATED_DOCKER").as_deref() == Ok("1") {
            engine.containers().prune(Default::default()).await.unwrap();
            engine.images().prune(Default::default()).await.unwrap();
            engine
                .volumes()
                .prune(engine::opts::VolumesPruneOpts {
                    all: true,
                    until: None,
                })
                .await
                .unwrap();
            eprintln!("PASS isolated container/image/volume prune");
        } else {
            eprintln!("SKIP global prune: PODS_TEST_ISOLATED_DOCKER is not 1");
        }
    });
}

async fn exercise(
    engine: &engine::Engine,
    base_image: &str,
    prefix: &str,
    context: &std::path::Path,
    resources: &mut Resources,
) -> anyhow::Result<()> {
    engine.ping().await?;
    let info = engine.info().await?;
    eprintln!(
        "Daemon: arch={:?}, version={:?}, kernel={:?}",
        info.arch, info.version, info.kernel
    );

    let images = engine.images();
    let mut pull = images.pull(engine::opts::ImagePullOpts {
        reference: base_image.into(),
    });
    while let Some(report) = pull.next().await {
        if let engine::dto::ImagePullReport::Error { message } = report? {
            anyhow::bail!("image pull: {message}");
        }
    }
    images.get(base_image).inspect().await?;
    images.get(base_image).history().await?;
    images.get(base_image).json().await?;
    images.list().await?;
    eprintln!("PASS image pull/list/inspect/history");

    let volume_name = format!("{prefix}-data");
    resources.volumes.push(volume_name.clone());
    let volume_name = engine
        .volumes()
        .create(engine::opts::VolumeCreateOpts {
            name: Some(volume_name),
        })
        .await?;
    engine.volumes().get(&volume_name).inspect().await?;
    engine.volumes().get(&volume_name).json().await?;
    engine.volumes().list().await?;

    std::fs::write(context.join("bind.txt"), "pods-bind-ready\n")?;
    resources.containers.push(prefix.to_owned());
    let id = engine.containers().create(engine::opts::ContainerCreateOpts {
        name: prefix.into(),
        image: base_image.into(),
        cmd: Some(vec!["sh".into(), "-c".into(),
            "cat /pods-bind/bind.txt; echo pods-volume-ready > /pods-volume/marker; echo pods-log-ready; trap 'exit 0' TERM; while :; do sleep 1; done".into()]),
        terminal: false,
        env: [("PODS_TEST".into(), "1".into())].into(),
        labels: [("com.github.marhkb.Pods.test".into(), prefix.into())].into(),
        memory_limit: Some(128 * 1024 * 1024),
        ports: vec![engine::dto::PortBinding {
            host_ip: String::new(), host_port: None, target_port: 8080,
            protocol: engine::dto::PortMappingProtocol::Tcp,
        }],
        mounts: vec![engine::opts::ContainerCreateMountOpts {
            host_path: context.to_string_lossy().into(), container_path: "/pods-bind".into(),
            read_only: true, selinux: Default::default(),
        }],
        volumes: vec![engine::opts::ContainerCreateVolumeOpts {
            volume: volume_name, container_path: "/pods-volume".into(), ..Default::default()
        }],
        ..Default::default()
    }).await?;
    *resources.containers.last_mut().unwrap() = id.clone();
    let container = engine.containers().get(&id);
    container.start().await?;
    engine.containers().list().await?;
    container.inspect().await?;
    let inspection: serde_json::Value = serde_json::from_str(&container.json().await?)?;
    ensure!(
        inspection["HostConfig"]["Memory"] == 134_217_728,
        "memory limit was not applied"
    );
    ensure!(
        inspection["NetworkSettings"]["Ports"]["8080/tcp"].is_array(),
        "port was not published"
    );

    let mut logs = container.logs(engine::opts::LogsOpts {
        follow: true,
        stdout: true,
        stderr: true,
        tail: "50".into(),
        ..Default::default()
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut output = Vec::new();
        while let Some(chunk) = logs.next().await {
            output.extend_from_slice(chunk?.as_ref());
            if String::from_utf8_lossy(&output).contains("pods-log-ready") {
                ensure!(
                    String::from_utf8_lossy(&output).contains("pods-bind-ready"),
                    "bind mount was not readable"
                );
                return Ok::<_, anyhow::Error>(());
            }
            ensure!(output.len() < 64 * 1024, "unexpected excessive test logs");
        }
        anyhow::bail!("log stream ended before marker");
    })
    .await??;
    drop(logs);
    eprintln!("PASS create/start/inspect/port/bind/volume/logs");

    let containers = engine.containers();
    let mut stats = containers.stats_stream(1);
    let snapshot = tokio::time::timeout(Duration::from_secs(10), stats.next())
        .await?
        .context("stats stream ended")??;
    ensure!(
        snapshot
            .into_single_stats()
            .iter()
            .any(|(container_id, _)| container_id == &id),
        "test container absent from stats"
    );
    let mut top = container.top_stream(1);
    let processes = tokio::time::timeout(Duration::from_secs(10), top.next())
        .await?
        .context("top stream ended")??;
    ensure!(
        !processes.processes().is_empty(),
        "container process list is empty"
    );
    drop(stats);
    drop(top);
    eprintln!("PASS stats/top");

    let exec = container
        .create_exec(engine::opts::ExecCreateOpts {
            command: vec!["/bin/sh".into()],
            tty: Some(true),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
        })
        .await?;
    let attached = exec
        .start(true)
        .await?
        .into_attached()
        .context("exec did not attach")?;
    let (mut reader, mut writer) = attached.split();
    exec.resize(40, 12).await?;
    writer
        .write_all(b"cat /pods-volume/marker; printf 'pods-exec-%s\\n' ready; exit\n")
        .await?;
    writer.flush().await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut output = Vec::new();
        while let Some(chunk) = reader.next().await {
            output.extend_from_slice(chunk?.as_ref());
            let text = String::from_utf8_lossy(&output);
            if text.contains("pods-exec-ready") && text.contains("pods-volume-ready") {
                return Ok::<_, anyhow::Error>(());
            }
            ensure!(output.len() < 64 * 1024, "unexpected excessive exec output");
        }
        anyhow::bail!("exec ended before output markers");
    })
    .await??;
    drop(writer);
    drop(reader);
    eprintln!("PASS interactive exec/input/output/resize");

    let mut archive = tar::Builder::new(tempfile::tempfile_in(context)?);
    let payload = b"pods-copy-ready\n";
    let mut header = tar::Header::new_gnu();
    header.set_path("pods-copy.txt")?;
    header.set_size(payload.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append(&header, &payload[..])?;
    let mut file = archive.into_inner()?;
    file.rewind()?;
    container
        .copy_to("/tmp".into(), tokio::fs::File::from_std(file))
        .await?;
    let mut download = container.copy_from("/tmp/pods-copy.txt".into());
    let mut downloaded = Vec::new();
    while let Some(chunk) = download.next().await {
        downloaded.extend_from_slice(&chunk?);
        ensure!(
            downloaded.len() < 1024 * 1024,
            "unexpected excessive copy response"
        );
    }
    let mut archive = tar::Archive::new(&downloaded[..]);
    let mut found = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.ends_with("pods-copy.txt") {
            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;
            ensure!(content == payload, "copied contents differ");
            found = true;
        }
    }
    ensure!(found, "downloaded archive is missing copied file");
    eprintln!("PASS copy to/from container");

    // Generate rename events until the streaming subscription is established.
    let mut events = engine.events();
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut tick = tokio::time::interval(Duration::from_millis(200));
        let mut serial = 0;
        loop {
            tokio::select! {
                event = events.next() => {
                    let event = event.context("events stream ended")??;
                    if let engine::Response::Docker(event) = event
                        && event.actor.and_then(|actor| actor.id).as_deref() == Some(id.as_str())
                        && event.action.as_deref() == Some("rename")
                    {
                        return Ok::<_, anyhow::Error>(());
                    }
                }
                _ = tick.tick() => {
                    serial += 1;
                    container.rename(&format!("{prefix}-{serial}")).await?;
                }
            }
        }
    })
    .await??;
    drop(events);
    container.pause().await?;
    container.unpause().await?;
    container.restart(false).await?;
    container.stop().await?;
    container.start().await?;
    eprintln!("PASS events/rename/pause/unpause/restart/stop/start");

    let committed = format!("{prefix}-commit:latest");
    resources.image_tags.push(committed.clone());
    container
        .commit(engine::opts::ContainerCommitOpts {
            repo: Some(format!("{prefix}-commit")),
            tag: Some("latest".into()),
            pause: true,
            ..Default::default()
        })
        .await?;
    let image = images.get(&committed);
    image.inspect().await?;
    let tag_repo = format!("{prefix}-alias");
    resources.image_tags.push(format!("{tag_repo}:latest"));
    image.tag(tag_repo.clone(), "latest".into()).await?;
    image.untag(&tag_repo, "latest").await?;
    resources.image_tags.pop();
    eprintln!("PASS commit/tag/untag");

    std::fs::write(
        context.join("Dockerfile"),
        format!("FROM {base_image}\nCOPY bind.txt /pods-built-marker\n"),
    )?;
    let built = format!("{prefix}-build:latest");
    resources.image_tags.push(built.clone());
    let mut build = images.build(engine::opts::ImageBuildOpts {
        dockerfile: "Dockerfile".into(),
        path: context.to_string_lossy().into(),
        tag: built.clone(),
        ..Default::default()
    })?;
    let mut finished = false;
    while let Some(report) = build.next().await {
        match report? {
            engine::dto::ImageBuildReport::Error { message } => {
                anyhow::bail!("image build: {message}")
            }
            engine::dto::ImageBuildReport::Finished { .. } => finished = true,
            engine::dto::ImageBuildReport::Streaming { .. } => {}
        }
    }
    ensure!(finished, "build stream did not report completion");
    images.get(built).inspect().await?;
    eprintln!("PASS image build (native daemon architecture)");
    eprintln!(
        "SKIP registry push/search: external registry credentials and network policy require separate acceptance"
    );
    Ok(())
}
