#!/bin/sh
# Invoked as an ordinary user inside the disposable full-system QEMU guest.
set -u

if [ "$(id -u)" = 0 ]; then
    echo "Run the tests as an ordinary guest user." >&2
    exit 1
fi
prefix=${PODS_TEST_PREFIX:-/work/install}
output=${PODS_UI_TEST_OUTPUT_ROOT:-/out}
test_name=${PODS_UI_TEST_NAME:-ui_tests::mobile_widgets_fit_and_keep_controls_reachable}
export GSETTINGS_BACKEND=memory
export GSETTINGS_SCHEMA_DIR="$prefix/share/glib-2.0/schemas"
export XDG_DATA_DIRS="$prefix/share:/usr/local/share:/usr/share"
export XDG_CONFIG_HOME="$output/config"
export XDG_RUNTIME_DIR=/run/user/1000
export GDK_BACKEND=x11
export GSK_RENDERER=cairo
export G_DEBUG=fatal-criticals
export NO_AT_BRIDGE=1

run_ui() {
    log=$1
    shift
    status_file="$log.exit"
    rm -f "$status_file"
    (
        "$@"
        printf '%s\n' "$?" > "$status_file"
    ) 2>&1 | tee "$log"
    [ -f "$status_file" ] || return 1
    return "$(cat "$status_file")"
}

mkdir -p "$XDG_CONFIG_HOME"
if [ "${1:-}" = --wayland ]; then
    export LANG=C.UTF-8
    unset LANGUAGE
    export PODS_UI_TEST_REGEX_LABEL='Regular Expression'
    export PODS_UI_TEST_OUTPUT="$output/mobile-ui-wayland"
    export PODS_UI_TEST_INPUT_HELPER=/opt/pods-tests/guest-wayland-input.sh
    export WAYLAND_DISPLAY=wayland-pods
    export GDK_BACKEND=wayland
    mkdir -p "$PODS_UI_TEST_OUTPUT"
    # Xvfb supplies Weston with an input seat. The application still uses native
    # Wayland; the helper sends guest XTest events through the nested compositor.
    weston --backend=x11-backend.so --renderer=pixman \
        --shell=desktop-shell.so --socket="$WAYLAND_DISPLAY" \
        --width="${PODS_TEST_WAYLAND_WIDTH:-1280}" \
        --height="${PODS_TEST_WAYLAND_HEIGHT:-1024}" --idle-time=0 --no-config \
        --log="$output/weston.log" &
    compositor_pid=$!
    for attempt in 1 2 3 4 5; do
        [ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ] && break
        sleep 1
    done
    run_ui "$output/mobile-ui-wayland.log" dbus-run-session -- \
        timeout -k 5 "${PODS_UI_TEST_TIMEOUT:-600}" /opt/pods-tests/pods \
        "$test_name" \
        --exact --ignored --test-threads=1 --nocapture
    status=$?
    echo "UI_WAYLAND_EXIT=$status"
    kill "$compositor_pid" 2>/dev/null || true
    wait "$compositor_pid" 2>/dev/null || true
    exit "$status"
fi

result=0
locales=${PODS_TEST_UI_LOCALES-"C.UTF-8 zh_CN.UTF-8"}
for locale in $locales; do
    export LANG=$locale
    unset LANGUAGE
    export PODS_UI_TEST_OUTPUT="$output/mobile-ui-$locale"
    case "$locale" in
        zh_CN.UTF-8) export PODS_UI_TEST_REGEX_LABEL=正则表达式 ;;
        *) export PODS_UI_TEST_REGEX_LABEL='Regular Expression' ;;
    esac
    mkdir -p "$PODS_UI_TEST_OUTPUT"
    run_ui "$output/mobile-ui-$locale.log" \
        dbus-run-session -- xvfb-run -a -s '-screen 0 1280x1024x24 -nolisten tcp' \
        timeout -k 5 "${PODS_UI_TEST_TIMEOUT:-600}" /opt/pods-tests/pods "$test_name" \
        --exact --ignored --test-threads=1 --nocapture
    status=$?
    echo "UI_${locale}_EXIT=$status"
    [ "$status" = 0 ] || result=1
done

if [ "${PODS_TEST_WAYLAND:-1}" = 1 ] && command -v weston >/dev/null; then
    xvfb-run -a -s '-screen 0 1280x1024x24 -nolisten tcp' "$0" --wayland
    [ "$?" = 0 ] || result=1
fi
exit "$result"
