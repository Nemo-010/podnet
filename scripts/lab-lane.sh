#!/bin/sh
# The lane of podnet in the profiles of pod-lab (T-358 step 1).
#
# Usage: sh scripts/lab-lane.sh PODLAB PODNET [PROFILE...]
#
# PODLAB is the binary podlab of PodUtils/podcrates (crate pod-lab, feature
# cli), or a directory from scripts/get-podlab.sh: the lane then checks the
# asset podlab-ARCH-unknown-linux-musl against the SHA256SUMS of the
# directory and runs it. PODNET is the podnet binary to test. With no
# PROFILE, the lane runs in each profile of `podlab list`, and in the row
# allow-connect. Each command
# runs in a lab of the profile: `podlab run PROFILE -- sh lab-lane.sh
# --case NAME ...`.
#
# The row allow-connect is not a profile of podlab. It runs the rows of
# PROXY_CASES in the lab
# `podlab run --allow-connect PORT bind-denied` (podlab 0.58.0 or later):
# each TCP connect is refused, except to PORT. A stand-in HTTP CONNECT
# proxy (tests/consumers/connect-proxy.py) listens on 127.0.0.1:PORT
# outside the lab. It connects only proxy.test:PORT+1, to a server on
# 127.0.0.1:PORT+1. LANE_PROXY_PORT gives PORT (default 7120). LANE_PLANT
# plants a defect: refuse (the proxy refuses each CONNECT) or port (podnet
# gets the port PORT+2, which the lab refuses). Each pod row must then fail.
#
# The network labs of NET_LABS (podlab 0.60.0 or later) run the rows of
# NET_CASES. A lab gives its own proxy (HTTPS_PROXY, maybe with a
# credential), a TLS stand-in server for each name, and SSL_CERT_FILE.
# The plant LANE_PLANT=net gives podnet a proxy that does not listen; each
# net-pod row must then fail.
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
NET_LABS="proxy-only proxy-tls-reterminate proxy-per-name ws-upgrade-refused sni-allowlist no-proxy-no-dns ipv6-only"
NET_CASES="version doctor net-curl net-pod-auto net-pod-seccomp net-pod-netns"
PROXY_CASES="proxy-direct proxy-port proxy-pod-auto proxy-pod-seccomp proxy-pod-env proxy-doctor"

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
        # In a network lab, the doctor names the proxy of the lab, and its
        # credential where the lab gives one.
        if in_list "$LANE_PROFILE" "$NET_LABS"; then
            want="proxy  none: each connection is direct"
            if test -n "${HTTPS_PROXY:-}"; then
                want="proxy  http://127.0.0.1:3128 (credentials: none)"
                case $HTTPS_PROXY in *@*) want="proxy  http://127.0.0.1:3128 (credentials: present)" ;; esac
            fi
            grep -qxF "$want" "$dir/doctor.txt" || { echo "no line: $want"; exit 1; }
        fi
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
    proxy-*)
        proxy_case "$name" "$podnet" "$dir"
        exit $?
        ;;
    net-*)
        net_case "$name" "$podnet" "$dir"
        exit $?
        ;;
    *)
        echo "unknown case $name"
        exit 2
        ;;
    esac
}

# The rows of allow-connect, in the lab with --allow-connect of the proxy
# port. The proxy log is $dir/../proxy.txt; the outer script empties it
# before each row.
proxy_case() {
    name=$1
    podnet=$2
    dir=$3
    port=$LANE_PROXY_PORT
    tport=$((port + 1))
    upstream="http://127.0.0.1:$LANE_UPSTREAM_PORT"
    url="http://proxy.test:$tport/"
    body='podnet-proxy-only'
    log="$dir/../proxy.txt"
    # The variables of the host of this script are not the proxy of the lab.
    unset HTTPS_PROXY https_proxy HTTP_PROXY http_proxy ALL_PROXY all_proxy NO_PROXY no_proxy
    case $name in
    proxy-direct)
        # The negative control: with no podnet and no proxy, the lab
        # refuses the connect to the server.
        python3 -I "$dir/../http-client.py" urllib "http://127.0.0.1:$tport/" >"$dir/out.txt" 2>&1
        code=$?
        cat "$dir/out.txt"
        echo "client exit $code"
        test $code -ne 0 && grep -q 'Permission denied' "$dir/out.txt"
        return $?
        ;;
    proxy-port)
        # The control of the lab: the proxy port is open, with no podnet.
        python3 -I "$dir/../connect.py" "$port" "proxy.test:$tport" >"$dir/out.txt" 2>&1
        code=$?
        cat "$dir/out.txt" "$log"
        echo "client exit $code"
        test $code -eq 0 && grep -qx "$body" "$dir/out.txt"
        return $?
        ;;
    proxy-pod-auto | proxy-pod-seccomp)
        backend=${name#proxy-pod-}
        timeout 60 "$podnet" pod --backend "$backend" -x "$upstream" -- \
            python3 -I "$dir/../http-client.py" urllib "$url" >"$dir/out.txt" 2>&1
        code=$?
        ;;
    proxy-pod-env)
        # The upstream from the environment, as a proxy-only host sets it.
        HTTPS_PROXY=$upstream HTTP_PROXY=$upstream NO_PROXY=localhost,127.0.0.1 \
            timeout 60 "$podnet" pod -- python3 -I "$dir/../http-client.py" urllib "$url" >"$dir/out.txt" 2>&1
        code=$?
        ;;
    proxy-doctor)
        # The doctor sends its probes through the proxy, and names the
        # refusals of the proxy (the stand-in refuses each other target).
        HTTPS_PROXY=$upstream timeout 120 "$podnet" doctor >"$dir/out.txt" 2>&1
        code=$?
        cat "$dir/out.txt" "$log"
        echo "doctor exit $code"
        case $code in 0 | 1) ;; *) return 1 ;; esac
        grep -q 'panicked' "$dir/out.txt" && return 1
        grep -q '^request CONNECT ' "$log" && grep -q 'proxy 403' "$dir/out.txt"
        return $?
        ;;
    esac
    cat "$dir/out.txt" "$log"
    echo "pod exit $code"
    test $code -eq 0 && grep -qx "$body" "$dir/out.txt" && grep -q "^request CONNECT proxy.test:$tport " "$log"
}

