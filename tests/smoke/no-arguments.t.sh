#!/bin/sh

dir="$(dirname "$0")/../.."
cd "$dir"

output=$(cargo run --quiet --release -- 2>&1)

if echo "$output" | tail -n 1 | grep -q "Error: no command given";
then
	echo "$0: passed"
	exit 0
else
	echo "$0: failed: output = $output"
	exit 1
fi

