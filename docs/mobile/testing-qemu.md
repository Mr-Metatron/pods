# Mobile UI checks in full-system QEMU

The native test harness can run in a disposable Alpine guest with its own Linux
kernel. This is useful when the development host cannot create Unix sockets.
The host only needs QEMU TCG, ordinary file access and a serial console; the
commands below use no KVM, host socket listener or guest network interface.

## Guest inputs

Use the same Alpine release and architecture for the root filesystem, kernel,
kernel modules and installed packages. The development run uses Alpine 3.24.4,
the `virt` Linux 6.18.60 kernel and QEMU 11.0.3. The release manifest at
`https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/ARCH/latest-releases.yaml`
provides the minirootfs checksum. Kernel, initramfs and module images are under
the release directory's `netboot/` subdirectory.

Install the application's native APK and its dependencies into a writable guest
root directory. For an uninstalled development build, the runtime packages are
`gtk4.0`, `libadwaita`, `gtksourceview5`, `vte3-gtk4`,
`gsettings-desktop-schemas` and their dependencies.
The GTK 4 VTE library is in `vte3-gtk4`; the `vte3` package contains GTK 3 VTE.
The GUI checks additionally use `xvfb`, `xvfb-run`, `xauth`, `dbus`,
`font-dejavu` and `font-noto-cjk`. Wayland checks use `weston`,
`weston-backend-x11`, `weston-shell-desktop` and `xdotool`. These are guest test
dependencies; the application's APK does not depend on them.

Extract the matching `modloop-virt` SquashFS image and place its `modules/`
directory at `/lib/modules` in the guest. The ordinary UI runner uses an isolated
local HTTP fixture. The separately selected terminal check starts a disposable
Docker daemon as described below.

Keep generated disks, downloaded packages, kernels and screenshots outside the
Git checkout. The checked-in scripts contain the reproducible test logic.

## Prepare the test filesystem

Build the UI test executable from the current source revision and Cargo lockfile:

```sh
cargo test --locked --bin pods --no-run --target-dir _build/src
```

Place the actual executable artifacts in the guest as follows. Cargo's `.d`
dependency files are not executables.

| Guest path | Artifact |
| --- | --- |
| `/opt/pods-tests/pods` | `pods-*` binary containing the ignored GTK integration test |
| `/opt/pods-tests/guest-ui-tests.sh` | `tests/qemu/guest-ui-tests.sh` |
| `/opt/pods-tests/guest-wayland-input.sh` | `tests/qemu/guest-wayland-input.sh` |
| `/vm-init` | `tests/qemu/guest-init.sh` |

Give the binaries and scripts mode `0755`. The compiled configuration in the
Pods test executable names the installed resource directory: install the
resources, schema and translations at that exact prefix inside the guest. Local
development used `/work/install`; native APK builds use `/usr`. For the latter,
create `/etc/pods-test-env` containing `PODS_TEST_PREFIX=/usr`.

For Chinese checks, install the Pods `zh_CN` translation alias supplied by the
APK. The test sets `LANG=zh_CN.UTF-8`, unsets `LANGUAGE` and verifies the result of
`gettext("Regular Expression")` in the GTK process before creating widgets.

The guest entry point creates the ordinary `tester` account with UID 1000 and
invokes the mobile UI checks under that account. Results are written under
`/out`. The default run includes English and Chinese X11 checks and an English
Wayland check. During development, `PODS_TEST_UI_LOCALES=C.UTF-8` and
`PODS_TEST_WAYLAND=0` in `/etc/pods-test-env` select a single X11 run. Setting
`PODS_TEST_UI_LOCALES=""` selects the Wayland check alone.

The runner streams each process's log to the serial console and retains it in
the output directory. Each process has a 600-second timeout; a development guest
may set `PODS_UI_TEST_TIMEOUT` explicitly. A timeout is a failed run with retained
evidence. `PODS_UI_TEST_OUTPUT_ROOT` selects a separate output directory for an
iteration. `PODS_UI_TEST_SECTION=forms`, `auxiliary`, `empty` or `pages` selects
one section when diagnosing a concrete failure; final combined runs leave it unset.

