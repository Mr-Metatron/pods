#!/bin/sh
# Run in disposable native Alpine containers. Build containers have /source
# read-only and /out writable, without a Docker socket or privileged QEMU.
set -eu

run_logged() {
    log=$1
    shift
    if "$@" > "$log" 2>&1; then
        cat "$log"
    else
        status=$?
        cat "$log" >&2
        return "$status"
    fi
}

collect_build_logs() {
    for directory in /home/builder/aport/pods/src/pods-*/output/meson-logs; do
        [ ! -d "$directory" ] || cp -a "$directory" /out/logs/
    done
}

# Use --list to distinguish the GTK test harness from the application binary.
find_test_binary() {
    binary=
    for candidate in "$builddir/output/src/release/deps/$1"-*; do
        [ -f "$candidate" ] && [ -x "$candidate" ] || continue
        if "$candidate" --list 2>/dev/null | grep -Fq "$2: test"; then
            if [ -n "$binary" ]; then
                echo "Multiple test binaries matched $1" >&2
                return 1
            fi
            binary=$candidate
        fi
    done
    if [ -z "$binary" ]; then
        echo "No compiled test binary matched $1" >&2
        return 1
    fi
    printf '%s\n' "$binary"
}

check_installed() {
    [ "$(id -u)" -ne 0 ]
    # Use the same distribution compiler flags as abuild. The package itself
    # runs metadata checks; CI separately compiles and runs mobile checks.
    sharedir=/usr/share/abuild
    set +u
    . "$sharedir/functions.sh"
    set -u
    srcdir=/home/builder/aport/pods/src
    . /home/builder/aport/pods/APKBUILD
    cd "$builddir"
    export CARGO_NET_OFFLINE=true
    run_logged /out/logs/mobile-harness-build.txt \
        cargo test --no-run --locked --offline --release \
        --target-dir "$builddir/output/src" \
        --bin pods --test engine_contract --test engine_live

    ui_test=ui_tests::mobile_widgets_fit_and_keep_controls_reachable
    ui_binary=$(find_test_binary pods "$ui_test")
    contract_binary=$(find_test_binary engine_contract \
        engine::socket::tests::denied_socket_retains_permission_denied_errno)
    live_binary=$(find_test_binary engine_live docker_local_connection_smoke)
    cp "$ui_binary" /out/tests/pods-ui
    cp "$contract_binary" /out/tests/engine_contract
    cp "$live_binary" /out/tests/engine_live
    (cd /out/tests && sha256sum pods-ui engine_contract engine_live > SHA256SUMS)

    id > /out/logs/test-user.txt
    run_logged /out/logs/engine-contract.txt \
        timeout 180 "$contract_binary" --include-ignored --test-threads=1 --nocapture

    export GSETTINGS_BACKEND=memory
    export GSETTINGS_SCHEMA_DIR=/usr/share/glib-2.0/schemas
    export GDK_BACKEND=x11
    export GSK_RENDERER=cairo
    export G_DEBUG=fatal-criticals
    export XDG_RUNTIME_DIR=/home/builder/runtime
    mkdir -p "$XDG_RUNTIME_DIR"
    chmod 700 "$XDG_RUNTIME_DIR"

    unset LANGUAGE LC_ALL LC_MESSAGES
    for language in en zh_CN; do
        case "$language" in
            en) export LANG=en_US.UTF-8 ;;
            zh_CN) export LANG=zh_CN.UTF-8 ;;
        esac
        translation=$(TEXTDOMAINDIR=/usr/share/locale TEXTDOMAIN=pods gettext 'Regular Expression')
        printf 'LANG=%s, LANGUAGE unset: %s\n' "$LANG" "$translation" \
            > "/out/logs/locale-$language.txt"
        case "$language" in
            en) [ "$translation" = 'Regular Expression' ] ;;
            zh_CN) [ "$translation" = '正则表达式' ] ;;
        esac
        export PODS_UI_TEST_REGEX_LABEL="$translation"
        export PODS_UI_TEST_OUTPUT="/out/screenshots/$language"
        export XDG_CONFIG_HOME="/home/builder/config-$language"
        mkdir -p "$PODS_UI_TEST_OUTPUT" "$XDG_CONFIG_HOME"
        run_logged "/out/logs/ui-$language.txt" \
            timeout 180 dbus-run-session -- \
            xvfb-run -a -s '-screen 0 1280x1024x24' \
            "$ui_binary" "$ui_test" --exact --ignored --test-threads=1 --nocapture
    done
    run_logged /out/logs/version.txt dbus-run-session -- pods --version
}

