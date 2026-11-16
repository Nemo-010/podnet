#!/bin/sh
# A consumer as a test (T-358 step 3): a Python HTTP client under the vnet
# shim.
#
# Usage: sh scripts/consumer-python.sh PODNET [PODLAB [PROFILE]]
#
# PODNET is the podnet binary to test. PODLAB is the binary podlab of
# PodUtils/podcrates (crate pod-lab, feature cli); PROFILE is the lab
# (default bind-denied: each AF_INET and AF_INET6 bind and connect gives
# EACCES). With no PODLAB, the script runs only the cases with no lab.
#
# The client (tests/consumers/python-http/client.py) sends GET with
# urllib.request of the standard library, and with requests when python3
# can import it. podnet net shim --forward 127.0.0.1:PORT=unix:PATH sends
# the connect to a broker on a unix socket. A TCP server on 127.0.0.1:PORT
# (the decoy) gives another body.
#
# The cases, for each library:
#   direct      no lab, no shim: the client gets the body of the decoy
#               (the control of the decoy).
#   shim        no lab, the shim: the client gets the body of the broker.
#   proxy       no lab, the shim, HTTP_PROXY=http://127.0.0.1:PROXYPORT
#               and a forward of PROXYPORT to a proxy on a unix socket:
#               the client gets the body of the proxy, and the proxy gets
#               the absolute URI (the egress path of a consumer).
#   lab-bare    the lab, no shim: the connect fails with EACCES (proof
#               that the lab refuses AF_INET).
#   lab-shim    the lab, the shim: the client gets the body of the broker.
#   lab-proxy   the lab, the shim and HTTP_PROXY: the body of the proxy.
# In each shim case, the server on the unix socket reads the peer of its
# AF_UNIX connection (SO_PEERCRED) and the peer must be the python process;
# the shim log must show the forward. As root, the lab cases run as uid
# 65534 (setpriv).
#
# Environment: CONSUMER_DIR (default .work/consumer-python in the current
# directory; not in /tmp or HOME, which a lab can hide), CONSUMER_PORT
# (default 7106; PORT+1 is the port of the proxy), CONSUMER_SHIM (a shim
# to use in place of the shim of PODNET: a planted defect, for the proof
# that the check fails). It needs python3 and timeout. It exits 0 when
# each case passes, else 1, and prints the output of a failed case.

set -u

if test $# -lt 1 || test $# -gt 3; then
    echo "usage: sh scripts/consumer-python.sh PODNET [PODLAB [PROFILE]]" >&2
    exit 64
fi

abs() { (cd "$(dirname "$1")" && printf '%s/%s\n' "$(pwd)" "$(basename "$1")"); }
podnet=$(abs "$1")
podlab=
test $# -ge 2 && podlab=$(abs "$2")
profile=${3:-bind-denied}
root=$(cd "$(dirname "$0")/.." && pwd)
port=${CONSUMER_PORT:-7106}
pport=$((port + 1))
work=${CONSUMER_DIR:-$PWD/.work/consumer-python}

