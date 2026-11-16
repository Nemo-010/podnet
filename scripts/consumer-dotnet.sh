#!/bin/sh
# A consumer as a test (T-358 step 3): a .NET HttpClient under the vnet shim.
#
# Usage: sh scripts/consumer-dotnet.sh PODNET [PODLAB [PROFILE]]
#
# PODNET is the podnet binary to test. PODLAB is the binary podlab of
# PodUtils/podcrates (crate pod-lab, feature cli); PROFILE is the lab
# (default bind-denied: each AF_INET and AF_INET6 bind and connect gives
# EACCES). With no PODLAB, the script runs only the cases with no lab.
#
# The client (tests/consumers/dotnet-http) does what the runner of
# PodUtils/podrunner does: HttpClient with the default SocketsHttpHandler
# (dual-mode IPv6 sockets) sends GET to http://127.0.0.1:PORT/. A broker
# on a unix socket answers. podnet net shim --forward
# 127.0.0.1:PORT=unix:PATH sends the connect to the broker.
#
# The cases:
#   direct     no lab, no shim: the client gets the body of a TCP server
#              on 127.0.0.1:PORT (the control of the TCP server).
#   shim       no lab, the shim: the client gets the body of the unix
#              broker, not of the TCP server.
#   lab-bare   the lab, no shim: the connect fails with EACCES (proof that
#              the lab refuses AF_INET).
#   lab-shim   the lab, the shim: the client gets the body of the broker.
# In each shim case, the broker reads the peer of its AF_UNIX connection
# (SO_PEERCRED) and the peer must be the dotnet process; the shim log must
# show the forward. As root, the lab cases run as uid 65534 (setpriv).
#
# Environment: CONSUMER_DIR (default .work/consumer-dotnet in the current
# directory; not in /tmp or HOME, which a lab can hide), CONSUMER_PORT
# (default 7105), CONSUMER_SHIM (a shim to use in place of the shim of
# PODNET: a planted defect, for the proof that the check fails). It needs dotnet (SDK 8), python3, cc and timeout.
# It exits 0 when each case passes, else 1, and prints the output of a
# failed case.

set -u

if test $# -lt 1 || test $# -gt 3; then
    echo "usage: sh scripts/consumer-dotnet.sh PODNET [PODLAB [PROFILE]]" >&2
    exit 64
fi

abs() { (cd "$(dirname "$1")" && printf '%s/%s\n' "$(pwd)" "$(basename "$1")"); }
podnet=$(abs "$1")
podlab=
test $# -ge 2 && podlab=$(abs "$2")
profile=${3:-bind-denied}
root=$(cd "$(dirname "$0")/.." && pwd)
port=${CONSUMER_PORT:-7105}
work=${CONSUMER_DIR:-$PWD/.work/consumer-dotnet}
export DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1

