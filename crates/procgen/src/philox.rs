//! Philox4x32-10 constants and round function from the Random123 reference.

const MULTIPLIER_0: u32 = 0xD251_1F53;
const MULTIPLIER_1: u32 = 0xCD9E_8D57;
const WEYL_0: u32 = 0x9E37_79B9;
const WEYL_1: u32 = 0xBB67_AE85;

/// Maps a 64-bit key and 128-bit counter to four deterministic 32-bit words.
///
/// The counter is interpreted in the supplied word order. All operations are
/// wrapping integer arithmetic, so output does not depend on platform floats.
pub fn philox4x32_10(mut key: [u32; 2], mut counter: [u32; 4]) -> [u32; 4] {
    for round in 0..10 {
        let product_0 = u64::from(MULTIPLIER_0) * u64::from(counter[0]);
        let product_1 = u64::from(MULTIPLIER_1) * u64::from(counter[2]);
        counter = [
            (product_1 >> 32) as u32 ^ counter[1] ^ key[0],
            product_1 as u32,
            (product_0 >> 32) as u32 ^ counter[3] ^ key[1],
            product_0 as u32,
        ];
        if round != 9 {
            key[0] = key[0].wrapping_add(WEYL_0);
            key[1] = key[1].wrapping_add(WEYL_1);
        }
    }
    counter
}
