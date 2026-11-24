# Full-system QEMU validation

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
`weston-backend-headless` and `weston-shell-desktop`.

Docker acceptance uses `docker`, `iproute2`, `iptables`, `procps-ng` and
`python3`. `procps-ng` supplies the daemon host's `ps` implementation used by
Docker Top. The guest kernel must provide cgroups, namespaces, veth, bridge and
the netfilter modules used by the daemon. Extract the matching `modloop-virt`
SquashFS image and place its `modules/` directory at `/lib/modules` in the guest.

Keep generated disks, downloaded packages, kernels and screenshots outside the
Git checkout. The checked-in scripts contain the reproducible test logic.

## Prepare the test filesystem

Build all three test executables with the same source revision and Cargo lockfile:

```sh
cargo test --locked --no-run --target-dir _build/src
```

Place the actual executable artifacts in the guest as follows. Cargo's `.d`
dependency files are not executables.

| Guest path | Artifact |
| --- | --- |
| `/opt/pods-tests/pods` | `pods-*` binary containing the ignored GTK integration test |
| `/opt/pods-tests/engine-contract` | `engine_contract-*` integration test binary |
| `/opt/pods-tests/engine-live` | `engine_live-*` integration test binary |
| `/opt/pods-tests/guest-ui-tests.sh` | `tests/qemu/guest-ui-tests.sh` |
| `/vm-init` | `tests/qemu/guest-init.sh` |

Give the binaries and scripts mode `0755`. The compiled configuration in the
Pods test executable names the installed resource directory: install the
resources, schema and translations at that exact prefix inside the guest. Local
development used `/work/install`; native APK builds use `/usr`. For the latter,
create `/etc/pods-test-env` containing `PODS_TEST_PREFIX=/usr`.

For Chinese checks, install the Pods `zh_CN` translation alias supplied by the
APK. The test sets `LANG=zh_CN.UTF-8`, unsets `LANGUAGE` and verifies the result of
`gettext("Regular Expression")` in the GTK process before creating widgets.

The root guest entry point mounts cgroup2, creates the ordinary `tester` account
with UID 1000, and invokes the contract, permission and GUI checks under that
account. The permission check must run without root privileges. Results are
written under `/out`.

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

The GTK test instantiates the application's actual templates and models. It
measures 360 and 366 pixel portrait widths, 651 pixel landscape width, a reduced
300 pixel height for keyboard occlusion, and a 1024 pixel desktop width. It
calibrates the window request against the allocated content size, accounting for
window decorations, then keeps exact viewport and control-boundary assertions.

The checks cover navigation breakpoints, search options and translated match
counts, terminal action bytes and clipboard input, suggestion popup height and
selection, and creation dialogs. The real log page is also filled past its line
and byte limits to check GTK buffer deletion, timestamp alignment and disabled
undo history. Screenshots are saved for visual inspection.

Xvfb supplies the X11 run. Weston uses its headless backend and software renderer
for the Wayland run. These runs exercise native GTK and Wayland code paths on
the guest architecture. Pixel 2 testing still needs Plasma Mobile, the device's
graphics driver, display scaling, touchscreen and actual software keyboard.

## Findings from the development run

ARM64 full-system TCG booted the official Alpine kernel and root filesystem. A
guest `syslogd` bound `/dev/log`, and `logger` successfully sent a Unix socket
message. On the x86_64 guest, the Unix API contract and an ordinary-user
`EACCES` check ran successfully after the contract fixture was corrected.

The first GTK launch exposed a pre-existing schema mismatch: the images panel
enumeration used `id`, while the schema offered and defaulted to `name`. The
schema now matches the enumeration. A keyfile-backend check explicitly saved
`name` using the old schema, then read `id` using the corrected schema, confirming
the existing GSettings fallback behavior without extra migration code.

The Docker run performed a real pull from a loopback registry, then created and
started a container and checked inspection, published ports, bind mounts,
volumes and logs. The first Top request identified the missing daemon-host
`procps-ng` dependency. The fixture retains this real operation for the rerun.
