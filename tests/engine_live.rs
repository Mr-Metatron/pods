//! Opt-in, read-only checks of a phone's explicitly selected local Docker socket.
//! This smoke test does not create, change or delete daemon resources.
#![allow(dead_code, unused_imports)]

#[path = "../src/engine/mod.rs"]
mod engine;
#[path = "../src/gettext.rs"]
mod gettext;

use std::time::Duration;

#[test]
#[ignore = "requires an explicitly selected local Docker daemon"]
fn docker_local_connection_smoke() {
    let host = std::env::var("PODS_TEST_DOCKER_HOST")
        .expect("set PODS_TEST_DOCKER_HOST explicitly; no default socket is selected");
    assert!(
        host.starts_with("unix:///"),
        "an absolute local Unix socket URL is required"
    );
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        tokio::time::timeout(Duration::from_secs(30), async {
            let engine = engine::Engine::new(host).await?;
            anyhow::ensure!(matches!(engine, engine::Engine::Docker(_)), "Docker required for this smoke test");
            engine.ping().await?;
            let info = engine.info().await?;
            eprintln!("Daemon: arch={:?}, version={:?}, kernel={:?}", info.arch, info.version, info.kernel);
            let containers = engine.containers().list().await?;
            let images = engine.images().list().await?;
            let volumes = engine.volumes().list().await?;
            eprintln!("PASS local Docker connection/read access: {} containers, {} images, {} volumes",
                containers.len(), images.len(), volumes.len());
            Ok::<_, anyhow::Error>(())
        }).await.expect("local Docker connection smoke exceeded 30 seconds")
            .expect("local Docker connection/read access failed");
    });
}
