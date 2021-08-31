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

	#[test]
	fn wait_size_0_failures_is_zero() {
		let distribution = rand::distributions::Uniform::new_inclusive(0.0, 1.0);
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 0_u32, &mut rng, &distribution),
			Duration::new(0, 0)
		);
	}
}
