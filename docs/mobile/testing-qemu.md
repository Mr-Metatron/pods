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
`gtk4.0`, `libadwaita`, `gtksourceview5`, `vte3-gtk4` and their dependencies.
The GTK 4 VTE library is in `vte3-gtk4`; the `vte3` package contains GTK 3 VTE.
The GUI checks additionally use `xvfb`, `xvfb-run`, `xauth`, `dbus`,
`font-dejavu` and `font-noto-cjk`. Wayland checks use `weston`,
`weston-backend-x11`, `weston-shell-desktop` and `xdotool`. These are guest test
dependencies; the application's APK does not depend on them.

Extract the matching `modloop-virt` SquashFS image and place its `modules/`
directory at `/lib/modules` in the guest. The UI runner does not start a Docker
daemon or run general container lifecycle tests.

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

The GTK test directly instantiates the changed widgets' actual templates and models. It
measures 360 and 366 pixel portrait widths, 651 pixel landscape width, a reduced
300 pixel height for keyboard occlusion, and a 1024 pixel desktop width. It
calibrates the window request against the allocated content size, accounting for
window decorations, then keeps exact viewport and control-boundary assertions.

The checks cover compact search options and translated match counts, terminal
toolbar breakpoints, 44-pixel pointer targets, the five fixed auxiliary-key byte
sequences, suggestion popup height and selection, and the connection dialog.
Both socket-help popovers are measured at portrait, landscape and reduced-height
viewports. Their text is checked for a usable width and scrolled to the end;
screenshots preserve the beginning and end of each popover. Paste continues to call the
existing terminal paste function; its connected-terminal and software-keyboard
behavior remains a device acceptance item.

The test registers an application context for GTK widgets but does not activate
the complete application or construct its unchanged main window and client
panels. It uses the unchanged application schema and does not replace the image
sort setting. Full-application startup validation remains blocked pending the
user's explicit approval for the separate schema fix recorded in
[Upstream issues](upstream-issues.md).

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
message. Mobile widget results must be recorded separately for the architecture,
GTK backend, locale and exact source revision actually tested. Archived runs
from broader earlier candidates are retained as experiments and are not gates
for this scoped mobile change.

## Scoped x86_64 run

The scoped source revision `f427564e41347ae8bbe4a3a7f8e01eccadc18aa0` was
tested with the unchanged upstream schema default
`images.sort-attribute='name'`. The executable was built with
`cargo test --offline --locked --bin pods --no-run --target-dir _build/src`.
Its SHA-256 is:

```text
0b9315882c3c747f5a2d2ac50c58600aca225637f5840b8ef1e37fdb42e80ec5
```

The test runs as guest UID 1000 with `G_DEBUG=fatal-criticals`. It uses GTK
4.22.4, libadwaita 1.9.2 and VTE 0.84.0 on Alpine 3.24.4. Screenshot directories
for this scoped run start with `/out/mobile-ui-`; earlier `/out/ui-` directories
belong to archived experiments.

| GTK backend | Locale | Result | Guest evidence |
| --- | --- | --- | --- |
| X11 / Xvfb | `C.UTF-8` | Passed; 27 screenshots | `/out/mobile-ui-C.UTF-8.log` and matching directory |
| X11 / Xvfb | `zh_CN.UTF-8`, `LANGUAGE` unset | Passed; 27 screenshots | `/out/mobile-ui-zh_CN.UTF-8.log` and matching directory |
| Wayland / nested Weston with virtual keyboard and pointer | `C.UTF-8` | Passed; 27 screenshots | `/out/mobile-ui-wayland.log` and matching directory |

All three runs used the same executable and resource files. The guest reported
`QEMU_UI_TEST_EXIT=0`, remounted its filesystem read-only and powered off without
filesystem I/O errors. The exported 81 PNG files were decoded successfully and
representative portrait, reduced-height, desktop and popup images were inspected.

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

This record verifies the affected widgets under x86_64 emulation. ARM64 widget
execution using the APK and harness from the same source revision is a separate
pending result. Full application startup and Pixel 2 device acceptance remain
as described above.
