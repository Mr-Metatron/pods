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
mkdir -p /dev/pts /sys/fs/cgroup /out
mount -t devpts devpts /dev/pts
mount -t cgroup2 cgroup2 /sys/fs/cgroup
ip link set lo up
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
for report in /out/contracts.log /out/permissions.log /out/ui-*.log; do
    echo "QEMU_REPORT=$report"
    cat "$report"
done
if [ -x /work/build-env/run-docker-guest.sh ]; then
    PODS_TEST_GUEST=1 PODS_TEST_USER=tester PODS_TEST_LIVE_BIN=/opt/pods-tests/engine-live \
        /work/build-env/run-docker-guest.sh
    echo "QEMU_DOCKER_TEST_EXIT=$?"
fi
echo QEMU_NATIVE_TEST_GUEST_FINISHED
sync
mount -o remount,ro /
poweroff -f