# The rows of the network labs. The lab sets HTTPS_PROXY (or no proxy)
# and SSL_CERT_FILE; its TLS stand-in answers each name.
net_case() {
    name=$1
    podnet=$2
    dir=$3
    url=https://lab.test/
    if test -z "${HTTPS_PROXY:-}"; then
        # No proxy: podnet pod has no egress, and must say so.
        test "$name" = net-curl && { echo "the lab has no proxy"; return 77; }
        timeout 60 "$podnet" pod -- /bin/true >"$dir/out.txt" 2>&1
        code=$?
        cat "$dir/out.txt"
        echo "pod exit $code"
        test $code -ne 0 && grep -q 'no proxy: give -x PROXY' "$dir/out.txt" || return 1
        echo "the lab has no proxy; podnet pod refused with the reason"
        return 77
    fi
    case $name in
    net-curl)
        # The control of the lab: curl reaches the stand-in through the proxy.
        curl -sS -m 20 -o /dev/null -w '%{http_code}\n' "$url" >"$dir/out.txt" 2>&1
        code=$?
        cat "$dir/out.txt"
        test $code -eq 0 && grep -qx 200 "$dir/out.txt"
        return $?
        ;;
    esac
    backend=${name#net-pod-}
    if test "${LANE_PLANT:-}" = net; then
        # The plant: a proxy that does not listen.
        HTTPS_PROXY=http://127.0.0.1:1 https_proxy=http://127.0.0.1:1
        HTTP_PROXY=http://127.0.0.1:1 http_proxy=http://127.0.0.1:1
        export HTTPS_PROXY https_proxy HTTP_PROXY http_proxy
    fi
    timeout 60 "$podnet" pod --backend "$backend" -- \
        curl -sS -m 20 -o /dev/null -w '%{http_code}\n' "$url" >"$dir/out.txt" 2>&1
    code=$?
    cat "$dir/out.txt"
    echo "pod exit $code"
    test $code -eq 0 && grep -qx 200 "$dir/out.txt" && return 0
    if test $code -eq 3 && refusal_expected "pod-$backend" "$LANE_PROFILE"; then
        echo "podnet refused with exit 3 and a reason: $why"
        return 77
    fi
    return 1
}

in_list() {
    for i in $2; do
        test "$i" = "$1" && return 0
    done
    return 1
}

# The profiles that take what a pod backend needs, by design. It sets why.
refusal_expected() {
    case $1:$2 in
    pod-netns:userns-off | pod-netns:nonroot-nocaps | pod-netns:seccomp-nonewprivs)
        why="no user namespace"
        ;;
    pod-netns:bind-denied)
        # Since T-382 the backend probes a TCP connect first.
        why="the lab refuses each TCP connect of the program (Landlock), in each namespace"
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
    cat >"$1/connect.py" <<'EOF'
import socket, sys
c = socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=10)
c.sendall(b"CONNECT %s HTTP/1.1\r\nHost: %s\r\n\r\n" % ((sys.argv[2].encode(),) * 2))
head = b""
while b"\r\n\r\n" not in head:
    b = c.recv(1)
    if not b:
        sys.exit("no reply of the proxy")
    head += b
if head.split()[1] != b"200":
    sys.exit("proxy: " + head.decode().splitlines()[0])
c.sendall(b"GET / HTTP/1.0\r\n\r\n")
data = b""
while True:
    b = c.recv(4096)
    if not b:
        break
    data += b
