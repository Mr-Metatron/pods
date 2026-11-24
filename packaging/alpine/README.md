# Native Alpine and postmarketOS packages

This package uses the system GTK, libadwaita, GtkSourceView and VTE libraries.
The installed application keeps its upstream ID `com.github.marhkb.Pods` and
appears as **Pods** in the application launcher. Container engines continue to
run independently of the GUI.

## Build requirements

Use Alpine 3.24 or a compatible postmarketOS environment with Rust/Cargo 1.92
or newer, GTK 4.22, GLib/GIO 2.88 and libadwaita 1.9. The template lists the full
build dependencies. The package targets `aarch64` and `x86_64`; the CI workflow
uses native runners for both, with ordinary Alpine containers and no QEMU.

Build on an Alpine machine of the desired architecture, or in an Alpine VM.
Compiling the GTK Rust bindings can use substantial memory. The package defaults
to two Cargo jobs, thin LTO and 16 codegen units; a 4 GB phone can use
`PODS_BUILD_JOBS=1 abuild -r`, but a
separate build machine avoids competing with its desktop and running containers.
`PODS_RELEASE_LTO` and `PODS_RELEASE_CODEGEN_UNITS` override the package defaults.
These variables are applied after abuild loads its own Cargo settings. They
control compilation; managed containers retain their own resource limits.

## Candidate package status

This is a reviewable packaging candidate. The checkout currently contains
`APKBUILD.in` and the source snapshot helper described below. The helper creates
a complete `APKBUILD` and its matching source archive from a committed revision,
with the actual SHA-512 checksum.

The two-line `images.sort-attribute` schema correction is the explicitly
approved startup-blocker exception, tracked as M-STARTUP. It aligns the schema
with the existing `id` enum value. See [the upstream issue record](../../docs/mobile/upstream-issues.md).
The [verification report](../../docs/mobile/verification.md) identifies the tested
source revision, package results and remaining device checks.

## Create a local source snapshot

Commit the changes to package, then run the helper from the checkout:

```sh
python3 build-aux/prepare-alpine.py --ref HEAD --output /tmp/pods-aport
```

The helper exports exactly that Git commit, reads its APKBUILD template, and
writes a compressed source archive and a ready-to-run `APKBUILD` with its real
SHA-512 checksum. Uncommitted and untracked files are outside the snapshot.
Reusing the same commit produces the same source bytes. The generated file
records the full commit; it has no `SKIP` checksum or mutable branch download.
Keep the APKBUILD and matching archive together when copying them to a builder.

For a distribution submission, publish an immutable source archive and replace
the generated local `source=` entry with its download URL while retaining the
verified checksum. Add the responsible package maintainer before submitting to
aports or pmaports.

## Build with abuild

Set up the standard Alpine packaging tools once:

```sh
doas apk add alpine-sdk
doas addgroup "$USER" abuild
```

Log out and back in so the new group is active, then create a local signing key
if this build account does not have one:

```sh
abuild-keygen -a -i
cd /tmp/pods-aport
abuild -r
```

`prepare()` downloads the versions fixed by Cargo.lock into the Cargo home
selected by upstream's Meson build. `build()` uses locked, offline Cargo;
`check()` runs the existing desktop-file, metainfo and schema validators.
Packaging uses Meson to install the binary, resources, schemas, desktop file,
icons and metainfo. Translations are in the `pods-lang` subpackage.
It includes a `zh_CN` alias for the upstream `zh_Hans` catalog, so the usual
Plasma Simplified Chinese locale works without setting `LANGUAGE` globally.

## Install the candidate

Copy the signed `pods` and `pods-lang` packages and the builder's **public** key
to the phone. Install that public key in `/etc/apk/keys/`, then install the two
packages with `apk add /path/to/pods-*.apk`. Keep the signing private key on the
builder. If the package was built on the device and `abuild-keygen -i` installed
the public key there, that trust step is already complete.

CI artifacts include the packages, the build's public key in `keys/`, the
generated APKBUILD, installed dependency versions and ELF inspection results.
Its private key stays in the disposable builder.

Both native CI jobs install the APKs, then compile and run the mobile test
harnesses as an ordinary user. The checks cover local engine identification,
socket errors and permissions, the statistics concurrency limit, and the
modified mobile controls. Installed GTK resources are exercised under Xvfb
with an in-memory GSettings backend and the Cairo renderer. English and
Simplified Chinese runs save screenshots for phone, landscape, reduced-height
and desktop viewports in `screenshots/`; full test output is in `logs/`.
Failed jobs retain their available evidence.
The `tests/` directory contains the matching `pods-ui`, `engine_contract` and
`engine_live` harnesses with a relative-path `SHA256SUMS` file. They can be run
on an Alpine VM of the same architecture after installing the matching APKs,
without a Rust toolchain; the GTK harness additionally needs a display and the
environment described in `src/ui_tests.rs`.

The x86_64 job also runs `docker_local_connection_smoke` in a separate Alpine
container. It selects the runner daemon's Unix socket explicitly and checks
engine identification, ping, information and resource listing. It uses an
ordinary user whose supplemental socket group exists only inside the test
container. Docker version and test output are saved separately. These checks
complement the Pixel 2 Wayland and touch acceptance below.

After approval and resolution of the startup blocker above, open **Pods** from
the application drawer as the ordinary desktop user. The package includes the
icon theme and desktop schemas used by the existing UI.
It does not enable or install a container daemon, alter socket permissions, or
change user groups. The `openssh-client-default` dependency preserves SSH remote
connections; TLS uses the system CA certificates.

Docker's process listing invokes `ps` on the daemon host. The package includes
`procps-ng` so a local Alpine Docker daemon supports the process columns used by
Pods, including CPU percentage. For a remote Docker connection, install procps
on the daemon host as well; the client's copy does not provide the remote `ps`.

The desktop session should provide a Secret Service for saved registry
credentials and a file chooser portal. On Plasma, use the session's existing
KDE portal and credential service. Local Docker socket connections do not need
registry credentials stored in a keyring.

## Pixel 2 verification

The supplied Wave15 r73 / Wave16 baseline has the needed GTK, GLib and
libadwaita versions. Package architecture must be `aarch64`; verify the installed
ELF uses `/lib/ld-musl-aarch64.so.1`. Installation into Alpine on x86_64 does not
test the phone's graphics, touchscreen or keyboard.

Test launch from the application drawer, Chinese translation (`pods-lang`),
366×651 portrait and 651×366 landscape, keyboard-reduced height, Chinese preedit,
terminal rotation, file dialogs, Docker permission errors, reconnect after
suspend, and long-running log/metrics views. Record memory and CPU with the
number of containers and log rate so repeated runs are comparable.

This device's provided baseline uses Freedreno for GLES and Lavapipe for Vulkan.
Compare the default GTK renderer with this per-launch diagnostic setting:

```sh
GDK_BACKEND=wayland GSK_RENDERER=opengl pods
```

Check that the active renderer uses the Adreno GPU. Keep a device-specific
override only if the comparison establishes that it is needed; the regular
desktop entry retains GTK's normal renderer selection.

See [the design and acceptance matrix](../../docs/mobile/design.md) for the
full functional and device testing scope. Record successful commands and
remaining device checks separately in the verification report.
