#!/bin/sh
# Get the release binary podlab of PodUtils/podcrates (T-358).
#
# Usage: sh scripts/get-podlab.sh VERSION [DIR]
#
# VERSION is a tag of podcrates (v0.58.0 or later: earlier tags have no
# podlab asset). DIR must not exist (default: .work/podlab-VERSION in the
# current directory). The script downloads SHA256SUMS and
# podlab-ARCH-unknown-linux-musl of this machine into DIR, checks the
# asset against SHA256SUMS, and makes it executable. Give DIR to
# scripts/lab-lane.sh as PODLAB. The release of podcrates has no signature
# of its assets: the check is the SHA-256 of SHA256SUMS only. It needs curl
# and sha256sum.

set -eu

if test $# -lt 1 || test $# -gt 2; then
    echo "usage: sh scripts/get-podlab.sh VERSION [DIR]" >&2
    exit 64
fi
version=$1
dir=${2:-$PWD/.work/podlab-$version}
if test -e "$dir"; then
    echo "get-podlab: $dir exists; give a new directory" >&2
    exit 64
fi
asset="podlab-$(uname -m)-unknown-linux-musl"
base="https://github.com/PodUtils/podcrates/releases/download/$version"
mkdir -p "$dir"
for f in SHA256SUMS "$asset"; do
    n=0
    until curl -qfsSL --proto '=https' -o "$dir/$f" "$base/$f"; do
        n=$((n + 1))
        test $n -ge 4 && { echo "get-podlab: cannot download $base/$f" >&2; exit 1; }
        sleep 2
    done
done
(cd "$dir" && grep " $asset\$" SHA256SUMS | sha256sum -c -)
chmod 755 "$dir/$asset"
"$dir/$asset" --version
echo "PODLAB: $dir"
