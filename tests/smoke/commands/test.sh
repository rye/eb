#!/bin/sh

if [ $(( (RANDOM % 10) )) -le 3 ];
then
	echo "$0: Success!"
	exit 0
else
	echo "$0: Try again..."
	exit 1
fi
