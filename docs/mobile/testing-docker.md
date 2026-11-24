# Docker validation

Run these commands from the repository root after installing the native build dependencies. They exercise the Rust engine adapter used by Pods; the UI, compositor, keyboard and Pixel 2 checks are separate in the mobile acceptance report.

## Protocol and unit tests

```sh
cargo fetch --locked
meson setup _build --prefix=/usr -Dcargo-offline=true
cargo test --locked --offline --target-dir _build/src --test engine_contract
```

The contract suite starts temporary Unix HTTP endpoints. It checks engine identification, Docker API negotiation, Docker/Podman capabilities, container creation options, lifecycle requests, volume requests, exec creation and resizing, log/event decoding, the statistics concurrency limit and disposal of stale CPU counters. The tests call the production adapter and record the SDK's actual HTTP targets with `--nocapture`.

The socket unit tests also check rootless discovery and the distinction between an absent path, a regular file, an active socket and a stale socket. A restricted execution environment that denies `socket(AF_UNIX)` will fail these tests before the HTTP fixture can start. Record that environment failure separately from a protocol test result.

To test a real filesystem access denial, run the following as an ordinary user with no DAC override capability:

```sh
cargo test --locked --offline --target-dir _build/src --test engine_contract \
  denied_socket_retains_permission_denied_errno -- --ignored --nocapture
```

This creates a private temporary socket, removes its permission bits, attempts to connect through the production helper, verifies `PermissionDenied`, and restores permissions before cleanup. It does not change a Docker socket or user groups.

## Real Docker Engine acceptance

Choose the daemon explicitly. The test never falls back to the default Docker socket. The example keeps global prune disabled, creates uniquely named resources, and removes those resources after success, API failure or the overall timeout.

```sh
PODS_TEST_DOCKER_HOST=unix:///var/run/docker.sock \
PODS_TEST_ISOLATED_DOCKER=0 \
PODS_TEST_IMAGE=alpine:latest \
cargo test --locked --offline --target-dir _build/src --test engine_live \
  docker_engine_end_to_end -- --ignored --nocapture
```

This test requires registry access to pull the selected image. It leaves that base image in the daemon's cache. Test containers, volumes and committed/built image tags use a unique `pods-test-...` prefix, which is included in cleanup errors.

The test verifies:

- Image pull, listing, inspect, JSON inspection and history.
- Container creation with environment variables, a memory limit, a published port, a read-only bind mount and a named volume.
- Running logs, container statistics and process listing.
- An attached interactive exec, text input/output and terminal resize.
- Streaming copy into a container and reading the copied archive back.
- Event delivery, rename, pause/unpause, restart, stop and subsequent start.
- Commit, tag/untag and image building using the daemon's native architecture.
- Removal of the created test resources during cleanup.

Registry search and authenticated push have separate acceptance requirements and are explicitly reported as skipped by this test. UI multi-selection, full-screen terminal behaviour, rotation, soft keyboard interaction and suspend/resume are device checks.

For bind mounts, Docker must see the test's current directory at the same absolute path. When running the tests inside a build container, mount the workspace at its original host path and set that path as the container's working directory. `PODS_TEST_SHARED_DIR` can select another writable directory that both the test process and daemon see at the same path.

For a disposable daemon with its own data directory and no other workloads, set `PODS_TEST_ISOLATED_DOCKER=1` to additionally exercise container, image and volume prune after resource cleanup. Keep this value at `0` when using a shared development or CI daemon.

## Reporting results

Save the full command output, test commit, architecture, libc, GTK/libadwaita versions and Docker version. The real-daemon test prints the daemon architecture, version and kernel, followed by the completed checks. A failure before the first `PASS` marker establishes only the connection/setup failure. Record the last completed marker and cleanup result.

Protocol tests establish request/response behaviour against fixtures. Native daemon tests establish the exercised engine operations on that host. Pixel 2 acceptance additionally requires the application's actual Wayland/Plasma session, touch and Chinese input, rotation, socket group changes after a fresh login, and performance measurements on the phone.
