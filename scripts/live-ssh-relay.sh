#!/bin/sh
# A unix socket through a public SSH relay (T-398): an HTTP origin in Python
# listens on a unix socket, `podnet ssh-relay --json` publishes it, and curl
# must get the body of the origin through the URL of the relay. The relays
# are services of other parties: the script tries each one of the list in
# order and passes on the first that serves the origin. The URL host is
# masked before any output.
#
#   sh scripts/live-ssh-relay.sh PODNET [SERVICE...]
set -eu
podnet=$1
shift
[ "$#" -gt 0 ] || set -- localhost-run pinggy serveo tunnl-gg
dir=$(mktemp -d)
pids=""
trap 'kill $pids 2> /dev/null || true' EXIT

python3 -I -c '
import socketserver, sys
class Handler(socketserver.StreamRequestHandler):
    def handle(self):
        while self.rfile.readline() not in (b"\r\n", b"\n", b""):
            pass
        body = b"ssh-relay-proof\n"
        self.wfile.write(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: %d\r\n"
                         b"Connection: close\r\n\r\n" % len(body) + body)
socketserver.ThreadingUnixStreamServer(sys.argv[1], Handler).serve_forever()
' "$dir/origin.sock" &
pids="$pids $!"
i=0
until [ -S "$dir/origin.sock" ]; do
    i=$((i + 1))
    [ "$i" -lt 20 ] || { echo "the origin did not start"; exit 1; }
    sleep 0.5
done

# One relay: 0 when curl gets the body of the origin through its URL.
try() {
    service=$1
    out="$dir/$service.out"
    "$podnet" ssh-relay --service "$service" --socket "$dir/origin.sock" --json \
        > "$out" 2> "$dir/$service.err" &
    pid=$!
    pids="$pids $pid"
    i=0
    until grep -q '"event":"ready"' "$out" 2> /dev/null; do
        i=$((i + 1))
        if [ "$i" -ge 60 ] || ! kill -0 "$pid" 2> /dev/null; then
            echo "$service: no ready line in 60 s"
            sed 's/https:\/\/[^ "]*/https:\/\/MASKED/g' "$dir/$service.err" | tail -5
            kill "$pid" 2> /dev/null || true
            return 1
        fi
        sleep 1
    done
    url=$(grep '"event":"ready"' "$out" | head -n 1 | jq -r .url)
    host=${url#*://}
    host=${host%%/*}
    echo "::add-mask::$host"
    echo "$service: ready"
    i=0
    while [ "$i" -lt 12 ]; do
        i=$((i + 1))
        if body=$(curl -fsS --max-time 15 "$url/" 2> /dev/null) && [ "$body" = "ssh-relay-proof" ]; then
            echo "$service: the origin answered through the relay (try $i)"
            kill "$pid" 2> /dev/null || true
            return 0
        fi
        sleep 5
    done
    echo "$service: the URL did not give the body of the origin"
    kill "$pid" 2> /dev/null || true
    return 1
}

for service in "$@"; do
    if try "$service"; then
        exit 0
    fi
done
echo "no SSH relay served the origin: $*"
exit 1
