#!/bin/sh
# Invoked as an ordinary user inside the disposable full-system QEMU guest.
set -u

if [ "$(id -u)" = 0 ]; then
    echo "Run the tests as an ordinary guest user." >&2
    exit 1
fi
prefix=${PODS_TEST_PREFIX:-/work/install}
export GSETTINGS_BACKEND=memory
export GSETTINGS_SCHEMA_DIR="$prefix/share/glib-2.0/schemas"
export XDG_DATA_DIRS="$prefix/share:/usr/local/share:/usr/share"
export XDG_CONFIG_HOME=/out/config
export XDG_RUNTIME_DIR=/run/user/1000
export GDK_BACKEND=x11
export GSK_RENDERER=cairo
export NO_AT_BRIDGE=1

mkdir -p "$XDG_CONFIG_HOME"
result=0
/opt/pods-tests/engine-contract --test-threads=1 --nocapture > /out/contracts.log 2>&1
status=$?
echo "CONTRACT_EXIT=$status"
[ "$status" = 0 ] || result=1
/opt/pods-tests/engine-contract denied_socket_retains_permission_denied_errno \
    --include-ignored --test-threads=1 --nocapture > /out/permissions.log 2>&1
status=$?
echo "PERMISSIONS_EXIT=$status"
[ "$status" = 0 ] || result=1
for locale in C.UTF-8 zh_CN.UTF-8; do
    export LANG=$locale
    unset LANGUAGE
    export PODS_UI_TEST_OUTPUT="/out/ui-$locale"
    case "$locale" in
        zh_CN.UTF-8) export PODS_UI_TEST_REGEX_LABEL=正则表达式 ;;
        *) export PODS_UI_TEST_REGEX_LABEL='Regular Expression' ;;
    esac
    mkdir -p "$PODS_UI_TEST_OUTPUT"
    dbus-run-session -- xvfb-run -a -s '-screen 0 1280x1024x24 -nolisten tcp' \
        /opt/pods-tests/pods ui_tests::mobile_widgets_fit_and_keep_controls_reachable \
        --ignored --test-threads=1 --nocapture > "/out/ui-$locale.log" 2>&1
    status=$?
    echo "UI_${locale}_EXIT=$status"
    [ "$status" = 0 ] || result=1
done

if command -v weston >/dev/null; then
    export LANG=C.UTF-8
    export PODS_UI_TEST_REGEX_LABEL='Regular Expression'
    export PODS_UI_TEST_OUTPUT=/out/ui-wayland
    export WAYLAND_DISPLAY=wayland-pods
    export GDK_BACKEND=wayland
    mkdir -p "$PODS_UI_TEST_OUTPUT"
    weston --backend=headless-backend.so --renderer=pixman \
        --shell=desktop-shell.so --socket="$WAYLAND_DISPLAY" \
        --width=1280 --height=1024 --idle-time=0 --no-config \
        --log=/out/weston.log &
    compositor_pid=$!
    for attempt in 1 2 3 4 5; do
        [ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ] && break
        sleep 1
    done
    dbus-run-session -- /opt/pods-tests/pods \
        ui_tests::mobile_widgets_fit_and_keep_controls_reachable \
        --ignored --test-threads=1 --nocapture > /out/ui-wayland.log 2>&1
    status=$?
    echo "UI_WAYLAND_EXIT=$status"
    [ "$status" = 0 ] || result=1
    kill "$compositor_pid" 2>/dev/null || true
    wait "$compositor_pid" 2>/dev/null || true
fi
exit "$result"
