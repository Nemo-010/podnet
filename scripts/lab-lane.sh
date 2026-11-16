#!/bin/sh
# The lane of podnet in the profiles of pod-lab (T-358 step 1).
#
# Usage: sh scripts/lab-lane.sh PODLAB PODNET [PROFILE...]
#
# PODLAB is the binary podlab of PodUtils/podcrates (crate pod-lab, feature
# cli). PODNET is the podnet binary to test. With no PROFILE, the lane runs
# in each profile of `podlab list`. Each command runs in a lab of the
# profile: `podlab run PROFILE -- sh lab-lane.sh --case NAME ...`.
#
# The script prints one row for each profile and command: pass, fail or
# n.a. with the reason. It exits 1 when a command fails, else 0. The logs
# are in LANE_DIR (default: .work/lab-lane in the current directory).
# LANE_DIR must not be in /tmp (a lab mounts a tmpfs on /tmp) or in HOME
# (a lab mounts HOME noexec). python3 runs the stand-in servers and the
# dynamic client.
#
# Run it as a user that is not root: the profile nonroot-nocaps does not
# hold for root, and the other labs then run as root.

set -u

CASES="version man doctor shim-forward link-unix pod-seccomp pod-netns net-proxy-unix"

# The cases. Each one runs inside the lab and exits 0 (pass), 1 (fail) or
# 77 (not applicable, with the reason on the last line of its output).
case_main() {
    name=$1
    podnet=$2
    dir=$3
    mkdir -p "$dir" || exit 1
    export XDG_STATE_HOME="$dir/state" XDG_RUNTIME_DIR="$dir/run" XDG_CONFIG_HOME="$dir/config"
    mkdir -p "$XDG_STATE_HOME" "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME"
    chmod 700 "$XDG_RUNTIME_DIR"
    case $name in
    version)
        out=$("$podnet" --version) || exit 1
        echo "$out"
        case $out in podnet\ [0-9]*) exit 0 ;; *) exit 1 ;; esac
        ;;
    man)
        "$podnet" man --no-pager >"$dir/man.txt" || exit 1
        grep -q '^EXIT STATUS' "$dir/man.txt" || { echo "no EXIT STATUS section"; exit 1; }
        wc -l <"$dir/man.txt"
        ;;
    doctor)
        timeout 120 "$podnet" doctor >"$dir/doctor.txt" 2>&1
        code=$?
        cat "$dir/doctor.txt"
        echo "exit $code"
        # 0: a leg can work; 1: no leg can work. Both are answers.
        case $code in 0 | 1) ;; *) exit 1 ;; esac
        if grep -q 'panicked' "$dir/doctor.txt"; then exit 1; fi
        test -s "$dir/doctor.txt" || exit 1
        ;;
    shim-forward)
        # The echo server runs outside the lab (the broker of a consumer).
        sock="$dir/../echo.sock"
        "$podnet" net shim --out "$dir/shim" --forward "127.0.0.1:7001=unix:$sock" --json >"$dir/shim.json" ||
            exit 1
        cat "$dir/shim.json"
        # The negative control: with no shim, nothing answers at 127.0.0.1:7001.
        bare=$(python3 -I "$dir/../client.py" 127.0.0.1 7001 2>&1)
        echo "with no shim: $bare"
        got=$(python3 -I "$dir/../client.py" --env "$dir/shim.json" 127.0.0.1 7001)
        code=$?
        echo "client exit $code: $got"
        test "$bare" != "echo:ping" && test $code -eq 0 && test "$got" = "echo:ping"
        exit $?
        ;;
    link-unix)
        fp=$("$podnet" link status --create --json | python3 -I -c \
            'import json,sys; print(json.load(sys.stdin)["identity"]["fingerprint"])') || exit 1
        echo "fingerprint $fp"
        printf '%s\n' "$fp" >"$dir/allow"
        chmod 600 "$dir/allow"
        "$podnet" link listen "$dir/link.sock" --allow "$dir/allow" --to "unix:$dir/../echo.sock" \
            >"$dir/listen.txt" 2>&1 &
        listener=$!
        if ! wait_path "$dir/link.sock"; then
            kill $listener 2>/dev/null
            cat "$dir/listen.txt"
            listen_refused "$dir"
            exit $?
        fi
        got=$(printf 'ping' | timeout 30 "$podnet" link dial "$dir/link.sock" --peer "$fp" 2>"$dir/dial.txt")
        code=$?
        kill $listener 2>/dev/null
        cat "$dir/listen.txt" "$dir/dial.txt"
        echo "dial exit $code: $got"
        test $code -eq 0 && test "$got" = "echo:ping"
        exit $?
        ;;
    pod-seccomp | pod-netns)
        backend=${name#pod-}
        timeout 60 "$podnet" pod --backend "$backend" -x "socks5://unix:$dir/none.sock" -- /bin/true \
            >"$dir/pod.txt" 2>&1
        code=$?
        cat "$dir/pod.txt"
        echo "exit $code"
        test $code -eq 0 && exit 0
        # Exit 3: the backend cannot run here, with the reason. That is
        # not applicable only where the profile takes what the backend
        # needs by design (refusal_expected). Else it is a failure.
        if test $code -eq 3 && test -s "$dir/pod.txt" && refusal_expected "$name" "$LANE_PROFILE"; then
            echo "podnet refused with exit 3 and a reason: $why"
            exit 77
        fi
        exit 1
        ;;
    net-proxy-unix)
        "$podnet" net proxy --listen "unix:$dir/proxy.sock" >"$dir/proxy.txt" 2>&1 &
        proxy=$!
        if ! wait_path "$dir/proxy.sock"; then
            kill $proxy 2>/dev/null
            cat "$dir/proxy.txt"
            listen_refused "$dir"
            exit $?
        fi
        got=$(python3 -I "$dir/../socks.py" "$dir/proxy.sock")
        code=$?
        kill $proxy 2>/dev/null
        cat "$dir/proxy.txt"
        echo "greeting exit $code: $got"
        test $code -eq 0 && test "$got" = "0500"
        exit $?
        ;;
    *)
        echo "unknown case $name"
        exit 2
        ;;
    esac
}

