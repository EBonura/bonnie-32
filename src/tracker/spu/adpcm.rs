//! PS1 SPU ADPCM encoder and decoder
//!
//! The PS1 uses 4-bit ADPCM compression:
//! - 16-byte blocks encode 28 audio samples
//! - 5 prediction filters using previous 2 samples
//! - Shift values 0-12 for dynamic range
//!
//! Decoder: Used at runtime during voice playback
//! Encoder: Used at load time to convert SF2 PCM → ADPCM

use super::tables::{
    ADPCM_FILTER_POS, ADPCM_FILTER_NEG,
    SAMPLES_PER_ADPCM_BLOCK, ADPCM_BLOCK_SIZE,
};
use super::types::{AdpcmBlock, adpcm_flags};

/// Decode one ADPCM block (16 bytes → 28 i16 samples)
///
/// This matches the PS1 hardware decode exactly (Duckstation DecodeBlock):
/// - Extract shift and filter from header byte
/// - For each nibble: sign-extend, apply shift, add filter prediction, clamp to i16
/// - prev1/prev2 are updated in-place for the next block
///
/// The decoded samples are written to `output[0..28]`.
pub fn decode_block(block: &AdpcmBlock, prev1: &mut i16, prev2: &mut i16, output: &mut [i16; 28]) {
    let shift = block.shift().min(12) as i32;
    let filter = block.filter().min(4) as usize;
    let filter_pos = ADPCM_FILTER_POS[filter];
    let filter_neg = ADPCM_FILTER_NEG[filter];

    for i in 0..SAMPLES_PER_ADPCM_BLOCK {
        // Get 4-bit nibble, sign-extend to 16-bit, then shift
        // Hardware does: (s16)(nibble << 12) >> shift
        // The nibble is already sign-extended by get_nibble()
        let nibble = block.get_nibble(i) as i32;
        // Reconstruct: place nibble at bit 12, then arithmetic right shift
        let mut sample = (nibble << 12) >> shift;

        // Apply adaptive prediction filter
        sample += (*prev1 as i32 * filter_pos) >> 6;
        sample += (*prev2 as i32 * filter_neg) >> 6;

        // Clamp to i16 range (hardware saturates)
        let clamped = sample.clamp(-32768, 32767) as i16;

        output[i] = clamped;
        *prev2 = *prev1;
        *prev1 = clamped;
    }
}

/// Decode an ADPCM block from raw bytes in SPU RAM
///
/// Reads 16 bytes starting at `offset` from the given data slice.
pub fn decode_block_from_ram(
    ram: &[u8],
    offset: usize,
    prev1: &mut i16,
    prev2: &mut i16,
    output: &mut [i16; 28],
) {
    let mut bytes = [0u8; 16];
    for i in 0..ADPCM_BLOCK_SIZE {
        bytes[i] = ram[(offset + i) % ram.len()];
    }
    let block = AdpcmBlock::from_bytes(&bytes);
    decode_block(&block, prev1, prev2, output);
}

// =============================================================================
// ADPCM Encoder — converts PCM i16 samples to ADPCM blocks
// =============================================================================