command -v dotnet >/dev/null || { echo "consumer-dotnet: no dotnet (install dotnet-sdk-8.0)" >&2; exit 1; }
rm -rf "$work"
mkdir -p "$work/src" "$work/home" || exit 1
work=$(cd "$work" && pwd)
case $work in
/tmp | /tmp/*) echo "consumer-dotnet: CONSUMER_DIR $work is in /tmp; a lab hides it" >&2; exit 64 ;;
esac
# uid 65534 reads the files and writes the home of dotnet.
chmod 755 "$work" && chmod 777 "$work/home" || exit 1

# The client: build it once.
cp "$root/tests/consumers/dotnet-http/dotnet-http.csproj" "$root/tests/consumers/dotnet-http/Program.cs" "$work/src/" ||
    exit 1
if ! dotnet build -c Release -nologo -o "$work/app" "$work/src/dotnet-http.csproj" >"$work/build.txt" 2>&1; then
    cat "$work/build.txt"
    echo "consumer-dotnet: the build of the client failed" >&2
    exit 1
fi
chmod -R a+rX "$work/app"
echo "dotnet $(dotnet --version)"

# The broker on a unix socket, and a TCP server on 127.0.0.1:PORT with
# another body.
cat >"$work/server.py" <<'EOF'
import http.server, os, socket, socketserver, struct, sys

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = self.server.body
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def address_string(self):
        return "peer"

class Unix(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True

    def verify_request(self, request, client_address):
        # The peer of the AF_UNIX connection: its pid and its program.
        cred = request.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i"))
        pid, uid, _ = struct.unpack("3i", cred)
        try:
            with open("/proc/%d/comm" % pid) as f:
                comm = f.read().strip()
        except OSError:
            comm = "?"
        with open(self.peers, "a") as f:
            f.write("peer pid=%d uid=%d comm=%s\n" % (pid, uid, comm))
        return True

class Tcp(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True

if sys.argv[1] == "unix":
    os.umask(0)
    srv = Unix(sys.argv[2], Handler)
    srv.peers = sys.argv[3]
    srv.body = b"podnet-consumer-unix"
else:
    srv = Tcp(("127.0.0.1", int(sys.argv[2])), Handler)
    srv.body = b"tcp-direct"
srv.serve_forever()
EOF
sock="$work/broker.sock"
python3 -I "$work/server.py" unix "$sock" "$work/peers.txt" >"$work/broker.txt" 2>&1 &
broker=$!
python3 -I "$work/server.py" tcp "$port" >"$work/tcp.txt" 2>&1 &
tcp=$!
trap 'kill $broker $tcp 2>/dev/null' EXIT
i=0
while ! test -S "$sock"; do
    i=$((i + 1))
    test $i -gt 100 && { cat "$work/broker.txt"; echo "consumer-dotnet: the broker did not start" >&2; exit 1; }
    sleep 0.1
done

forward="127.0.0.1:$port=unix:$sock"
if ! "$podnet" net shim --out "$work/shim" --forward "$forward" --json >"$work/shim.json" 2>"$work/shim.txt"; then
    cat "$work/shim.txt"
    echo "consumer-dotnet: podnet net shim failed" >&2
    exit 1
fi
chmod -R a+rX "$work/shim"
# CONSUMER_SHIM takes another shim (a planted defect: the check must fail).
shim=${CONSUMER_SHIM:-$work/shim/libpodnet-vnet.so}

# The prefix of a lab case: the lab, as uid 65534 when the script runs as
# root.
as_user=
test "$(id -u)" = 0 && as_user="setpriv --reuid 65534 --regid 65534 --clear-groups"

# run NAME LAB(0|1) SHIM(0|1): run the client; the output goes to
# $work/NAME.txt, the exit code to $code.
run() {
    : >"$work/peers.txt"
    set -- "$1" "$2" "$3" env HOME="$work/home" DOTNET_CLI_HOME="$work/home"
    if test "$3" = 1; then
        set -- "$@" LD_PRELOAD="$shim" PODNET_VNET_FORWARD="$forward" PODNET_VNET_LOG=1
    fi
    set -- "$@" timeout 120 dotnet "$work/app/dotnet-http.dll" "$port"
    name=$1
    lab=$2
    shift 3
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
        printf '%-10s pass\n' "$1"
    else
        printf '%-10s fail: %s\n' "$1" "$2"
        sed 's/^/    /' "$work/$1.txt"
        test -s "$work/peers.txt" && sed 's/^/    broker: /' "$work/peers.txt"
        failed=1
    fi
}

# check_shim NAME: the body of the broker, the shim log and the peer.
check_shim() {
    if test $code -ne 0; then
        result "$1" "exit $code"
    elif ! grep -qx 'podnet-consumer-unix' "$work/$1.txt"; then
        result "$1" "no body of the broker"
    elif ! grep -q 'connect -> forward' "$work/$1.txt"; then
        result "$1" "no forward in the shim log"
    elif ! grep -q 'comm=dotnet$' "$work/peers.txt"; then
        result "$1" "the peer of the AF_UNIX connection is not the dotnet process"
    else
        result "$1" pass
        sed 's/^/    broker: /' "$work/peers.txt"
    fi
}

run direct 0 0
if test $code -eq 0 && grep -qx 'tcp-direct' "$work/direct.txt"; then
    result direct pass
else
    result direct "exit $code, no body of the TCP server"
fi

run shim 0 1
check_shim shim

if test -n "$podlab"; then
    # shellcheck disable=SC2086
    if ! $as_user "$podlab" run "$profile" -- /bin/true >"$work/lab.txt" 2>&1; then
        cat "$work/lab.txt"
        echo "consumer-dotnet: the lab $profile does not hold here" >&2
        exit 1
    fi
    echo "lab $profile as uid $( ($as_user "$podlab" run "$profile" -- id -u) 2>/dev/null)"
    run lab-bare 1 0
    if test $code -eq 0; then
        result lab-bare "the client connected with no shim: the lab does not refuse AF_INET"
    elif ! grep -q 'Permission denied' "$work/lab-bare.txt"; then
        result lab-bare "exit $code, but not EACCES"
    else
        result lab-bare pass
        grep -m 1 'Permission denied' "$work/lab-bare.txt" | sed 's/^/    /'
    fi
    run lab-shim 1 1
    check_shim lab-shim
fi
exit $failed