# The profiles that take what a pod backend needs, by design. It sets why.
refusal_expected() {
    case $1:$2 in
    pod-netns:userns-off | pod-netns:nonroot-nocaps | pod-netns:seccomp-nonewprivs)
        why="no user namespace"
        ;;
    pod-netns:no-proc | pod-netns:proc-masked)
        why="no /proc for the id maps"
        ;;
    pod-seccomp:no-proc | pod-seccomp:proc-masked)
        why="no /proc/PID/mem of the program"
        ;;
    pod-netns:no-net-admin)
        # Since Linux 5.12, a map of the uid 0 of the parent needs
        # CAP_SETFCAP: root with no capability cannot map itself.
        test "$(id -u)" = 0 || return 1
        why="uid 0 with no capability cannot write its id map"
        ;;
    pod-netns:raw-denied)
        # The veth wire needs AF_PACKET; with /dev/net/tun the TUN works.
        test -r /dev/net/tun && test -w /dev/net/tun && return 1
        why="no /dev/net/tun for this user, and no AF_PACKET for the veth wire"
        ;;
    *) return 1 ;;
    esac
}

# A listener of podnet did not start. Where the profile refuses listen(2)
# of AF_UNIX (listen-denied), podnet cannot serve: not applicable, when
# podnet said so. Else a failure.
listen_refused() {
    if python3 -I "$1/../listen.py" "$1/probe.sock"; then
        return 1
    fi
    echo "the profile refuses listen(2) of AF_UNIX; podnet gave an error and stopped"
    return 77
}

wait_path() {
    i=0
    while ! test -S "$1"; do
        i=$((i + 1))
        test $i -gt 100 && return 1
        sleep 0.1
    done
}