/// Encode PCM samples to ADPCM blocks
///
/// Processes groups of 28 samples at a time, trying all 5 filter × 13 shift
/// combinations to find the best encoding for each block (minimum squared error).
///
/// Sets loop flags on appropriate blocks if loop points are provided.
/// Loop start must be block-aligned (multiple of 28 samples).
///
/// Returns the encoded ADPCM data as raw bytes (16 bytes per block).
pub fn encode_pcm_to_adpcm(
    samples: &[i16],
    loop_start_sample: Option<usize>,
    loop_end_sample: Option<usize>,
) -> Vec<u8> {
    if samples.is_empty() {
        // Return a single silent end block
        let mut block = [0u8; 16];
        block[1] = adpcm_flags::LOOP_END;
        return block.to_vec();
    }

    // Pad samples to multiple of 28
    let padded_len = ((samples.len() + SAMPLES_PER_ADPCM_BLOCK - 1)
        / SAMPLES_PER_ADPCM_BLOCK)
        * SAMPLES_PER_ADPCM_BLOCK;
    let mut padded = vec![0i16; padded_len];
    padded[..samples.len()].copy_from_slice(samples);

    let num_blocks = padded_len / SAMPLES_PER_ADPCM_BLOCK;

    // Convert loop points from sample indices to block indices
    let loop_start_block = loop_start_sample.map(|s| s / SAMPLES_PER_ADPCM_BLOCK);
    let loop_end_block = loop_end_sample
        .map(|s| (s + SAMPLES_PER_ADPCM_BLOCK - 1) / SAMPLES_PER_ADPCM_BLOCK)
        .unwrap_or(num_blocks);

    let mut result = Vec::with_capacity(num_blocks * ADPCM_BLOCK_SIZE);
    let mut prev1: i16 = 0;
    let mut prev2: i16 = 0;

    for block_idx in 0..num_blocks {
        let sample_offset = block_idx * SAMPLES_PER_ADPCM_BLOCK;
        let block_samples = &padded[sample_offset..sample_offset + SAMPLES_PER_ADPCM_BLOCK];

        // Find best filter + shift combination
        let (best_filter, best_shift, encoded_nibbles) =
            find_best_encoding(block_samples, prev1, prev2);

        // Build the ADPCM block
        let mut block_bytes = [0u8; 16];
        block_bytes[0] = best_shift | (best_filter << 4);

        // Set flags
        let mut flags = 0u8;
        if let Some(ls) = loop_start_block {
            if block_idx == ls {
                flags |= adpcm_flags::LOOP_START;
            }
        }
        let is_last = block_idx == num_blocks - 1;
        let is_loop_end = block_idx + 1 == loop_end_block;
        if is_last || is_loop_end {
            flags |= adpcm_flags::LOOP_END;
            if loop_start_block.is_some() {
                flags |= adpcm_flags::LOOP_REPEAT;
            }
        }
        block_bytes[1] = flags;

        // Pack nibbles into bytes (low nibble first)
        for i in 0..14 {
            let lo = encoded_nibbles[i * 2] & 0x0F;
            let hi = encoded_nibbles[i * 2 + 1] & 0x0F;
            block_bytes[2 + i] = lo | (hi << 4);
        }

        // Update prev1/prev2 by decoding what we just encoded
        // (the decoder state must match for the next block's filter prediction)
        let block = AdpcmBlock::from_bytes(&block_bytes);
        let mut decoded = [0i16; 28];
        decode_block(&block, &mut prev1, &mut prev2, &mut decoded);

        result.extend_from_slice(&block_bytes);
    }

    result
}