For a separate native color-chooser check, set `PODS_UI_TEST_COLOR_ONLY=desktop`
to inspect the chooser's actual toplevel on the ordinary output. For the phone
check, set `PODS_UI_TEST_COLOR_ONLY=phone`, `PODS_TEST_UI_LOCALES=""`,
`PODS_TEST_WAYLAND=1`, `PODS_TEST_WAYLAND_WIDTH=366` and
`PODS_TEST_WAYLAND_HEIGHT=651`. This measures a real 366×651 compositor output;
merely shrinking the parent application would leave the independent chooser
unconstrained. For the reduced-height output, select
`PODS_UI_TEST_COLOR_ONLY=phone-short` and `PODS_TEST_WAYLAND_HEIGHT=300`.
The test reaches the palette's rightmost column using its existing scroller,
opens Custom, focuses the color-value entry and clicks the real Cancel button.
It checks that cancellation preserves the selected color. The main combined
test separately checks the form's existing `win.close` Cancel action in the
actual application window; ordinary layout fixtures do not install that action.

## Connected terminal check

The optional live test uses the existing VTE widget, Docker exec transport and
input actions with one temporary container in the guest. Install `docker`,
`docker-cli`, `containerd`, `runc` and the matching kernel modules in the
disposable filesystem. Copy `tests/qemu/guest-terminal-tests.sh` to
`/opt/pods-tests/guest-terminal-tests.sh` with mode `0755`, and copy the verified
Alpine minirootfs tarball for that guest architecture into the guest. Set:

```sh
PODS_TEST_LIVE_TERMINAL=1
PODS_TEST_BASE_ROOTFS=/opt/pods-tests/alpine-minirootfs.tar.gz
```

The guest entry point invokes the opt-in helper as root to start its dedicated
daemon at `/run/pods-ui-terminal.sock`, using a new private data directory,
the VFS storage driver and no bridge or iptables rules. It imports the local
root filesystem without network access and creates one labelled container with
`--network none`. Only this test socket is given the guest test user's group and
mode `0660`. The UI still runs as ordinary UID 1000. The helper removes its
container and image and stops its daemon when the test exits.

The exact ignored test is
`ui_tests::terminal::live_terminal_keeps_input_and_resize`. It checks ordinary
VTE commit input, clipboard paste through the touch button, the five auxiliary
key byte sequences as received by the shell, Ctrl+C interrupting a command,
selection/copy, and actual container `stty size` after each viewport change.
X11 and Wayland runs retain terminal output, screenshots, Docker version and
socket permissions. This check sends no requests to a host daemon and must not
be directed at a user's existing containers. It exercises the current terminal
implementation; actual IME preedit, touch delivery and Plasma Keyboard behavior
remain device checks.

## Assemble and boot

Extract Alpine's matching `initramfs-virt` to a staging directory, preserving
symlinks. Add the complete matching module directory under `usr/lib/modules`,
then replace its `/init` with `tests/qemu/initramfs-init.sh`. The helper creates
the initial device nodes in the archive without requiring host `mknod` access:

```sh
chmod +x initramfs/init
python3 build-aux/qemu-initramfs.py initramfs boot-initramfs.cpio.gz
mke2fs -F -b 4096 -t ext4 -d guest-root guest.ext4 1048576
build-aux/run-qemu-tests.sh x86_64 vmlinuz-virt boot-initramfs.cpio.gz guest.ext4
```

The explicit block size and block count create a 4 GiB sparse filesystem. The
launcher uses 2 CPUs and 2 GiB RAM; `PODS_QEMU_MEMORY=1024` selects 1 GiB RAM for
smaller workloads. For ARM64, use the ARM64 input files and replace `x86_64` with
`aarch64`. The launcher selects the `virt` machine and Cortex-A72 CPU for ARM64.

The guest remounts the filesystem read-only before powering off. After QEMU
exits, extract the results without mounting the disk on the host:

```sh
mkdir -p results
debugfs -R 'rdump /out results' guest.ext4
```

Never edit an image while QEMU has it open. If a run ended before the read-only
remount, run `e2fsck -f` before using `debugfs -w` to update files. Journal replay
after offline edits can otherwise overwrite those edits.

## Checks and scope

The GTK test activates the actual application, constructs its main window and
checks the welcome page at 366 × 651, 651 × 366, 366 × 300, 360 × 651 and
1024 × 768 content viewports. It reads the installed image sort default and
asserts `id` before activation. This is the two-line schema correction explicitly
approved by the user; the test neither overwrites settings nor substitutes a
test schema. Each process uses a fresh temporary application configuration and
`GSETTINGS_BACKEND=memory`.

The test constructs the existing `ClientView` with real GObject models populated
through the existing refresh and inspection entry points. A local Unix HTTP
fixture supplies empty and populated containers, images and volumes, details,
logs and process rows. It records requests and permits only the read-only
operations used by these UI checks. This tests actual view state, allocation,
focus, scrolling and navigation; daemon operation results are checked separately.

