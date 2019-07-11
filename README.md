# eb

`eb` is a small command-line application that tries its given command-line arguments with an exponentially-increasing timeout as non-zero status codes are returned.
You might use this when, for example, you have a program that exits if something fails.

`eb` only exits once a status code of `0` has been returned.

You can use `eb` like `watch`:

```console
$ eb -- nc -z 192.0.2.42 53
```

## The algorithm

In the above console input, `eb` will watch the command and keep track of how long it takes to fail.
Upon the first failure, a "slot time" is set to the time it took for the command to fail.

In each instance, after `n` failures, a random number of slot times between `0` and `2^n - 1` is chosen, and these slot times are delayed through.
So, after the first failure, `eb` will either wait `0` or `1` slot times; after the second failure this number increases to between `0` and `3`.
The exponent `n` is `clamp`ed to be within the range `0` and `MAX_N` where `MAX_N` is some predefined number; in our case this is `16`.
In practice, this means that after `16` collisions, the possible upper bound on delay does not increase.
Because commands take a variable amount of time to complete, the slot time is adjusted to be the _average_ of all execution times.
The delay will start very quickly after the command fails.

Because of the way the algorithm works, it is recommended to use `eb` in situations where the given command will fail quickly.
This will result in a proportionally small maximum delay time.

## License

eb: A command executor exercising exponential backoff
Copyright (C) 2019 Kristofer J. Rye

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
