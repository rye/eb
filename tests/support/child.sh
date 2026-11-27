#!/bin/sh
# Only shell builtins: negligible runtime, no random outcomes or external tools.
set -eu
count_file=$1
arguments_file=$2
mode=$3
shift 3
IFS= read -r count < "$count_file"
count=$((count + 1))
printf '%s\n' "$count" > "$count_file"
: > "$arguments_file"
for argument do
	printf '%s\n' "$argument" >> "$arguments_file"
done
printf 'attempt %s\n' "$count"
case "$mode" in
	signal) kill -TERM "$$" ;;
	never) exit 7 ;;
	*) [ "$count" -ge "$mode" ] ;;
esac