sys.stdout.write(data.split(b"\r\n\r\n", 1)[1].decode() + "\n")
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
if test -d "$podlab"; then
    # A directory of scripts/get-podlab.sh: the asset of this machine,
    # checked against the SHA256SUMS of the release.
    asset="podlab-$(uname -m)-unknown-linux-musl"
    if ! (cd "$podlab" && grep " $asset\$" SHA256SUMS | sha256sum -c --status -); then
        echo "lab-lane: $podlab/$asset does not match $podlab/SHA256SUMS" >&2
        exit 1
    fi
    podlab="$podlab/$asset"
    test -x "$podlab" || { echo "lab-lane: $podlab is not executable" >&2; exit 1; }
fi
podnet=$(abs "$2")
self=$(abs "$0")
root=$(cd "$(dirname "$self")/.." && pwd)
shift 2
work=${LANE_DIR:-$PWD/.work/lab-lane}
rm -rf "$work"
mkdir -p "$work" || exit 1
work=$(cd "$work" && pwd)
case $work in
/tmp | /tmp/*) echo "lab-lane: LANE_DIR $work is in /tmp; a lab hides it" >&2; exit 64 ;;
esac
write_helpers "$work"
cp "$root/tests/consumers/python-http/client.py" "$work/http-client.py" || exit 1
cp "$root/tests/consumers/connect-proxy.py" "$root/tests/consumers/servers.py" "$work/" || exit 1
python3 -I "$work/echo.py" "$work/echo.sock" &
pids=$!
# $pids is a list of numbers: split it.
# shellcheck disable=SC2086
trap 'kill $pids 2>/dev/null' EXIT
i=0
while ! test -S "$work/echo.sock"; do
    i=$((i + 1))
    test $i -gt 100 && { echo "lab-lane: the echo server did not start" >&2; exit 1; }
    sleep 0.1
done

LANE_PROXY_PORT=${LANE_PROXY_PORT:-7120}
LANE_UPSTREAM_PORT=$LANE_PROXY_PORT
proxy_mode=
case ${LANE_PLANT:-} in
'') ;;
refuse) proxy_mode=refuse ;;
port) LANE_UPSTREAM_PORT=$((LANE_PROXY_PORT + 2)) ;;
net) ;;
*) echo "lab-lane: LANE_PLANT is refuse, port or net" >&2; exit 64 ;;
esac
export LANE_PROXY_PORT LANE_UPSTREAM_PORT

# The stand-in proxy and its server, outside the lab.
start_proxy() {
    tport=$((LANE_PROXY_PORT + 1))
    python3 -I "$work/servers.py" http-tcp "$tport" podnet-proxy-only >"$work/target.out" 2>&1 &
    pids="$pids $!"
    : >"$work/proxy.txt"
    python3 -I "$work/connect-proxy.py" "$LANE_PROXY_PORT" "$work/proxy.txt" \
        "proxy.test:$tport=$tport" $proxy_mode >"$work/proxy.out" 2>&1 &
    pids="$pids $!"
    i=0
    while ! grep -qx ready "$work/proxy.out" 2>/dev/null; do
        i=$((i + 1))
        test $i -gt 100 && { cat "$work/proxy.out"; return 1; }
        sleep 0.1
    done
}

if test $# -eq 0; then
    # Each profile name is one word with no space or glob character: the
    # split of the list into the arguments is the intent.
    # shellcheck disable=SC2046
    set -- $("$podlab" list | awk '/^[^ ]/ { print $1 }') allow-connect
fi

failed=0
printf '%-22s %-17s %s\n' PROFILE COMMAND RESULT
for profile in "$@"; do
    # allow-connect: the lab bind-denied with a connect to the proxy port
    # only. A network lab runs the rows of NET_CASES.
    lab=$profile
    cases=$CASES
    if test "$profile" = allow-connect; then
        lab="--allow-connect $LANE_PROXY_PORT bind-denied"
        cases=$PROXY_CASES
    elif in_list "$profile" "$NET_LABS"; then
        cases=$NET_CASES
    fi
    # $lab is the profile, or options and the profile: split it.
    # shellcheck disable=SC2086
    "$podlab" run $lab -- /bin/true >"$work/$profile.lab.txt" 2>&1
    code=$?
    if test $code -ne 0; then
        why=$(grep -v '^ ' "$work/$profile.lab.txt" | grep -v '^holds' | head -n 1)
        printf '%-22s %-17s n.a.: the lab does not hold (exit %s): %s\n' "$profile" '*' "$code" "$why"
        continue
    fi
    if test "$profile" = allow-connect && ! start_proxy; then
        printf '%-22s %-17s fail: the stand-in proxy did not start\n' "$profile" '*'
        failed=1
        continue
    fi
    for c in $cases; do
        log="$work/$profile.$c.txt"
        : >"$work/proxy.txt"
        # shellcheck disable=SC2086
        LANE_PROFILE=$profile "$podlab" run $lab -- /bin/sh "$self" --case "$c" "$podnet" "$work/$profile-$c" >"$log" 2>&1
        code=$?
        case $code in
        0) result=pass ;;
        77) result="n.a.: $(tail -n 1 "$log")" ;;
        *)
            result="fail (exit $code, $log)"
            failed=1
            ;;
        esac
        printf '%-22s %-17s %s\n' "$profile" "$c" "$result"
    done
done
exit $failed