write_helpers() {
    cat >"$1/echo.py" <<'EOF'
import os, socket, sys
path = sys.argv[1]
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind(path)
s.listen(8)
while True:
    c, _ = s.accept()
    data = b""
    while True:
        b = c.recv(4096)
        if not b:
            break
        data += b
    c.sendall(b"echo:" + data)
    c.close()
EOF
    cat >"$1/listen.py" <<'EOF'
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind(sys.argv[1])
try:
    s.listen(1)
except PermissionError:
    sys.exit(1)
EOF
    cat >"$1/client.py" <<'EOF'
import json, os, socket, sys
if sys.argv[1] == "--env":
    # Start this client again, dynamic, with the environment of the shim.
    env = dict(os.environ)
    with open(sys.argv[2]) as f:
        env.update(json.load(f))
    os.execve(sys.executable, [sys.executable, "-I", sys.argv[0]] + sys.argv[3:], env)
c = socket.create_connection((sys.argv[1], int(sys.argv[2])), timeout=10)
c.sendall(b"ping")
c.shutdown(socket.SHUT_WR)
data = b""
while True:
    b = c.recv(4096)
    if not b:
        break
    data += b
sys.stdout.write(data.decode())
EOF
    cat >"$1/socks.py" <<'EOF'
import socket, sys
c = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
c.settimeout(10)
c.connect(sys.argv[1])
c.sendall(b"\x05\x01\x00")
sys.stdout.write(c.recv(2).hex())
EOF
}

if test "${1:-}" = "--case"; then
    case_main "$2" "$3" "$4"
    exit $?
fi

if test $# -lt 2; then
    echo "usage: sh scripts/lab-lane.sh PODLAB PODNET [PROFILE...]" >&2
    exit 64
fi

abs() { (cd "$(dirname "$1")" && printf '%s/%s\n' "$(pwd)" "$(basename "$1")"); }
podlab=$(abs "$1")
podnet=$(abs "$2")
self=$(abs "$0")
shift 2
work=${LANE_DIR:-$PWD/.work/lab-lane}
rm -rf "$work"
mkdir -p "$work" || exit 1
work=$(cd "$work" && pwd)
case $work in
/tmp | /tmp/*) echo "lab-lane: LANE_DIR $work is in /tmp; a lab hides it" >&2; exit 64 ;;
esac
write_helpers "$work"
python3 -I "$work/echo.py" "$work/echo.sock" &
echo_server=$!
trap 'kill $echo_server 2>/dev/null' EXIT
i=0
while ! test -S "$work/echo.sock"; do
    i=$((i + 1))
    test $i -gt 100 && { echo "lab-lane: the echo server did not start" >&2; exit 1; }
    sleep 0.1
done

if test $# -eq 0; then
    # Each profile name is one word with no space or glob character: the
    # split of the list into the arguments is the intent.
    # shellcheck disable=SC2046
    set -- $("$podlab" list | awk '/^[^ ]/ { print $1 }')
fi

failed=0
printf '%-22s %-16s %s\n' PROFILE COMMAND RESULT
for profile in "$@"; do
    "$podlab" run "$profile" -- /bin/true >"$work/$profile.lab.txt" 2>&1
    code=$?
    if test $code -ne 0; then
        why=$(grep -v '^ ' "$work/$profile.lab.txt" | grep -v '^holds' | head -n 1)
        printf '%-22s %-16s n.a.: the lab does not hold (exit %s): %s\n' "$profile" '*' "$code" "$why"
        continue
    fi
    for c in $CASES; do
        log="$work/$profile.$c.txt"
        LANE_PROFILE=$profile "$podlab" run "$profile" -- /bin/sh "$self" --case "$c" "$podnet" "$work/$profile-$c" >"$log" 2>&1
        code=$?
        case $code in
        0) result=pass ;;
        77) result="n.a.: $(tail -n 1 "$log")" ;;
        *)
            result="fail (exit $code, $log)"
            failed=1
            ;;
        esac
        printf '%-22s %-16s %s\n' "$profile" "$c" "$result"
    done
done
exit $failed
