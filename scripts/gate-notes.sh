#!/bin/sh
# The section of the release gate in the notes of a release (T-358 step 4).
#
# Usage: sh scripts/gate-notes.sh LANE.TXT [RUN-URL]
#
# LANE.TXT is the output of scripts/lab-lane.sh (gate.yml keeps it as the
# artifact gate). The script prints Markdown: the counts of the rows that
# pass, fail and are n.a., then each row (the profile, the command and
# the result) in a block that opens on a click. RUN-URL names the run of
# the gate. It exits 1 when the file has no row or a row fails: a release
# has no failed row of the gate.

set -eu

if test $# -lt 1 || test $# -gt 2; then
    echo "usage: sh scripts/gate-notes.sh LANE.TXT [RUN-URL]" >&2
    exit 64
fi
lane=$1
url=${2:-}

rows=$(awk '$1 != "PROFILE" && NF >= 3 && ($3 == "pass" || $3 ~ /^fail/ || $3 ~ /^n\.a\./)' "$lane")
if test -z "$rows"; then
    echo "gate-notes: $lane has no row of the lane" >&2
    exit 1
fi
pass=$(printf '%s\n' "$rows" | awk '$3 == "pass"' | wc -l | tr -d ' ')
fail=$(printf '%s\n' "$rows" | awk '$3 ~ /^fail/' | wc -l | tr -d ' ')
na=$(printf '%s\n' "$rows" | awk '$3 ~ /^n\.a\./' | wc -l | tr -d ' ')

printf '## The release gate\n\n'
printf 'The static binary of this release ran the lane of `scripts/lab-lane.sh` in the labs of pod-lab, as a user that is\n'
printf 'not root (`gate.yml`, `docs/development.md`): %s rows pass, %s fail, %s are n.a. (a lab that does not hold on the\n' "$pass" "$fail" "$na"
printf 'runner, or a command that the lab takes away by design, with the reason).'
if test -n "$url"; then
    printf ' The run: %s.' "$url"
fi
printf '\n\n<details><summary>Each row</summary>\n\n'
printf '| Profile | Command | Result |\n| --- | --- | --- |\n'
printf '%s\n' "$rows" | awk '{ r = $3; for (i = 4; i <= NF; i++) r = r " " $i; gsub(/\|/, "\\|", r); printf "| %s | %s | %s |\n", $1, $2, r }'
printf '\n</details>\n'

if test "$fail" != 0; then
    echo "gate-notes: $fail rows of the lane fail" >&2
    exit 1
fi
