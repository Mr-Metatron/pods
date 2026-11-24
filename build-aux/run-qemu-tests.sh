#!/bin/sh
# Boot an already prepared disposable image. No host networking, KVM or sockets.
set -eu

if [ "$#" -ne 4 ]; then
    echo "Usage: $0 x86_64|aarch64 KERNEL INITRAMFS DISK_IMAGE" >&2
    exit 2
fi
arch=$1
kernel=$2
initramfs=$3
disk=$4

case "$arch" in
    x86_64)
        machine=q35
        cpu=max
        console=ttyS0
        ;;
    aarch64)
        machine=virt
        cpu=cortex-a72
        console=ttyAMA0
        ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 2 ;;
esac

exec "qemu-system-$arch" \
    -machine "$machine" -cpu "$cpu" -accel tcg \
    -m "${PODS_QEMU_MEMORY:-2048}" -smp 2 \
    -nodefaults -display none -monitor none -serial stdio -nic none -no-reboot \
    -kernel "$kernel" -initrd "$initramfs" \
    -drive "file=$disk,format=raw,if=virtio" \
    -append "console=$console rdinit=/init panic=-1"