The sidebar and existing navigation actions select each resource and return at
narrow widths. Checks cover empty states, ordinary and selected lists, grid/list
views, search results and no-results states, menus, long labels, details and
cancelled confirmation dialogs. Split-view children and header buttons must fit
on both sides of the revised 720sp threshold, as well as at the former 668sp
boundary. This catches the child clipping that the earlier loading-only check
did not assert. Long image and volume labels must retain usable text width while
short labels keep their compact inline layout.

Empty container, image and volume states also scroll their existing StatusPage
containers to the end in all five viewports. The real buttons must be mapped,
sensitive, tall enough for their measured minimum, and fully inside the clipped
viewport and window. The image action bin must contain its child and both
buttons. This catches the original 1px bin minimum even when the buttons
themselves retain a 44px allocation. Welcome, filtered-list Show All and stopped
terminal Start buttons use the same scrolling and clipped-boundary checks in
their existing viewport loops; those added checks do not activate their actions.

Form checks use the real connection, container, image, volume, copy, commit,
prune and action dialogs. They exercise text entry, focus, scroll-to-end and
menus at phone, reduced-height and desktop sizes. Global search, saved-connection
filtering, engine information, action history, Inspect, logs and process views
are checked in their populated states. Failed independent sections retain their
panic output while the remaining sections run; any failed section fails the
combined test.

The existing global-search transition can leave its initial placeholder after
an unmatched query removes all rows. This unrelated state issue is retained in
[Upstream issues](upstream-issues.md). Its actual state is captured separately;
the no-results page has an explicitly labelled template-only layout check. That
check does not count the existing query-to-placeholder transition as passed.

The changed widgets are also instantiated from their actual templates and
models. Their checks use 360 and 366 pixel portrait widths, 651 pixel landscape
width, a reduced 300 pixel height for keyboard occlusion, and a 1024 pixel desktop
width. Window requests are calibrated against allocated content size, accounting
for window decorations, before exact viewport and control-boundary assertions.

The checks cover compact search options and translated match counts, terminal
toolbar breakpoints, 44-pixel pointer targets, the five fixed auxiliary-key byte
sequences, suggestion popup height and selection, and the connection dialog.
Both socket-help popovers are measured at portrait, landscape and reduced-height
viewports. Their text is checked for a usable width and scrolled to the end;
screenshots preserve the beginning and end of each popover. The separate live
terminal check below exercises the existing paste function with a connected
container. Actual software-keyboard protocol and preedit remain device checks.

The empty saved-view default remains recorded in [Upstream issues](upstream-issues.md).
The 668sp split-view clipping is directly relevant to mobile layout and is
addressed by the local threshold change; historical source C results below
retain their original narrower coverage.

Xvfb supplies the X11 run. For Wayland, Weston runs its X11 backend inside a
separate Xvfb display, with the Pixman software renderer. The application uses
`GDK_BACKEND=wayland`. Before opening a popup, the test calls the guest input
helper, which focuses the nested compositor and sends a Shift key through
XTest. Weston delivers that event to its virtual Wayland input seat.

