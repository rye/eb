#![no_std]

use core::{cmp::PartialOrd, time::Duration};

// This function is a basic implementation of the generic from the
// `core::cmp::Ord` clamp function, which is unstable.  This code is stable,
// but eventually you might want to change this to the `.clamp` method on
// ord-y things.
pub fn clamp<T>(value: T, min: T, max: T) -> T
where
	T: PartialOrd,
{
	assert!(min <= max);

	if value < min {
		min
	} else if value > max {
		max
	} else {
		value
	}
}

pub enum SlotTime {
	UserSpecified(Duration),
	AutoGenerated(Duration),
}

#[cfg(test)]
mod tests {
	use super::*;

	mod clamp {
		use super::clamp;

		mod i32 {
			use super::clamp;

			#[test]
			fn inside_range() {
				assert_eq!(clamp(1_i32, 0_i32, 2_i32), 1_i32);
			}

			#[test]
			fn below_range() {
				assert_eq!(clamp(-1_i32, 0_i32, 2_i32), 0_i32);
			}

			#[test]
			fn above_range() {
				assert_eq!(clamp(3_i32, 0_i32, 2_i32), 2_i32);
			}
		}

		#[test]
		#[should_panic]
		fn panics_if_not_ordered_properly() {
			clamp(1_i32, 2_i32, 0_i32);
		}
	}
}
