#!/bin/sh

dir="$(dirname "$0")/../.."
cd "$dir"

output=$(cargo run --quiet --release -- "$dir/tests/smoke/commands/test.sh" 2>&1)

echo "$0: Command completed, $(echo "$output" | wc -l) lines of output"

if echo "$output" | tail -n 1 | grep -q ": Success";
then
	echo "$0: passed"
	exit 0
else
	echo "$0: failed: output = $output"
	exit 1
fi