An earlier headless Weston diagnostic reported `GdkWaylandDisplay` with
`input seat: None`; the search options popover remained visible but unmapped at
0 by 0 pixels. Wayland associates popup requests with input-event serials;
see the [Wayland input model](https://wayland.freedesktop.org/docs/book/Protocol.html#input).
[Gdk.Popup.present](https://docs.gtk.org/gdk4/method.Popup.present.html) also
documents that a failed presentation produces no layout event. The nested
compositor supplies input without changing the application's popover properties.

These runs exercise native GTK and Wayland code paths on the guest architecture.
Pixel 2 testing still needs Plasma Mobile, the device's graphics driver, display
scaling, touchscreen and actual software keyboard.

## Evidence boundaries

ARM64 full-system TCG booted the official Alpine kernel and root filesystem. A
guest `syslogd` bound `/dev/log`, and `logger` successfully sent a Unix socket
message. Application results are recorded separately for the architecture, GTK
backend, locale and exact source revision actually tested. The x86_64 result
below includes real application activation, welcome-page rendering and existing
navigation. ARM64 application execution uses a separate guest with matching
native APKs and test artifacts. Archived runs from broader earlier candidates
are retained as experiments and are not gates for this mobile change.

## Source S final ARM64 APK run

Source `27802b3ef04e69b582355a62eba1461dc8cb90bb` was tested with the exact
`3.3.0_git20261124-r2` APKs and matching harnesses from native CI run
`45385035150`, artifact `14916304845`. The new ARM64 guest used Alpine 3.24.4,
Linux 6.18.60-0-virt, 4096-byte pages, two virtual CPUs, 1536 MiB RAM, TCG and
no network interface. The package signature, installed version, executable
hash and `images.sort-attribute='id'` schema value were checked before tests.

| Check, as guest UID 1000 | Result |
| --- | --- |
| Contract suite, including actual permission denial | 12 passed, 0.50 seconds |
| Local Docker 29.9.3 read-only connection | 1 passed, 0.32 seconds; empty container, image and volume lists |
| Complete GTK fixture group, native Wayland client | 1 passed, 840.89 seconds; 448 PNGs and 697 read-only fixture requests |
| Live Docker terminal, X11 | 1 passed, 17.50 seconds; 7 PNGs |
| Live Docker terminal, Wayland | 1 passed, 17.32 seconds; 7 PNGs |

The full UI process had a preset 900-second budget. Its two-paint requirement,
five-second individual paint deadline and geometry assertions were unchanged.
This run included the final empty-image action height and all additional
Welcome, filtered-list Show All and stopped-terminal Start scroll checks.
The real terminal used the separate offline daemon/container described above;
the five viewport sizes matched container `stty size` in both backends.

All guest stages and QEMU exited with status zero. The guest remounted its root
filesystem read-only and powered off before extraction. The 513 exported
output files passed checksum verification, and all 462 PNGs decoded. Extraction
warnings about preserving guest ownership on the host were retained; file
content checks passed. Evidence is in the delivery ZIP under
`evidence/arm64-qemu/`; exact package hashes and native EN/zh_CN results are in
the [verification report](verification.md).

This exact-APK result is separate from the earlier development debug run,
which passed the complete EN/X11 group in 730.98 seconds with 448 PNGs and an
explicit 1200-second process budget. Pixel 2 scaling, GPU, touch, software
keyboard, Chinese preedit, Plasma Portal and power measurements remain device
checks.

## Source C x86_64 run

Source revision `20ed3a2f6ced56710d8c3588c238b61e66d90f99` was tested on
2026-11-24 with the installed schema default `images.sort-attribute='id'`.
The executable was built with
`cargo test --offline --locked --bin pods --no-run --target-dir _build/src`.
Its SHA-256 is:

```text
7bf8bc18fc560e02fb7822232b59628a52ac52239db098ed0af74d145727d492
```

The test runs as guest UID 1000 with `G_DEBUG=fatal-criticals`. It uses GTK
4.22.4, libadwaita 1.9.2 and VTE 0.84.0 on Alpine 3.24.4. Source C results were
exported to `build-env/qemu-x86/startup-verification/` outside the Git checkout.
That directory contains `boot.log`, the three test logs and screenshot
directories, and `SHA256SUMS` for the executable, UI resources, CSS resources and
compiled schema. Source A exports remain in the separate `scoped-verification/`
directory.

| GTK backend | Locale | Result | Test duration | Guest evidence |
| --- | --- | --- | --- | --- |
| X11 / Xvfb | `C.UTF-8` | Passed; 38 screenshots | 38.64 s | `/out/mobile-ui-C.UTF-8.log` and matching directory |
| X11 / Xvfb | `zh_CN.UTF-8`, `LANGUAGE` unset | Passed; 38 screenshots | 35.20 s | `/out/mobile-ui-zh_CN.UTF-8.log` and matching directory |
| Wayland / nested Weston with virtual keyboard and pointer | `C.UTF-8` | Passed; 38 screenshots | 35.01 s | `/out/mobile-ui-wayland.log` and matching directory |

All three runs used the same executable and resource files. The guest reported
`QEMU_UI_TEST_EXIT=0`, remounted its filesystem read-only and powered off without
filesystem I/O errors. The exported 114 PNG files were decoded successfully.
Each group contains five actual main-window startup images, six navigation
images and the existing 27 affected-widget images. Representative portrait,
reduced-height, desktop and popup images were inspected. The Chinese process
passes its gettext assertion; untranslated upstream welcome strings remain in
English in that locale.

The installed compiled schema has SHA-256
`0c771b769e335594a51cd4abb45bfecfc01ef5f8d5db351e53a1fb80bd41d6f9`.
Both log markers, `Application activation and main-window startup passed with
the installed schema` and `Existing sidebar activation and navigation back
passed at 668px breakpoint`, appear in every group. The 668-pixel boundary also
produces the existing minimum-width warning recorded in the issue list; the
test does not assert that every child inside that boundary layout fits.

The terminal toolbar's minimum width is 338 pixels after the padding adjustment
limited to its new buttons. All six targets remain at least 44 by 44 pixels at
360 and 366 pixel portrait widths and 651 pixel landscape width. The toolbar
is hidden at 1024 pixels. The suggestion list requests a minimum of 58 and a
natural height of 200 pixels; its parent stack now uses the visible page's height
instead of the hidden status page's larger height.

The help popup checks use `compute_bounds` on the actual GTK content frame,
including padding and borders. Native Wayland popup surfaces reserve additional
space for transparent shadows. The measured frame dimensions were:

| Backend and locale | Podman help frame | Docker help frame | Scrollable height |
| --- | --- | --- | --- |
| X11, English | 320 × 236 | 288 × 236 | 220 |
| X11, Chinese | 239 × 236 | 287 × 236 | 220 |
| Wayland, English | 322 × 238 | 290 × 238 | 220 |

These frames fit the tested 366 × 651, 366 × 300 and 651 × 366 viewports. The
Wayland native surfaces measure 368 × 296 and 336 × 296 respectively; their
transparent margins are retained in the screenshots and excluded from the
content-clipping assertion. Each help text was scrolled to its end and captured.

This record verifies application startup, existing navigation and the affected
widgets under x86_64 emulation. The matching ARM64 APK run is recorded below.
Pixel 2 device acceptance remains as described above.

## Source C ARM64 run

The native aarch64 APKs and all three harnesses came from source C's
[successful native build](https://github.com/Mr-Metatron/pods/actions/runs/45345743799),
artifact `14899187208`. The full-system guest installed the signed
`3.3.0_git20261124-r1` packages with their matching public key, ran the schema and
icon triggers, and read the installed image sort default as `id`. Its Linux
6.18.60-0-virt kernel reported a 4096-byte page size. Tests ran as UID 1000.

| Guest check | Result | Duration |
| --- | --- | --- |
| Socket and transfer contract, including actual permission denial | 12 passed | 0.47 s |
| Isolated Docker 29.9.3 read-only smoke | 1 passed; no containers, images or volumes | 0.29 s |
| X11 English startup, navigation and widgets | 1 passed; 38 screenshots | 37.70 s |
| X11 Chinese startup, navigation and widgets | 1 passed; 38 screenshots | 34.16 s |
| Native Wayland startup, navigation and widgets | 1 passed; 38 screenshots | 33.14 s |

The matching ARM64 UI harness has SHA-256
`b4f8a9ed5fd7b00a2b6b8259e374ad704365b13817a480864da6d0836aaefd36`.
The complete guest returned `QEMU_ARM64_SOURCE_C_GUEST_EXIT=0`, remounted its
filesystem read-only and powered off. Evidence is under
`build-env/qemu-arm64/candidate-C-evidence/`. The first attempt had already
installed the APKs but its scratch version assertion misread `apk info -v`
descriptions as a version. That attempt remains separately archived; correcting
the fixture and rerunning a clean disk required no product, APK or repository
test changes.

A separate X11 capture started the installed production `/usr/bin/pods` with an
isolated configuration and a valid saved connection to the same Docker daemon.
It only set the existing last-used connection UUID; the installed image sort
default remained `id` before and after the run. Desktop and portrait screenshots
were inspected: both show the loaded `No Containers Available` state, with the
desktop also showing `QEMU local Docker` and Docker `v29.9.3`. The existing quit
action returned exit zero, and all three Docker inventories remained empty.

The production window measured 1024 × 768 on the desktop. A request for
366 × 651 produced an actual X11 outer window of 370 × 651; captions retain that
measured size. Exact 360/366 content viewports are covered separately by the
GTK harness. This production capture uses X11; the Wayland result above uses
the native Wayland harness. It does not exercise a physical keyboard, touch,
GPU or any Docker write operation.

The installed production binary has SHA-256
`44105efa4230c92b2be42a19600766252314f81d7f9d76e5a2f696cbd0560af7`, matching the
binary extracted from the signed APK. Production PNGs, geometry, logs and
before/after inventories are under
`candidate-C-evidence/out/production-C/run.PdnhBL/`.

## Earlier source A record

Source A, `f427564e41347ae8bbe4a3a7f8e01eccadc18aa0`, was tested with the original
schema default `images.sort-attribute='name'`. At that stage the test only
instantiated affected widgets and did not activate the complete application.
Its executable SHA-256 was
`0b9315882c3c747f5a2d2ac50c58600aca225637f5840b8ef1e37fdb42e80ec5`.
The X11 English, X11 Chinese and Wayland English runs passed with 27 screenshots
each, in 23.36, 21.43 and 21.43 seconds respectively. Those 81 images and their
logs remain under `build-env/qemu-x86/scoped-verification/`; they are kept
separate from source C's startup and navigation results.
