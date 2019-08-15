use rand_core::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg as Pcg;

pub fn get_proposer_index(seed: u64, weights: &[u64], weight_sum: u64) -> usize {
    let tmp = u64::max_value() / weight_sum;
    let mut rng = Pcg::seed_from_u64(seed);
    let mut acc = 0u64;
    let mut random_digit = rng.next_u64();
    while random_digit >= weight_sum * tmp {
        random_digit = rng.next_u64();
    }

    for (index, weight) in weights.iter().enumerate() {
        acc += *weight;
        if random_digit < acc * tmp {
            return index;
        }
    }
    0
}