/// Find the best filter + shift for a group of 28 samples
///
/// Tries all 5 filters × 13 shift values and picks the combination
/// with the lowest total squared error.
///
/// Returns (filter, shift, nibbles[28])
fn find_best_encoding(
    samples: &[i16],
    prev1: i16,
    prev2: i16,
) -> (u8, u8, [u8; 28]) {
    let mut best_error: i64 = i64::MAX;
    let mut best_filter: u8 = 0;
    let mut best_shift: u8 = 0;
    let mut best_nibbles = [0u8; 28];

    for filter in 0..5u8 {
        let filter_pos = ADPCM_FILTER_POS[filter as usize];
        let filter_neg = ADPCM_FILTER_NEG[filter as usize];

        for shift in 0..13u8 {
            let mut total_error: i64 = 0;
            let mut p1 = prev1 as i32;
            let mut p2 = prev2 as i32;
            let mut nibbles = [0u8; 28];

            for i in 0..SAMPLES_PER_ADPCM_BLOCK {
                let target = samples[i] as i32;

                // Calculate prediction
                let prediction = (p1 * filter_pos) >> 6;
                let prediction = prediction + ((p2 * filter_neg) >> 6);

                // Calculate the residual we need to encode
                let residual = target - prediction;

                // Quantize: the decoder does (nibble << 12) >> shift
                // So to encode: nibble = (residual << shift) >> 12
                // But we need to round to nearest
                let scaled = residual << shift;
                let mut nibble = (scaled + (1 << 11)) >> 12; // round to nearest

                // Clamp to 4-bit signed range (-8 to 7)
                nibble = nibble.clamp(-8, 7);
                nibbles[i] = (nibble & 0x0F) as u8;

                // Decode what we actually encoded (for error calculation and state update)
                let decoded_residual = (nibble << 12) >> shift;
                let decoded = (prediction + decoded_residual).clamp(-32768, 32767);

                let err = (target - decoded) as i64;
                total_error += err * err;

                p2 = p1;
                p1 = decoded;
            }

            if total_error < best_error {
                best_error = total_error;
                best_filter = filter;
                best_shift = shift;
                best_nibbles = nibbles;
            }
        }
    }

    (best_filter, best_shift, best_nibbles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_silence() {
        // A silent ADPCM block (all zeros)
        let block = AdpcmBlock::default();
        let mut prev1: i16 = 0;
        let mut prev2: i16 = 0;
        let mut output = [0i16; 28];

        decode_block(&block, &mut prev1, &mut prev2, &mut output);

        for s in &output {
            assert_eq!(*s, 0);
        }
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // Generate a simple sine wave
        let samples: Vec<i16> = (0..28 * 4)
            .map(|i| ((i as f64 * 0.1).sin() * 16000.0) as i16)
            .collect();

        // Encode
        let encoded = encode_pcm_to_adpcm(&samples, None, None);

        // Decode all blocks
        let mut prev1: i16 = 0;
        let mut prev2: i16 = 0;
        let mut decoded = Vec::new();
        let num_blocks = encoded.len() / 16;

        for b in 0..num_blocks {
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&encoded[b * 16..(b + 1) * 16]);
            let block = AdpcmBlock::from_bytes(&bytes);
            let mut output = [0i16; 28];
            decode_block(&block, &mut prev1, &mut prev2, &mut output);
            decoded.extend_from_slice(&output);
        }

        // Check that decoded is close to original (ADPCM is lossy but should be close)
        let mut max_error: i32 = 0;
        for i in 0..samples.len() {
            let err = (samples[i] as i32 - decoded[i] as i32).abs();
            max_error = max_error.max(err);
        }
        // With good encoding, max error should be well under 10% of full range
        assert!(
            max_error < 3000,
            "Max error {} too large for ADPCM roundtrip",
            max_error
        );
    }

    #[test]
    fn test_encode_sets_loop_flags() {
        let samples = vec![0i16; 28 * 4]; // 4 blocks
        let encoded = encode_pcm_to_adpcm(&samples, Some(28), None);

        // Block 1 should have LOOP_START
        assert_ne!(encoded[1 * 16 + 1] & adpcm_flags::LOOP_START, 0);
        // Last block should have LOOP_END and LOOP_REPEAT
        let last_block_flags = encoded[3 * 16 + 1];
        assert_ne!(last_block_flags & adpcm_flags::LOOP_END, 0);
        assert_ne!(last_block_flags & adpcm_flags::LOOP_REPEAT, 0);
    }

    #[test]
    fn test_nibble_extraction() {
        let mut block = AdpcmBlock::default();
        // Set byte 0 of data to 0xAB → low nibble = 0xB, high nibble = 0xA
        block.data[0] = 0xAB;

        let n0 = block.get_nibble(0); // low nibble 0xB → sign-extended = -5
        let n1 = block.get_nibble(1); // high nibble 0xA → sign-extended = -6

        assert_eq!(n0, -5);
        assert_eq!(n1, -6);
    }
}
