#!/bin/sh
# A consumer as a test (T-358 step 3): podnet pod around a dynamic program.
#
# Usage: sh scripts/consumer-pod.sh PODNET PODLAB [PROFILE...]
#
# PODNET is the podnet binary to test. PODLAB is the binary podlab of
# PodUtils/podcrates (crate pod-lab, feature cli). With no PROFILE, the
# script runs in bind-denied, no-net-admin, no-tun and udp-denied. As root,
# each lab case runs as uid 65534 (setpriv).
#
# Outside the lab: a SOCKS5 server on a unix socket (the upstream of the
# pod), the server on 127.0.0.1:PORT+1 with the body podnet-consumer-pod,
# and a decoy on 127.0.0.1:PORT with the body tcp-direct. The SOCKS5
# server connects only consumer.test:PORT, to the server on PORT+1; it
# refuses each other target. A program (python3 with urllib, and curl when
# it exists; each is dynamic) sends GET to http://consumer.test:PORT/ and
# exits 42 when it got the body. The rows:
#   control           no lab, no pod: the program gets the decoy at
#                     127.0.0.1:PORT (the control of the decoy).
#   PROFILE bare      the lab, no pod: the program sends GET to the decoy.
#                     In bind-denied it must fail with EACCES (the proof
#                     that the lab refuses AF_INET); in another lab the
#                     row only says what the lab does.
#   PROFILE BACKEND   the lab, podnet pod --backend BACKEND -x
#                     socks5://unix:PATH -- PROGRAM: pass when the pod exits
#                     42 (the exit of the program), the program printed
#                     the body of the server, and the SOCKS5 server got
#                     the CONNECT of consumer.test:PORT. n.a. only when
#                     podnet exits 3 with a reason, in a lab that takes
#                     what the backend needs by design. A failure with an
#                     entry in the record says "fail, known: T-NNN". Else
#                     fail.
# BACKEND is auto, netns, seccomp and netns-kernel.
#
# Environment: CONSUMER_DIR (default .work/consumer-pod in the current
# directory; not in /tmp or HOME, which a lab can hide), CONSUMER_PORT
# (default 7108), CONSUMER_PLANT (decoy: the SOCKS5 server sends the
# program to the decoy; each pod row must fail). It needs python3 and timeout. It exits 0 when no row
# fails, else 1, and prints the output of a failed row.

set -u

if test $# -lt 2; then
    echo "usage: sh scripts/consumer-pod.sh PODNET PODLAB [PROFILE...]" >&2
    exit 64
fi

abs() { (cd "$(dirname "$1")" && printf '%s/%s\n' "$(pwd)" "$(basename "$1")"); }
podnet=$(abs "$1")
podlab=$(abs "$2")
shift 2
profiles=${*:-bind-denied no-net-admin no-tun udp-denied}
root=$(cd "$(dirname "$0")/.." && pwd)
port=${CONSUMER_PORT:-7108}
sport=$((port + 1))
work=${CONSUMER_DIR:-$PWD/.work/consumer-pod}
BACKENDS="auto netns seccomp netns-kernel"

