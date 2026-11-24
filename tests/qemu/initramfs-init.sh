#!/bin/sh
# Minimal initramfs entry point; the directory must include BusyBox and matching
# kernel modules, including virtio block and ext4 with their dependencies.
set -eu
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
/bin/busybox --install -s
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev
modprobe virtio_pci || true
modprobe virtio_blk
modprobe ext4
mkdir -p /sysroot
for attempt in 1 2 3 4 5; do
    [ -b /dev/vda ] && break
    sleep 1
done
mount -t ext4 /dev/vda /sysroot
mount --move /dev /sysroot/dev
mount --move /proc /sysroot/proc
mount --move /sys /sysroot/sys
exec switch_root /sysroot /vm-init
