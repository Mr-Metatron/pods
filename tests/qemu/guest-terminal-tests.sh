#!/bin/sh
# Opt-in live VTE checks in a disposable full-system guest. The application runs
# as an ordinary user against a dedicated daemon and a single temporary container.
set -eu
[ "${PODS_TEST_GUEST:-0}" = 1 ]
[ "$(id -u)" = 0 ]
test_user=${PODS_TEST_USER:-tester}
[ "$(id -u "$test_user")" != 0 ]
: "${PODS_TEST_BASE_ROOTFS:?Supply the Alpine minirootfs matching this guest architecture}"
logs=${PODS_UI_TEST_OUTPUT_ROOT:-/out/live-terminal}
runtime=/run/pods-ui-terminal
socket=/run/pods-ui-terminal.sock
[ ! -e "$socket" ]
state=$(mktemp -d /var/lib/pods-ui-terminal.XXXXXX)
mkdir -p "$logs" "$runtime" "$state" /sys/fs/cgroup
chown "$test_user" "$logs"
if [ ! -f /sys/fs/cgroup/cgroup.controllers ]; then
    mount -t cgroup2 cgroup2 /sys/fs/cgroup
fi
uname -a > "$logs/platform.txt"
printf 'Dedicated data root: %s\n' "$state" >> "$logs/platform.txt"
sha256sum "$PODS_TEST_BASE_ROOTFS" >> "$logs/platform.txt"
container_id=
image_id=
daemon_pid=
cleanup() {
    if [ -n "$container_id" ]; then
        docker --host "unix://$socket" rm -f "$container_id" >/dev/null 2>&1 || true
    fi
    if [ -n "$image_id" ]; then
        docker --host "unix://$socket" image rm "$image_id" >/dev/null 2>&1 || true
    fi
    if [ -n "$daemon_pid" ]; then
        kill "$daemon_pid" 2>/dev/null || true
        wait "$daemon_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM
printf '%s\n' '{"features":{"containerd-snapshotter":false}}' > "$runtime/daemon.json"
dockerd --config-file "$runtime/daemon.json" --host "unix://$socket" \
    --data-root "$state" --exec-root "$runtime/exec" --pidfile "$runtime/dockerd.pid" \
    --storage-driver vfs --bridge none --iptables=false --ip6tables=false \
    > "$logs/dockerd.log" 2>&1 &
daemon_pid=$!
ready=0
for attempt in $(seq 1 60); do
    if docker --host "unix://$socket" info > "$logs/docker-info.txt" 2>&1; then
        ready=1
        break
    fi
    kill -0 "$daemon_pid" 2>/dev/null || break
    sleep 1
done
[ "$ready" = 1 ] || { cat "$logs/dockerd.log" >&2; exit 1; }
chgrp "$(id -g "$test_user")" "$socket"
chmod 660 "$socket"
stat -c '%a %u %g %n' "$socket" > "$logs/socket-permissions.txt"
docker --host "unix://$socket" version > "$logs/docker-version.txt"
image_id=$(docker --host "unix://$socket" import "$PODS_TEST_BASE_ROOTFS")
container_id=$(docker --host "unix://$socket" run -d --network none \
    --name pods-ui-terminal --label io.github.marhkb.pods.mobile-ui-test=true \
    "$image_id" /bin/sh -c 'while :; do sleep 60; done')
docker --host "unix://$socket" inspect "$container_id" > "$logs/container.json"
export PODS_TEST_DOCKER_HOST="unix://$socket"
export PODS_TEST_ISOLATED_DOCKER=1
export PODS_UI_TEST_CONTAINER_ID="$container_id"
export PODS_UI_TEST_NAME=ui_tests::terminal::live_terminal_keeps_input_and_resize
export PODS_UI_TEST_OUTPUT_ROOT="$logs"
export PODS_TEST_UI_LOCALES=C.UTF-8
export PODS_TEST_WAYLAND=1
su -s /bin/sh -c '/opt/pods-tests/guest-ui-tests.sh' "$test_user"