rm -rf "$work"
mkdir -p "$work/run" || exit 1
work=$(cd "$work" && pwd)
case $work in
/tmp | /tmp/*) echo "consumer-pod: CONSUMER_DIR $work is in /tmp; a lab hides it" >&2; exit 64 ;;
esac
cp "$root/tests/consumers/python-http/client.py" "$root/tests/consumers/servers.py" "$work/" || exit 1
chmod -R a+rX "$work" && chmod 777 "$work/run" || exit 1

programs=python
command -v curl >/dev/null && programs="python curl"
echo "podnet $("$podnet" --version)"
echo "programs: $programs"

ssock="$work/socks.sock"
# CONSUMER_PLANT=decoy: the SOCKS5 server sends consumer.test to the decoy
# (a planted defect: each pod row must fail).
target=$sport
test "${CONSUMER_PLANT:-}" = decoy && target=$port
python3 -I "$work/servers.py" socks-unix "$ssock" "$work/socks.txt" "consumer.test:$port=$target" \
    >"$work/socks.out" 2>&1 &
pids=$!
python3 -I "$work/servers.py" http-tcp "$sport" podnet-consumer-pod >"$work/server.out" 2>&1 &
pids="$pids $!"
python3 -I "$work/servers.py" http-tcp "$port" tcp-direct >"$work/decoy.out" 2>&1 &
pids="$pids $!"
# $pids is a list of numbers: split it.
# shellcheck disable=SC2086
trap 'kill $pids 2>/dev/null' EXIT
i=0
while ! test -S "$ssock"; do
    i=$((i + 1))
    test $i -gt 100 && { cat "$work/socks.out"; echo "consumer-pod: the SOCKS5 server did not start" >&2; exit 1; }
    sleep 0.1
done
sleep 0.3

as_user=
test "$(id -u)" = 0 && as_user="setpriv --reuid 65534 --regid 65534 --clear-groups"

# The program: GET of URL, exit 42 when the body came.
# The single quotes are the text of the program: sh -c expands them.
# shellcheck disable=SC2016
program() {
    case $1 in
    python) set -- sh -c 'python3 -I "$0" urllib "$1" && exit 42' "$work/client.py" "$2" ;;
    curl) set -- sh -c 'curl -v -sS --max-time 20 --noproxy "*" "$0" && echo && exit 42' "$2" ;;
    esac
    printf '%s\n' "$@"
}

# The profiles that take what a backend needs, by design. It sets why.
refusal_expected() {
    case $1:$2 in
    netns:userns-off | netns:nonroot-nocaps | netns:seccomp-nonewprivs | netns-kernel:userns-off | \
        netns-kernel:nonroot-nocaps | netns-kernel:seccomp-nonewprivs)
        why="no user namespace"
        ;;
    netns:bind-denied | netns-kernel:bind-denied)
        # Landlock refuses each TCP connect of the program, in each network
        # namespace: the program cannot reach the stack of the pod.
        why="the lab refuses each TCP connect of the program (Landlock), in each namespace"
        ;;
    *:no-proc | *:proc-masked)
        # The id maps of netns and netns-kernel, and /proc/PID/mem of
        # seccomp: each backend needs /proc, so auto has none.
        why="no /proc"
        ;;
    netns:raw-denied)
        # The veth wire needs AF_PACKET; with /dev/net/tun the TUN works.
        # $as_user is empty or a command and its options: split it.
        # shellcheck disable=SC2086
        $as_user sh -c 'test -r /dev/net/tun && test -w /dev/net/tun' && return 1
        why="no /dev/net/tun for this user, and no AF_PACKET for the veth wire"
        ;;
    netns-kernel:listen-denied)
        why="the backend serves each flow from a TCP listener in the namespace"
        ;;
    netns-kernel:udp-denied)
        why="the lab refuses the UDP socket that the backend makes"
        ;;
    netns:no-net-admin | netns-kernel:no-net-admin)
        test "$(id -u)" = 0 || return 1
        why="uid 0 with no capability cannot write its id map"
        ;;
    *) return 1 ;;
    esac
}

# The failures that have an entry in the record. It sets entry.
known() {
    case $1:$2 in
    netns-kernel:getsockname-denied) entry="T-384: the backend reads the ports of its listeners with getsockname" ;;
    auto:udp-denied | seccomp:udp-denied | netns:udp-denied) entry="T-383: no name resolves when the program has no UDP socket" ;;
    *) return 1 ;;
    esac
}

failed=0
row() {
    printf '%-14s %-8s %-14s %s\n' "$1" "$2" "$3" "$4"
}

# run FILE CMD...: the command with its output to FILE; the code to $code.
run() {
    out=$1
    shift
    "$@" >"$out" 2>&1
    code=$?
}

# A program as a list of lines (no word in it has a newline).
for prog in $programs; do
    old_ifs=$IFS
    IFS='
'
    # shellcheck disable=SC2046
    set -- $(program "$prog" "http://127.0.0.1:$port/")
    IFS=$old_ifs
    run "$work/run/control.$prog.txt" timeout 60 "$@"
    if test $code -eq 42 && grep -qx tcp-direct "$work/run/control.$prog.txt"; then
        row - "$prog" control pass
    else
        row - "$prog" control "fail (exit $code, $work/run/control.$prog.txt)"
        failed=1
    fi
done

for profile in $profiles; do
    # shellcheck disable=SC2086
    if ! $as_user "$podlab" run "$profile" -- /bin/true >"$work/run/$profile.lab.txt" 2>&1; then
        row "$profile" '*' '*' "n.a.: the lab does not hold here ($work/run/$profile.lab.txt)"
        continue
    fi
    for prog in $programs; do
        old_ifs=$IFS
        IFS='
'
        # shellcheck disable=SC2046
        set -- $(program "$prog" "http://127.0.0.1:$port/")
        IFS=$old_ifs
        f="$work/run/$profile.$prog.bare.txt"
        # shellcheck disable=SC2086
        run "$f" timeout 60 $as_user "$podlab" run "$profile" -- "$@"
        if test "$profile" = bind-denied; then
            if test $code -ne 42 && grep -q -i 'permission denied' "$f"; then
                row "$profile" "$prog" bare "pass: refused ($(grep -m 1 -i -o '[^:]*permission denied.*' "$f" | head -c 80))"
            else
                row "$profile" "$prog" bare "fail: the lab did not refuse (exit $code, $f)"
                failed=1
            fi
        elif test $code -eq 42; then
            row "$profile" "$prog" bare "info: the lab lets the program reach the decoy"
        else
            row "$profile" "$prog" bare "info: exit $code with no pod ($f)"
        fi
        old_ifs=$IFS
        IFS='
'
        # shellcheck disable=SC2046
        set -- $(program "$prog" "http://consumer.test:$port/")
        IFS=$old_ifs
        for backend in $BACKENDS; do
            f="$work/run/$profile.$prog.$backend.txt"
            : >"$work/socks.txt"
            # shellcheck disable=SC2086
            run "$f" timeout 120 $as_user "$podlab" run "$profile" -- env HOME="$work/run" \
                "$podnet" pod --backend "$backend" --name-backend -x "socks5://unix:$ssock" -- "$@"
            if test $code -eq 42 && grep -qx podnet-consumer-pod "$f" &&
                grep -q "^connect consumer.test:$port " "$work/socks.txt"; then
                row "$profile" "$prog" "$backend" pass
            elif test $code -eq 3 && refusal_expected "$backend" "$profile"; then
                row "$profile" "$prog" "$backend" "n.a.: exit 3; $why: $(grep '^podnet pod: ' "$f" | grep -v 'Choice: \|does not get' | head -n 1 | cut -c 1-200)"
            elif known "$backend" "$profile"; then
                row "$profile" "$prog" "$backend" "fail, known: $entry (exit $code, $f)"
                failed=1
            else
                row "$profile" "$prog" "$backend" "fail (exit $code, $f)"
                failed=1
            fi
        done
    done
done
exit $failed