build_package() {
    apk add --no-cache \
        alpine-sdk appstream-glib blueprint-compiler ca-certificates dbus \
        desktop-file-utils file font-dejavu font-noto-cjk gettext-dev glib-dev \
        gtk4.0-dev gtksourceview5-dev libadwaita-dev libxml2-utils meson \
        musl-locales musl-locales-lang ninja pkgconf python3 vte3-dev cargo xvfb-run

    adduser -D builder
    addgroup builder abuild
    cp -a /source /worktree
    mkdir -p /out/packages /out/keys /out/logs /out/screenshots /out/tests
    chown -R builder:builder /worktree \
        /out/packages /out/keys /out/logs /out/screenshots /out/tests
    trap collect_build_logs EXIT

    su builder -c 'python3 /worktree/build-aux/prepare-alpine.py --output /home/builder/aport/pods'
    cp /home/builder/aport/pods/APKBUILD /out/APKBUILD
    su builder -c 'abuild-keygen -a -n -q'
    cp /home/builder/.abuild/*.pub /etc/apk/keys/
    cp /home/builder/.abuild/*.pub /out/keys/
    # Prerequisites are installed above. Package as a regular user and retain
    # build products; mobile checks run separately after package installation.
    su builder -c 'cd /home/builder/aport/pods && abuild -d -K'
    find /home/builder/packages -name '*.apk' -exec cp '{}' /out/packages/ \;
    apk add /out/packages/*.apk

    apk info --all pods > /out/package-info.txt
    apk list --installed > /out/installed-packages.txt
    file /usr/bin/pods > /out/elf.txt
    readelf -l /usr/bin/pods >> /out/elf.txt
    scanelf -n /usr/bin/pods > /out/shared-libraries.txt
    sha256sum /out/packages/*.apk > /out/SHA256SUMS
    su builder -c 'sh /worktree/build-aux/ci-alpine.sh check'
}

check_live_engine() {
    # Only this read-only smoke check uses the runner's explicitly selected
    # socket, after package installation and mobile checks.
    : "${PODS_TEST_UID:?set the ordinary runner UID}"
    [ "$PODS_TEST_UID" -ne 0 ]
    [ "$PODS_TEST_DOCKER_HOST" = unix:///var/run/docker.sock ]
    cp /out/keys/*.pub /etc/apk/keys/
    apk add --no-cache /out/packages/*.apk

    adduser -D -u "$PODS_TEST_UID" tester
    socket_gid=$(stat -c %g /var/run/docker.sock)
    socket_group=$(awk -F: -v gid="$socket_gid" '$3 == gid { print $1; exit }' /etc/group)
    if [ -z "$socket_group" ]; then
        socket_group=engine-test
        addgroup -g "$socket_gid" "$socket_group"
    fi
    # Supplementary membership exists only in this disposable container. The
    # host's socket mode, groups and daemon configuration are unchanged.
    addgroup tester "$socket_group"
    su tester -c id
    su tester -c 'timeout 60 /out/tests/engine_live docker_local_connection_smoke --exact --ignored --test-threads=1 --nocapture'
}

case "${1:-package}" in
    package) build_package ;;
    check) check_installed ;;
    live) check_live_engine ;;
    *) echo 'Usage: ci-alpine.sh [package|check|live]' >&2; exit 2 ;;
esac
