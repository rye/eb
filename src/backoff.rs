use core::time::Duration;

pub fn wait_size<D: rand::distributions::Distribution<f32>, R: rand::Rng>(
	_slot_time: &Duration,
	_attempts_so_far: u32,
	_rng: &mut R,
	_distribution: &D,
) -> Duration {
	Duration::new(0, 0)
}

#[cfg(test)]
mod tests {
	use super::*;

	mod test_distribution {
		pub(super) struct Always1 {}

		impl rand::distributions::Distribution<f32> for Always1 {
			fn sample<R: rand::Rng + ?Sized>(&self, _: &mut R) -> f32 {
				1.0
			}
		}
	}

	#[test]
	fn attempts_0_wait_size_0() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 0_u32, &mut rng, &distribution),
			Duration::new(0, 0)
		);
	}

	#[test]
	fn attempts_1_wait_size_1() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 1_u32, &mut rng, &distribution),
			Duration::new(1, 0)
		);
	}

	#[test]
	fn attempts_2_wait_size_3() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 2_u32, &mut rng, &distribution),
			Duration::new(3, 0)
		);
	}

	#[test]
	fn attempts_3_wait_size_7() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 3_u32, &mut rng, &distribution),
			Duration::new(7, 0)
		);
	}
}