rm -rf "$work"
mkdir -p "$work" || exit 1
work=$(cd "$work" && pwd)
case $work in
/tmp | /tmp/*) echo "consumer-python: CONSUMER_DIR $work is in /tmp; a lab hides it" >&2; exit 64 ;;
esac
chmod 755 "$work" || exit 1
cp "$root/tests/consumers/python-http/client.py" "$root/tests/consumers/servers.py" "$work/" || exit 1
echo "python $(python3 -c 'import platform; print(platform.python_version())')"

# requests: copy the package and its dependencies, so that uid 65534 (and
# python3 -I) can import them.
libraries=urllib
if python3 -c 'import requests' 2>/dev/null; then
    mkdir -p "$work/pylib"
    python3 - "$work/pylib" <<'EOF' || exit 1
import importlib, os, shutil, sys
for name in ("requests", "urllib3", "idna", "charset_normalizer", "certifi"):
    try:
        mod = importlib.import_module(name)
    except ImportError:
        continue
    src = os.path.dirname(mod.__file__)
    shutil.copytree(src, os.path.join(sys.argv[1], name), ignore=shutil.ignore_patterns("__pycache__"))
EOF
    libraries="urllib requests"
    echo "requests $(python3 -c 'import requests; print(requests.__version__)')"
else
    echo "requests: not installed; only urllib runs"
fi
chmod -R a+rX "$work"

# The broker and the proxy on unix sockets, and the decoys on TCP.
sock="$work/broker.sock"
psock="$work/proxy.sock"
python3 -I "$work/servers.py" http-unix "$sock" podnet-consumer-unix "$work/peers.txt" >"$work/broker.txt" 2>&1 &
pids=$!
python3 -I "$work/servers.py" http-unix "$psock" podnet-consumer-proxy "$work/peers.txt" >"$work/proxy.txt" 2>&1 &
pids="$pids $!"
python3 -I "$work/servers.py" http-tcp "$port" tcp-direct >"$work/tcp.txt" 2>&1 &
pids="$pids $!"
python3 -I "$work/servers.py" http-tcp "$pport" tcp-proxy-decoy >"$work/tcp2.txt" 2>&1 &
pids="$pids $!"
# $pids is a list of numbers: split it.
# shellcheck disable=SC2086
trap 'kill $pids 2>/dev/null' EXIT
for s in "$sock" "$psock"; do
    i=0
    while ! test -S "$s"; do
        i=$((i + 1))
        test $i -gt 100 && { cat "$work/broker.txt" "$work/proxy.txt"; echo "consumer-python: a server did not start" >&2; exit 1; }
        sleep 0.1
    done
done

forward="127.0.0.1:$port=unix:$sock,127.0.0.1:$pport=unix:$psock"
if ! "$podnet" net shim --out "$work/shim" --forward "127.0.0.1:$port=unix:$sock" \
    --forward "127.0.0.1:$pport=unix:$psock" --json >"$work/shim.json" 2>"$work/shim.txt"; then
    cat "$work/shim.txt"
    echo "consumer-python: podnet net shim failed" >&2
    exit 1
fi
grep -q "unix:$psock" "$work/shim.json" || { cat "$work/shim.json"; echo "consumer-python: no forward in the shim environment" >&2; exit 1; }
chmod -R a+rX "$work/shim"
# CONSUMER_SHIM takes another shim (a planted defect: the check must fail).
shim=${CONSUMER_SHIM:-$work/shim/libpodnet-vnet.so}

# The prefix of a lab case: the lab, as uid 65534 when the script runs as
# root.
as_user=
test "$(id -u)" = 0 && as_user="setpriv --reuid 65534 --regid 65534 --clear-groups"

# run NAME LIBRARY LAB(0|1) SHIM(0|1) PROXY(0|1): run the client; the
# output goes to $work/NAME.txt, the exit code to $code.
run() {
    name=$1 library=$2 lab=$3 useshim=$4 proxy=$5
    : >"$work/peers.txt"
    set -- env -u NO_PROXY -u no_proxy -u HTTP_PROXY -u http_proxy -u HTTPS_PROXY -u https_proxy \
        -u ALL_PROXY -u all_proxy HOME="$work"
    if test "$useshim" = 1; then
        set -- "$@" LD_PRELOAD="$shim" PODNET_VNET_FORWARD="$forward" PODNET_VNET_LOG=1
    fi
    url="http://127.0.0.1:$port/"
    if test "$proxy" = 1; then
        set -- "$@" HTTP_PROXY="http://127.0.0.1:$pport" http_proxy="http://127.0.0.1:$pport"
        url="http://consumer.test/"
    fi
    set -- "$@" timeout 60 python3 -I "$work/client.py" "$library" "$url"
    test "$library" = requests && set -- "$@" "$work/pylib"
    if test "$lab" = 1; then
        # $as_user is empty or a command and its options: split it.
        # shellcheck disable=SC2086
        $as_user "$podlab" run "$profile" -- "$@" >"$work/$name.txt" 2>&1
    else
        "$@" >"$work/$name.txt" 2>&1
    fi
    code=$?
}

failed=0
result() {
    if test "$2" = pass; then
        printf '%-22s pass\n' "$1"
    else
        printf '%-22s fail: %s\n' "$1" "$2"
        sed 's/^/    /' "$work/$1.txt"
        test -s "$work/peers.txt" && sed 's/^/    server: /' "$work/peers.txt"
        failed=1
    fi
}

# check_shim NAME BODY: the body of the server on the unix socket, the
# shim log and the peer.
check_shim() {
    if test $code -ne 0; then
        result "$1" "exit $code"
    elif ! grep -qx "$2" "$work/$1.txt"; then
        result "$1" "no body $2"
    elif ! grep -q 'connect -> forward' "$work/$1.txt"; then
        result "$1" "no forward in the shim log"
    elif ! grep -q 'comm=python3' "$work/peers.txt"; then
        result "$1" "the peer of the AF_UNIX connection is not the python process"
    elif test "$2" = podnet-consumer-proxy && ! grep -q 'request GET http://consumer.test/ ' "$work/peers.txt"; then
        result "$1" "the proxy did not get the absolute URI"
    else
        result "$1" pass
        sed 's/^/    server: /' "$work/peers.txt"
    fi
}

if test -n "$podlab"; then
    # shellcheck disable=SC2086
    if ! $as_user "$podlab" run "$profile" -- /bin/true >"$work/lab.txt" 2>&1; then
        cat "$work/lab.txt"
        echo "consumer-python: the lab $profile does not hold here" >&2
        exit 1
    fi
    echo "lab $profile as uid $( ($as_user "$podlab" run "$profile" -- id -u) 2>/dev/null)"
fi

for lib in $libraries; do
    run "direct-$lib" "$lib" 0 0 0
    if test $code -eq 0 && grep -qx 'tcp-direct' "$work/direct-$lib.txt"; then
        result "direct-$lib" pass
    else
        result "direct-$lib" "exit $code, no body of the TCP server"
    fi
    run "shim-$lib" "$lib" 0 1 0
    check_shim "shim-$lib" podnet-consumer-unix
    run "proxy-$lib" "$lib" 0 1 1
    check_shim "proxy-$lib" podnet-consumer-proxy
    test -n "$podlab" || continue
    run "lab-bare-$lib" "$lib" 1 0 0
    if test $code -eq 0; then
        result "lab-bare-$lib" "the client connected with no shim: the lab does not refuse AF_INET"
    elif ! grep -q 'Permission denied' "$work/lab-bare-$lib.txt"; then
        result "lab-bare-$lib" "exit $code, but not EACCES"
    else
        result "lab-bare-$lib" pass
        grep -m 1 'Permission denied' "$work/lab-bare-$lib.txt" | sed 's/^/    /'
    fi
    run "lab-shim-$lib" "$lib" 1 1 0
    check_shim "lab-shim-$lib" podnet-consumer-unix
    run "lab-proxy-$lib" "$lib" 1 1 1
    check_shim "lab-proxy-$lib" podnet-consumer-proxy
done
exit $failed
