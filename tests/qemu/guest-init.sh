#!/bin/sh
set -u
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
if [ -f /etc/pods-test-env ]; then
    set -a
    . /etc/pods-test-env
    set +a
fi
mount -t tmpfs tmpfs /run
mount -t tmpfs tmpfs /tmp
mkdir -p /dev/pts /out
mount -t devpts devpts /dev/pts
echo QEMU_NATIVE_TEST_GUEST_BOOTED
uname -a
if [ ! -e /var/lib/pods-test-guest-ready ]; then
    chown -R 0:0 /etc /usr /bin /sbin /lib /var /work /opt /vm-init
    addgroup -g 1000 tester
    adduser -D -u 1000 -G tester tester
    fc-cache -f > /out/font-cache.log 2>&1
    touch /var/lib/pods-test-guest-ready
fi
mkdir -p /run/user/1000
chown -R tester:tester /out /run/user/1000
chmod 700 /run/user/1000
su tester -c /opt/pods-tests/guest-ui-tests.sh
status=$?
echo "QEMU_UI_TEST_EXIT=$status"
echo QEMU_NATIVE_TEST_GUEST_FINISHED
# Reap the test user's display and accessibility helpers before powering off.
su tester -c 'kill -TERM -1' 2>/dev/null || true
sleep 1
sync
mount -o remount,ro /
poweroff -f
