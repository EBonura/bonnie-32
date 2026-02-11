//! Comprehensive SPU pipeline test suite
//!
//! Tests every stage of the audio pipeline independently and writes WAV files
//! to /tmp/spu_tests/ for manual inspection. Run with:
//!
//!   cargo test --release spu_pipeline -- --nocapture
//!
//! Each test isolates one pipeline component and produces diagnostic output.

#[cfg(test)]
mod spu_pipeline {
    use crate::tracker::spu::adpcm;
    use crate::tracker::spu::tables::*;
    use crate::tracker::spu::types::*;
    use crate::tracker::spu::voice::Voice;
    use crate::tracker::spu::SpuCore;
    use std::f64::consts::PI;
    use std::fs;
    use std::path::Path;

    const OUT_DIR: &str = "/tmp/spu_tests";

    // =========================================================================
    // WAV writer — minimal, no dependencies
    // =========================================================================

    fn write_wav_mono(path: &str, samples: &[i16], sample_rate: u32) {
        let dir = Path::new(path).parent().unwrap();
        fs::create_dir_all(dir).ok();

        let data_len = (samples.len() * 2) as u32;
        let file_len = 36 + data_len;
        let mut buf = Vec::with_capacity(file_len as usize + 8);

        // RIFF header
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_len.to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        // fmt chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        buf.extend_from_slice(&2u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // data chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_len.to_le_bytes());
        for &s in samples {
            buf.extend_from_slice(&s.to_le_bytes());
        }

        fs::write(path, &buf).expect("Failed to write WAV");
    }

    fn write_wav_stereo(path: &str, left: &[i16], right: &[i16], sample_rate: u32) {
        assert_eq!(left.len(), right.len());
        let dir = Path::new(path).parent().unwrap();
        fs::create_dir_all(dir).ok();

        let num_samples = left.len();
        let data_len = (num_samples * 4) as u32; // 2 channels * 2 bytes
        let file_len = 36 + data_len;
        let mut buf = Vec::with_capacity(file_len as usize + 8);

        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_len.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&2u16.to_le_bytes()); // stereo
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&(sample_rate * 4).to_le_bytes()); // byte rate
        buf.extend_from_slice(&4u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..num_samples {
            buf.extend_from_slice(&left[i].to_le_bytes());
            buf.extend_from_slice(&right[i].to_le_bytes());
        }

        fs::write(path, &buf).expect("Failed to write WAV");
    }

    /// Generate a sine wave at the given frequency, returning i16 samples
    fn generate_sine(freq_hz: f64, duration_secs: f64, sample_rate: u32, amplitude: f64) -> Vec<i16> {
        let num_samples = (duration_secs * sample_rate as f64) as usize;
        (0..num_samples)
            .map(|i| {
                let t = i as f64 / sample_rate as f64;
                (amplitude * (2.0 * PI * freq_hz * t).sin()) as i16
            })
            .collect()
    }

    /// Store PCM samples in SPU RAM as ADPCM, return (offset, length, loop_offset)
    fn store_in_spu_ram(
        spu_ram: &mut SpuRam,
        pcm: &[i16],
        loop_start: Option<usize>,
        loop_end: Option<usize>,
    ) -> (u32, u32, u32, bool) {
        let adpcm = adpcm::encode_pcm_to_adpcm(pcm, loop_start, loop_end);
        let offset = spu_ram.allocate(&adpcm).expect("SPU RAM full");
        let has_loop = loop_start.is_some();
        let loop_off = if has_loop {
            let loop_block = loop_start.unwrap_or(0) / SAMPLES_PER_ADPCM_BLOCK;
            offset + (loop_block as u32 * ADPCM_BLOCK_SIZE as u32)
        } else {
            offset
        };
        (offset, adpcm.len() as u32, loop_off, has_loop)
    }

    /// Measure the dominant frequency in a signal using zero-crossing analysis
    fn measure_frequency(samples: &[i16], sample_rate: u32) -> f64 {
        // Count zero crossings (negative → positive)
        let mut crossings = Vec::new();
        for i in 1..samples.len() {
            if samples[i - 1] <= 0 && samples[i] > 0 {
                // Linear interpolation for sub-sample accuracy
                let frac = -samples[i - 1] as f64 / (samples[i] as f64 - samples[i - 1] as f64);
                crossings.push(i as f64 - 1.0 + frac);
            }
        }
        if crossings.len() < 2 {
            return 0.0;
        }
        // Average period from consecutive crossings
        let mut total_period = 0.0;
        for i in 1..crossings.len() {
            total_period += crossings[i] - crossings[i - 1];
        }
        let avg_period = total_period / (crossings.len() - 1) as f64;
        sample_rate as f64 / avg_period
    }

    /// Calculate RMS of a signal
    fn rms(samples: &[i16]) -> f64 {
        let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum / samples.len().max(1) as f64).sqrt()
    }

    /// Calculate SNR in dB between original and error
    fn snr_db(original_rms: f64, error_rms: f64) -> f64 {
        if error_rms == 0.0 {
            return 999.0;
        }
        20.0 * (original_rms / error_rms).log10()
    }

    // =========================================================================
    // TEST 1: ADPCM Encode/Decode Roundtrip
    // =========================================================================

    #[test]
    fn spu_pipeline_01_adpcm_roundtrip() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 1: ADPCM Encode/Decode Roundtrip");
        eprintln!("{}\n", "=".repeat(70));

        let test_signals = [
            ("silence", generate_sine(0.0, 0.1, 44100, 0.0)),
            ("440hz_low", generate_sine(440.0, 0.5, 44100, 4000.0)),
            ("440hz_mid", generate_sine(440.0, 0.5, 44100, 16000.0)),
            ("440hz_full", generate_sine(440.0, 0.5, 44100, 30000.0)),
            ("100hz", generate_sine(100.0, 0.5, 44100, 16000.0)),
            ("1000hz", generate_sine(1000.0, 0.5, 44100, 16000.0)),
            ("4000hz", generate_sine(4000.0, 0.5, 44100, 16000.0)),
            ("10000hz", generate_sine(10000.0, 0.5, 44100, 16000.0)),
            // Complex signal: two mixed sines
            ("mix_440_880", {
                let a = generate_sine(440.0, 0.5, 44100, 10000.0);
                let b = generate_sine(880.0, 0.5, 44100, 8000.0);
                a.iter().zip(b.iter()).map(|(&x, &y)| x.saturating_add(y)).collect()
            }),
        ];

        for (name, pcm) in &test_signals {
            let encoded = adpcm::encode_pcm_to_adpcm(pcm, None, None);
            let num_blocks = encoded.len() / 16;

            // Decode
            let mut prev1: i16 = 0;
            let mut prev2: i16 = 0;
            let mut decoded = Vec::new();
            for b in 0..num_blocks {
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&encoded[b * 16..(b + 1) * 16]);
                let block = AdpcmBlock::from_bytes(&bytes);
                let mut output = [0i16; 28];
                adpcm::decode_block(&block, &mut prev1, &mut prev2, &mut output);
                decoded.extend_from_slice(&output);
            }

            // Calculate error
            let mut max_err: i32 = 0;
            let mut errors: Vec<i16> = Vec::new();
            for i in 0..pcm.len().min(decoded.len()) {
                let err = pcm[i] as i32 - decoded[i] as i32;
                max_err = max_err.max(err.abs());
                errors.push(err.clamp(-32768, 32767) as i16);
            }
            let orig_rms = rms(pcm);
            let err_rms = rms(&errors);
            let snr = snr_db(orig_rms, err_rms);

            eprintln!(
                "  {:<15} samples={:<6} blocks={:<4} max_err={:<6} rms_err={:<8.1} snr={:.1}dB orig_rms={:.1}",
                name, pcm.len(), num_blocks, max_err, err_rms, snr, orig_rms,
            );

            // Write WAVs
            write_wav_mono(
                &format!("{}/01_adpcm/{}_original.wav", OUT_DIR, name),
                pcm, 44100,
            );
            write_wav_mono(
                &format!("{}/01_adpcm/{}_decoded.wav", OUT_DIR, name),
                &decoded[..pcm.len().min(decoded.len())], 44100,
            );
            write_wav_mono(
                &format!("{}/01_adpcm/{}_error.wav", OUT_DIR, name),
                &errors, 44100,
            );

            // For non-silence, SNR should be at least 20 dB
            if orig_rms > 100.0 {
                assert!(
                    snr > 20.0,
                    "{} ADPCM roundtrip SNR {:.1}dB is too low (expected >20dB)",
                    name, snr,
                );
            }
        }

        // Also test filter selection: dump which filter/shift each block chose
        eprintln!("\n  Filter/shift distribution for 440hz_mid:");
        let pcm = &generate_sine(440.0, 0.5, 44100, 16000.0);
        let encoded = adpcm::encode_pcm_to_adpcm(pcm, None, None);
        let num_blocks = encoded.len() / 16;
        for b in 0..num_blocks.min(10) {
            let sf = encoded[b * 16];
            let shift = sf & 0x0F;
            let filter = (sf >> 4) & 0x07;
            let flags = encoded[b * 16 + 1];
            eprintln!("    block {}: filter={} shift={} flags=0x{:02X}", b, filter, shift, flags);
        }
        eprintln!("    ... ({} blocks total)", num_blocks);

        eprintln!("\n  WAVs written to {}/01_adpcm/", OUT_DIR);
    }

    // =========================================================================
    // TEST 2: Gaussian Interpolation
    // =========================================================================

    #[test]
    fn spu_pipeline_02_gaussian_interpolation() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 2: Gaussian Interpolation at Various Pitches");
        eprintln!("{}\n", "=".repeat(70));

        // Store a 440Hz sine in SPU RAM and play it at different pitches
        let source_freq = 440.0;
        let pcm = generate_sine(source_freq, 1.0, 44100, 16000.0);

        let mut spu_ram = SpuRam::new();
        let (offset, length, loop_off, _) = store_in_spu_ram(&mut spu_ram, &pcm, None, None);

        // Test pitches: 0x800=half speed, 0x1000=native, 0x2000=double
        let test_pitches: Vec<(&str, u16, f64)> = vec![
            ("half_speed",   0x0800, source_freq / 2.0),  // 220 Hz
            ("native",       0x1000, source_freq),         // 440 Hz
            ("up_octave",    0x2000, source_freq * 2.0),   // 880 Hz
            ("up_fifth",     0x1800, source_freq * 1.5),   // ~660 Hz
            ("quarter",      0x0400, source_freq / 4.0),   // 110 Hz
        ];

        for (name, pitch, expected_freq) in &test_pitches {
            let region = SampleRegion {
                spu_ram_offset: offset,
                loop_offset: loop_off,
                has_loop: false,
                adpcm_length: length,
                base_note: 69, // A4
                base_pitch: *pitch,
                key_lo: 0,
                key_hi: 127,
                adsr: AdsrParams::sustained(),
                default_volume: 0x7FFF,
                fine_tune: 0,
                scale_tuning: 100,
            };

            let mut voice = Voice::new();
            voice.key_on(&region, 69, 127);

            // Render 0.5 seconds
            let num_samples = 22050;
            let mut output = Vec::with_capacity(num_samples);
            for _ in 0..num_samples {
                let (left, _right) = voice.tick(&spu_ram);
                output.push(left.clamp(-32768, 32767) as i16);
            }

            // Measure frequency (skip first 100 samples for transient)
            let skip = 200;
            let measured = measure_frequency(&output[skip..], 44100);
            let freq_error_pct = if *expected_freq > 0.0 {
                ((measured - expected_freq) / expected_freq * 100.0).abs()
            } else {
                0.0
            };

            let out_rms = rms(&output[skip..]);

            eprintln!(
                "  {:<15} pitch=0x{:04X} expected={:.1}Hz measured={:.1}Hz err={:.2}% rms={:.0}",
                name, pitch, expected_freq, measured, freq_error_pct, out_rms,
            );

            write_wav_mono(
                &format!("{}/02_gaussian/{}.wav", OUT_DIR, name),
                &output, 44100,
            );

            // Frequency should be within 2% of expected
            if *expected_freq > 50.0 && out_rms > 100.0 {
                assert!(
                    freq_error_pct < 2.0,
                    "{}: frequency error {:.2}% too large (expected {:.1}Hz, got {:.1}Hz)",
                    name, freq_error_pct, expected_freq, measured,
                );
            }
        }

        // Also write the raw PCM for reference
        write_wav_mono(&format!("{}/02_gaussian/00_source_pcm.wav", OUT_DIR), &pcm, 44100);
        eprintln!("\n  WAVs written to {}/02_gaussian/", OUT_DIR);
    }

    // =========================================================================
    // TEST 3: ADSR Envelope Timing
    // =========================================================================

    #[test]
    fn spu_pipeline_03_adsr_envelope() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 3: ADSR Envelope Timing and Shape");
        eprintln!("{}\n", "=".repeat(70));

        // Create a loud constant signal in SPU RAM (DC offset for clean envelope reading)
        // Actually, use a high-frequency sine so the envelope is clearly visible
        let pcm = generate_sine(1000.0, 3.0, 44100, 30000.0);
        let mut spu_ram = SpuRam::new();
        let (offset, length, _, _) = store_in_spu_ram(&mut spu_ram, &pcm, Some(0), Some(pcm.len()));

        let test_envelopes: Vec<(&str, AdsrParams, f64)> = vec![
            // (name, adsr, key_off_time_secs)
            ("fast_attack_fast_release", AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 4,
                sustain_level: 12,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: true,
                release_shift: 5,
            }, 0.5),

            ("slow_attack", AdsrParams {
                attack_exp: false,
                attack_shift: 8,
                attack_step: 0,
                decay_shift: 4,
                sustain_level: 12,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: true,
                release_shift: 8,
            }, 1.0),

            ("exp_attack", AdsrParams {
                attack_exp: true,
                attack_shift: 6,
                attack_step: 1,
                decay_shift: 4,
                sustain_level: 10,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: true,
                release_shift: 8,
            }, 1.0),

            ("deep_decay", AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 8,
                sustain_level: 4,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: true,
                release_shift: 10,
            }, 1.5),

            ("sustain_decrease", AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 4,
                sustain_level: 8,
                sustain_exp: true,
                sustain_decrease: true,
                sustain_shift: 10,
                sustain_step: 0,
                release_exp: true,
                release_shift: 8,
            }, 1.5),

            ("percussive", AdsrParams::percussive(), 0.3),
            ("sustained", AdsrParams::sustained(), 1.0),
        ];

        for (name, adsr, key_off_time) in &test_envelopes {
            let region = SampleRegion {
                spu_ram_offset: offset,
                loop_offset: offset, // loop from start
                has_loop: true,
                adpcm_length: length,
                base_note: 69,
                base_pitch: NATIVE_PITCH,
                key_lo: 0,
                key_hi: 127,
                adsr: *adsr,
                default_volume: 0x7FFF,
                fine_tune: 0,
                scale_tuning: 100,
            };

            let mut voice = Voice::new();
            voice.key_on(&region, 69, 127);

            let total_samples = (2.5 * 44100.0) as usize;
            let key_off_sample = (*key_off_time * 44100.0) as usize;
            let mut output_left = Vec::with_capacity(total_samples);
            let mut envelope_levels = Vec::with_capacity(total_samples);

            for i in 0..total_samples {
                if i == key_off_sample {
                    voice.key_off();
                }
                let (left, _) = voice.tick(&spu_ram);
                output_left.push(left.clamp(-32768, 32767) as i16);
                envelope_levels.push(voice.adsr_level());
            }

            // Find key timings from envelope
            let max_level = *envelope_levels.iter().max().unwrap_or(&0);
            let attack_peak_sample = envelope_levels.iter().position(|&l| l == max_level).unwrap_or(0);
            let attack_time_ms = attack_peak_sample as f64 / 44.1;

            // Find when sustain starts (envelope stops decreasing after decay)
            let sustain_level = adsr.sustain_level_i16();

            // Find when voice goes silent after key_off
            let silence_sample = envelope_levels[key_off_sample..].iter()
                .position(|&l| l == 0)
                .map(|i| i + key_off_sample);
            let release_time_ms = silence_sample
                .map(|s| (s - key_off_sample) as f64 / 44.1)
                .unwrap_or(f64::NAN);

            eprintln!(
                "  {:<25} attack_peak={:.1}ms (sample {}) max_lvl={} sustain_target=0x{:04X}",
                name, attack_time_ms, attack_peak_sample, max_level, sustain_level,
            );
            eprintln!(
                "  {:<25} key_off@{:.0}ms release_dur={:.1}ms silence@{:?}",
                "", key_off_time * 1000.0, release_time_ms, silence_sample,
            );
            eprintln!(
                "  {:<25} rate: atk={}*4+{}={} dec={}*4={} sus={}*4+{}={} rel={}*4={}",
                "",
                adsr.attack_shift, adsr.attack_step, adsr.attack_shift * 4 + adsr.attack_step,
                adsr.decay_shift, adsr.decay_shift * 4,
                adsr.sustain_shift, adsr.sustain_step, adsr.sustain_shift * 4 + adsr.sustain_step,
                adsr.release_shift, adsr.release_shift * 4,
            );

            // Write envelope as WAV (scale 0-0x7FFF to i16 range)
            let env_wav: Vec<i16> = envelope_levels.iter()
                .map(|&l| l)
                .collect();
            write_wav_mono(
                &format!("{}/03_adsr/{}_envelope.wav", OUT_DIR, name),
                &env_wav, 44100,
            );
            write_wav_mono(
                &format!("{}/03_adsr/{}_output.wav", OUT_DIR, name),
                &output_left, 44100,
            );

            // The max level should reach at least 0x7000 for sustained envelopes
            if adsr.attack_shift < 10 {
                assert!(
                    max_level > 0x6000,
                    "{}: max envelope level 0x{:04X} too low (attack didn't reach peak)",
                    name, max_level,
                );
            }
        }

        eprintln!("\n  WAVs written to {}/03_adsr/", OUT_DIR);
    }

    // =========================================================================
    // TEST 4: Full Voice Pipeline
    // =========================================================================

    #[test]
    fn spu_pipeline_04_full_voice() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 4: Full Voice Pipeline (ADPCM → Gauss → ADSR → Volume)");
        eprintln!("{}\n", "=".repeat(70));

        // Generate A4 (440Hz) at 44100Hz sample rate
        let source = generate_sine(440.0, 2.0, 44100, 20000.0);

        let mut spu_ram = SpuRam::new();
        let (offset, length, loop_off, _) = store_in_spu_ram(
            &mut spu_ram, &source, Some(0), Some(source.len()),
        );

        // Play at native pitch (should produce 440Hz)
        let region = SampleRegion {
            spu_ram_offset: offset,
            loop_offset: loop_off,
            has_loop: true,
            adpcm_length: length,
            base_note: 69, // A4
            base_pitch: NATIVE_PITCH, // 0x1000
            key_lo: 0,
            key_hi: 127,
            adsr: AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 0, // no decay (sustain_level=15)
                sustain_level: 15, // max sustain
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: true,
                release_shift: 8,
            },
            default_volume: 0x7FFF,
            fine_tune: 0,
            scale_tuning: 100,
        };

        // Test notes: A4 (69), A5 (81), A3 (57), C4 (60), E4 (64)
        let test_notes: Vec<(&str, u8, f64)> = vec![
            ("A4_native", 69, 440.0),
            ("A5_octave_up", 81, 880.0),
            ("A3_octave_down", 57, 220.0),
            ("C4", 60, 261.63),
            ("E4", 64, 329.63),
            ("C5", 72, 523.25),
        ];

        for (name, note, expected_freq) in &test_notes {
            let mut voice = Voice::new();
            voice.key_on(&region, *note, 127);

            let pitch = region.pitch_for_note(*note);
            eprintln!("  {:<20} note={} pitch=0x{:04X} expected_freq={:.2}Hz", name, note, pitch, expected_freq);

            // Render 1 second
            let num_samples = 44100;
            let mut left_samples = Vec::with_capacity(num_samples);
            let mut right_samples = Vec::with_capacity(num_samples);

            for i in 0..num_samples {
                // Key off at 0.8 seconds
                if i == 35280 {
                    voice.key_off();
                }
                let (left, right) = voice.tick(&spu_ram);
                left_samples.push(left.clamp(-32768, 32767) as i16);
                right_samples.push(right.clamp(-32768, 32767) as i16);
            }

            // Measure frequency from output (skip attack transient)
            let skip = 500;
            let analysis_end = 35000; // before key_off
            let measured = measure_frequency(&left_samples[skip..analysis_end], 44100);
            let freq_error_pct = ((measured - expected_freq) / expected_freq * 100.0).abs();
            let output_rms = rms(&left_samples[skip..analysis_end]);

            eprintln!(
                "  {:<20} measured_freq={:.2}Hz error={:.3}% rms={:.0}",
                "", measured, freq_error_pct, output_rms,
            );

            // Dump first 50 samples for debugging
            eprintln!(
                "  {:<20} first_20_samples: {:?}",
                "", &left_samples[..20],
            );

            write_wav_stereo(
                &format!("{}/04_voice/{}.wav", OUT_DIR, name),
                &left_samples, &right_samples, 44100,
            );

            // Frequency should be within 1%
            if output_rms > 50.0 {
                assert!(
                    freq_error_pct < 1.0,
                    "{}: frequency error {:.3}% (expected {:.2}Hz, got {:.2}Hz)",
                    name, freq_error_pct, expected_freq, measured,
                );
            }
        }

        // Write source PCM for reference
        write_wav_mono(&format!("{}/04_voice/00_source_440hz.wav", OUT_DIR), &source, 44100);
        eprintln!("\n  WAVs written to {}/04_voice/", OUT_DIR);
    }

    // =========================================================================
    // TEST 5: Pitch Accuracy
    // =========================================================================

    #[test]
    fn spu_pipeline_05_pitch_accuracy() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 5: Pitch Accuracy Across the Keyboard");
        eprintln!("{}\n", "=".repeat(70));

        // Use a 1000Hz source at 44100Hz (easier to analyze)
        let source = generate_sine(1000.0, 2.0, 44100, 20000.0);
        let mut spu_ram = SpuRam::new();
        let (offset, length, loop_off, _) = store_in_spu_ram(
            &mut spu_ram, &source, Some(0), Some(source.len()),
        );

        // Base note 69 (A4), base_pitch = 0x1000 (native)
        // This means: playing note 69 at pitch 0x1000 gives 1000Hz source frequency
        let region = SampleRegion {
            spu_ram_offset: offset,
            loop_offset: loop_off,
            has_loop: true,
            adpcm_length: length,
            base_note: 69,
            base_pitch: NATIVE_PITCH,
            key_lo: 0,
            key_hi: 127,
            adsr: AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 0,
                sustain_level: 15,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: false,
                release_shift: 31,
            },
            default_volume: 0x7FFF,
            fine_tune: 0,
            scale_tuning: 100,
        };

        // Test every note from C2 (36) to C7 (96)
        eprintln!("  {:>4} {:>6} {:>12} {:>12} {:>8}", "Note", "Pitch", "Expected Hz", "Measured Hz", "Err%");
        eprintln!("  {}", "-".repeat(50));

        let mut all_left = Vec::new();

        for note in (36..=96).step_by(4) {
            let pitch = region.pitch_for_note(note);
            let expected = 1000.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);

            let mut voice = Voice::new();
            voice.key_on(&region, note, 127);

            let num_samples = 22050; // 0.5 seconds
            let mut out = Vec::with_capacity(num_samples);
            for _ in 0..num_samples {
                let (left, _) = voice.tick(&spu_ram);
                out.push(left.clamp(-32768, 32767) as i16);
            }

            let measured = measure_frequency(&out[200..], 44100);
            let err_pct = if expected > 0.0 { ((measured - expected) / expected * 100.0).abs() } else { 0.0 };

            eprintln!(
                "  {:>4} 0x{:04X} {:>12.2} {:>12.2} {:>7.3}%",
                note, pitch, expected, measured, err_pct,
            );

            // Accumulate all notes into one file (with 0.1s gap)
            all_left.extend_from_slice(&out);
            all_left.extend_from_slice(&vec![0i16; 4410]); // gap

            // Pitch should be within 2% (skip notes where pitch hits 0x3FFF hardware clamp)
            if expected > 100.0 && expected < 15000.0 && pitch < MAX_PITCH {
                assert!(
                    err_pct < 2.0,
                    "Note {}: pitch error {:.3}% (expected {:.2}Hz, got {:.2}Hz)",
                    note, err_pct, expected, measured,
                );
            }
        }

        write_wav_mono(&format!("{}/05_pitch/all_notes.wav", OUT_DIR), &all_left, 44100);
        eprintln!("\n  WAVs written to {}/05_pitch/", OUT_DIR);
    }

    // =========================================================================
    // TEST 6: Volume and Pan
    // =========================================================================

    #[test]
    fn spu_pipeline_06_volume_pan() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 6: Volume and Pan Math");
        eprintln!("{}\n", "=".repeat(70));

        let source = generate_sine(440.0, 1.0, 44100, 20000.0);
        let mut spu_ram = SpuRam::new();
        let (offset, length, loop_off, _) = store_in_spu_ram(
            &mut spu_ram, &source, Some(0), Some(source.len()),
        );

        let region = SampleRegion {
            spu_ram_offset: offset,
            loop_offset: loop_off,
            has_loop: true,
            adpcm_length: length,
            base_note: 69,
            base_pitch: NATIVE_PITCH,
            key_lo: 0,
            key_hi: 127,
            adsr: AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 0,
                sustain_level: 15,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: false,
                release_shift: 31,
            },
            default_volume: 0x7FFF,
            fine_tune: 0,
            scale_tuning: 100,
        };

        // Test different volume levels
        let volume_tests: Vec<(&str, u8, u8, u8)> = vec![
            // (name, velocity, pan, volume)
            ("max_vol_center",   127, 64, 127),
            ("half_vol_center",  127, 64, 64),
            ("quarter_vol",      127, 64, 32),
            ("min_vol",          127, 64, 1),
            ("half_vel_center",  64,  64, 127),
            ("max_pan_left",     127, 0,  127),
            ("max_pan_right",    127, 127, 127),
            ("pan_left_quarter", 127, 32, 127),
            ("pan_right_quarter",127, 96, 127),
        ];

        for (name, velocity, pan, volume) in &volume_tests {
            let mut voice = Voice::new();
            voice.key_on(&region, 69, *velocity);
            voice.set_volume_from_pan(*pan, *volume);

            let num_samples = 22050;
            let mut left_out = Vec::with_capacity(num_samples);
            let mut right_out = Vec::with_capacity(num_samples);
            for _ in 0..num_samples {
                let (l, r) = voice.tick(&spu_ram);
                left_out.push(l.clamp(-32768, 32767) as i16);
                right_out.push(r.clamp(-32768, 32767) as i16);
            }

            let left_rms = rms(&left_out[200..]);
            let right_rms = rms(&right_out[200..]);
            let total_rms = ((left_rms * left_rms + right_rms * right_rms) / 2.0).sqrt();

            eprintln!(
                "  {:<22} vel={:>3} pan={:>3} vol={:>3} → L_rms={:>7.1} R_rms={:>7.1} total={:.1}",
                name, velocity, pan, volume, left_rms, right_rms, total_rms,
            );

            write_wav_stereo(
                &format!("{}/06_volume/{}.wav", OUT_DIR, name),
                &left_out, &right_out, 44100,
            );
        }

        // Verify: center pan should have equal L/R, hard left should have ~0 right, etc.
        eprintln!("\n  WAVs written to {}/06_volume/", OUT_DIR);
    }

    // =========================================================================
    // TEST 7: Multi-voice mixing through SpuCore
    // =========================================================================

    #[test]
    fn spu_pipeline_07_multi_voice_mixing() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 7: Multi-Voice Mixing Through SpuCore");
        eprintln!("{}\n", "=".repeat(70));

        let mut spu = SpuCore::new();

        // Create a sample library manually (without SF2)
        let mut library = SampleLibrary::new("test".to_string());

        // Store test signals: sine waves at different base frequencies
        let notes_hz = [261.63, 329.63, 392.00, 523.25]; // C4, E4, G4, C5
        let mut regions = Vec::new();

        for (i, &freq) in notes_hz.iter().enumerate() {
            let pcm = generate_sine(freq, 2.0, 44100, 16000.0);
            let adpcm = adpcm::encode_pcm_to_adpcm(&pcm, Some(0), Some(pcm.len()));
            let offset = library.spu_ram.allocate(&adpcm).expect("SPU RAM full");

            regions.push(SampleRegion {
                spu_ram_offset: offset,
                loop_offset: offset,
                has_loop: true,
                adpcm_length: adpcm.len() as u32,
                base_note: 60 + (i * 4) as u8, // C4, E4, G4, C5
                base_pitch: NATIVE_PITCH,
                key_lo: 60 + (i * 4) as u8,
                key_hi: 60 + (i * 4) as u8,
                adsr: AdsrParams {
                    attack_exp: false,
                    attack_shift: 0,
                    attack_step: 3,
                    decay_shift: 0,
                    sustain_level: 15,
                    sustain_exp: false,
                    sustain_decrease: false,
                    sustain_shift: 31,
                    sustain_step: 0,
                    release_exp: true,
                    release_shift: 8,
                },
                default_volume: 0x7FFF,
                fine_tune: 0,
                scale_tuning: 100,
            });
        }

        library.instruments.push(InstrumentBank {
            name: "Test Piano".to_string(),
            program: 0,
            regions,
        });

        // Manually inject library (since we're not going through SF2 loading)
        // We need to use a different approach - directly set up voices
        let spu_ram = &library.spu_ram;

        // Instead of using SpuCore (which requires SF2 loading), test voice mixing directly
        let mut voices: Vec<Voice> = Vec::new();
        let bank = &library.instruments[0];

        for (i, &freq) in notes_hz.iter().enumerate() {
            let note = 60 + (i * 4) as u8;
            let region = bank.region_for_note(note).unwrap();
            let mut voice = Voice::new();
            voice.key_on(region, note, 100);
            voices.push(voice);
        }

        // Render 2 seconds of mixed output
        let num_samples = 88200;
        let mut left_out = Vec::with_capacity(num_samples);
        let mut right_out = Vec::with_capacity(num_samples);
        let mut individual: Vec<Vec<i16>> = vec![Vec::with_capacity(num_samples); 4];

        for i in 0..num_samples {
            // Key off at 1.5 seconds
            if i == 66150 {
                for voice in &mut voices {
                    voice.key_off();
                }
            }

            let mut sum_l: i32 = 0;
            let mut sum_r: i32 = 0;
            for (v, voice) in voices.iter_mut().enumerate() {
                let (l, r) = voice.tick(spu_ram);
                sum_l += l;
                sum_r += r;
                individual[v].push(l.clamp(-32768, 32767) as i16);
            }

            left_out.push(sum_l.clamp(-32768, 32767) as i16);
            right_out.push(sum_r.clamp(-32768, 32767) as i16);
        }

        let mix_rms = rms(&left_out[200..66000]);
        eprintln!("  C-major chord: 4 voices mixed, rms={:.0}", mix_rms);
        eprintln!("  Individual voice RMS (during sustain):");
        for (i, ind) in individual.iter().enumerate() {
            let r = rms(&ind[200..66000]);
            eprintln!("    voice {}: rms={:.0}", i, r);
        }

        write_wav_stereo(
            &format!("{}/07_mixing/c_major_chord.wav", OUT_DIR),
            &left_out, &right_out, 44100,
        );
        for (i, ind) in individual.iter().enumerate() {
            write_wav_mono(
                &format!("{}/07_mixing/voice_{}.wav", OUT_DIR, i),
                ind, 44100,
            );
        }

        // Mix should be non-trivial
        assert!(mix_rms > 100.0, "Mixed output is too quiet: rms={:.0}", mix_rms);
        eprintln!("\n  WAVs written to {}/07_mixing/", OUT_DIR);
    }

    // =========================================================================
    // TEST 8: Voice pipeline stage-by-stage dump
    // =========================================================================

    #[test]
    fn spu_pipeline_08_stage_by_stage() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 8: Stage-by-Stage Pipeline Dump");
        eprintln!("{}\n", "=".repeat(70));

        // This test dumps intermediate values at each pipeline stage
        // to identify exactly where things go wrong.

        let source = generate_sine(440.0, 0.5, 44100, 20000.0);
        let mut spu_ram = SpuRam::new();
        let (offset, length, _, _) = store_in_spu_ram(
            &mut spu_ram, &source, Some(0), Some(source.len()),
        );

        // Stage A: Raw ADPCM decode (no interpolation, no envelope)
        eprintln!("  Stage A: ADPCM Decode (raw decoded samples from first 3 blocks)");
        {
            let mut prev1: i16 = 0;
            let mut prev2: i16 = 0;
            for b in 0..3 {
                let mut output = [0i16; 28];
                adpcm::decode_block_from_ram(
                    spu_ram.data(),
                    offset as usize + b * 16,
                    &mut prev1, &mut prev2,
                    &mut output,
                );
                eprintln!("    block {}: {:?}", b, &output[..14]);
                eprintln!("    block {} (cont): {:?}", b, &output[14..]);
            }
        }

        // Stage B: Source PCM for comparison
        eprintln!("\n  Stage B: Source PCM (first 84 samples = 3 blocks worth)");
        eprintln!("    {:?}", &source[..14]);
        eprintln!("    {:?}", &source[14..28]);
        eprintln!("    {:?}", &source[28..42]);

        // Stage C: Full voice tick — dump per-tick values for first 56 samples
        eprintln!("\n  Stage C: Full voice tick (first 56 ticks at native pitch)");
        {
            let region = SampleRegion {
                spu_ram_offset: offset,
                loop_offset: offset,
                has_loop: true,
                adpcm_length: length,
                base_note: 69,
                base_pitch: NATIVE_PITCH,
                key_lo: 0,
                key_hi: 127,
                adsr: AdsrParams {
                    attack_exp: false,
                    attack_shift: 0,
                    attack_step: 3, // fastest attack
                    decay_shift: 0,
                    sustain_level: 15,
                    sustain_exp: false,
                    sustain_decrease: false,
                    sustain_shift: 31,
                    sustain_step: 0,
                    release_exp: false,
                    release_shift: 31,
                },
                default_volume: 0x7FFF,
                fine_tune: 0,
                scale_tuning: 100,
            };

            let mut voice = Voice::new();
            voice.key_on(&region, 69, 127);

            eprintln!("    {:>4} {:>8} {:>8} {:>6}", "tick", "left", "adsr_lv", "phase");
            for i in 0..56 {
                let adsr_before = voice.adsr_level();
                let phase_before = voice.adsr_phase();
                let (left, right) = voice.tick(&spu_ram);

                if i < 56 {
                    eprintln!(
                        "    {:>4} {:>8} {:>8} {:>6?}",
                        i, left, adsr_before, phase_before,
                    );
                }
            }
        }

        // Stage D: Volume scaling check
        eprintln!("\n  Stage D: Volume scaling analysis");
        {
            // How much does the volume >> 15 shift reduce the signal?
            let test_vals: Vec<(i16, i16)> = vec![
                (0x7FFF, 0x3FFF), // max ADSR * max volume
                (0x7FFF, 0x2000), // max ADSR * mid volume
                (0x4000, 0x3FFF), // mid ADSR * max volume
                (0x1000, 0x3FFF), // low ADSR * max volume
                (0x0100, 0x3FFF), // very low ADSR * max volume
            ];
            eprintln!("    {:>8} {:>8} {:>12} {:>12}", "sample", "vol", "result >>15", "result");
            for (sample, vol) in &test_vals {
                let result_full = (*sample as i32 * *vol as i32) >> 15;
                eprintln!("    {:>8} {:>8} {:>12} {:>12}", sample, vol, result_full, result_full);
            }

            // Double shift: ADSR then volume
            eprintln!("\n    Double shift: sample * (adsr >> 15) * (vol >> 15)");
            let sample_val: i32 = 20000;
            let adsr_levels = [0x7FFF, 0x4000, 0x2000, 0x1000, 0x0800, 0x0100];
            let vol = 0x3FFF_i32;
            for &adsr in &adsr_levels {
                let after_adsr = (sample_val * adsr) >> 15;
                let after_vol = (after_adsr * vol) >> 15;
                eprintln!(
                    "    sample={} adsr=0x{:04X} → after_adsr={} → vol=0x{:04X} → final={}",
                    sample_val, adsr, after_adsr, vol, after_vol,
                );
            }
        }

        eprintln!("\n  All intermediate data above — compare source PCM vs decoded vs voice output");
    }

    // =========================================================================
    // TEST 9: ADPCM block boundary and loop behavior
    // =========================================================================

    #[test]
    fn spu_pipeline_09_loop_and_boundaries() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 9: ADPCM Block Boundaries and Looping");
        eprintln!("{}\n", "=".repeat(70));

        // Create a short sample that's exactly 2 blocks (56 samples)
        // with a loop from block 1 back to block 0
        let pcm = generate_sine(440.0, 56.0 / 44100.0, 44100, 20000.0);
        assert_eq!(pcm.len(), 56);

        let mut spu_ram = SpuRam::new();
        let (offset, length, loop_off, has_loop) = store_in_spu_ram(
            &mut spu_ram, &pcm, Some(0), Some(56),
        );

        eprintln!("  Short looping sample: {} PCM samples = {} ADPCM bytes", pcm.len(), length);
        eprintln!("  offset=0x{:X} loop_off=0x{:X} has_loop={}", offset, loop_off, has_loop);

        // Dump the ADPCM blocks
        for b in 0..(length / 16) {
            let addr = offset + b * 16;
            let sf = spu_ram.read_byte(addr);
            let flags = spu_ram.read_byte(addr + 1);
            eprintln!("  block {}: shift={} filter={} flags=0x{:02X} (end={} repeat={} start={})",
                b, sf & 0x0F, (sf >> 4) & 0x07, flags,
                flags & 1, (flags >> 1) & 1, (flags >> 2) & 1,
            );
        }

        let region = SampleRegion {
            spu_ram_offset: offset,
            loop_offset: loop_off,
            has_loop,
            adpcm_length: length,
            base_note: 69,
            base_pitch: NATIVE_PITCH,
            key_lo: 0,
            key_hi: 127,
            adsr: AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 0,
                sustain_level: 15,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: false,
                release_shift: 31,
            },
            default_volume: 0x7FFF,
            fine_tune: 0,
            scale_tuning: 100,
        };

        let mut voice = Voice::new();
        voice.key_on(&region, 69, 127);

        // Render 200 samples (should loop multiple times through the 56-sample loop)
        let mut output = Vec::new();
        for _ in 0..8820 { // 0.2 seconds
            let (left, _) = voice.tick(&spu_ram);
            output.push(left.clamp(-32768, 32767) as i16);
        }

        // Check the output is continuous and non-zero
        let out_rms = rms(&output[100..]);
        eprintln!("  Looped output rms={:.0} (should be >100 for 440Hz sine)", out_rms);
        eprintln!("  Samples around loop boundary (samples 50-70):");
        eprintln!("    {:?}", &output[50..70]);

        write_wav_mono(&format!("{}/09_loop/short_loop.wav", OUT_DIR), &output, 44100);

        assert!(out_rms > 100.0, "Looped output is too quiet (rms={:.0})", out_rms);

        // Also test non-looping behavior: sample should stop
        let region_no_loop = SampleRegion {
            has_loop: false,
            ..region.clone()
        };

        let pcm_no_loop = generate_sine(440.0, 56.0 / 44100.0, 44100, 20000.0);
        let adpcm_no_loop = adpcm::encode_pcm_to_adpcm(&pcm_no_loop, None, None);
        let offset_nl = spu_ram.allocate(&adpcm_no_loop).expect("SPU RAM full");

        let region_nl = SampleRegion {
            spu_ram_offset: offset_nl,
            loop_offset: offset_nl,
            has_loop: false,
            adpcm_length: adpcm_no_loop.len() as u32,
            ..region
        };

        let mut voice2 = Voice::new();
        voice2.key_on(&region_nl, 69, 127);

        let mut out2 = Vec::new();
        for _ in 0..4410 {
            let (left, _) = voice2.tick(&spu_ram);
            out2.push(left.clamp(-32768, 32767) as i16);
        }

        // After 56 samples (at native pitch), the voice should stop
        let tail_rms = rms(&out2[200..]);
        eprintln!("  Non-looping: tail rms={:.0} (should be ~0 after sample ends)", tail_rms);

        write_wav_mono(&format!("{}/09_loop/no_loop.wav", OUT_DIR), &out2, 44100);
        eprintln!("\n  WAVs written to {}/09_loop/", OUT_DIR);
    }

    // =========================================================================
    // TEST 10: Compare SPU output vs raw PCM at same pitch
    // =========================================================================

    #[test]
    fn spu_pipeline_10_spu_vs_raw_pcm() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 10: SPU Pipeline vs Raw PCM Comparison");
        eprintln!("{}\n", "=".repeat(70));

        // This is the key diagnostic test. We compare:
        // A) Raw PCM sine wave at 440Hz
        // B) Same PCM encoded to ADPCM, decoded back (ADPCM roundtrip only)
        // C) Same ADPCM played through full voice pipeline at native pitch
        // D) Same ADPCM played through voice pipeline at different pitches
        //
        // If B sounds good but C doesn't, the problem is in Gaussian/ADSR/volume.
        // If B already sounds bad, the problem is in ADPCM encode/decode.

        let source = generate_sine(440.0, 2.0, 44100, 20000.0);
        let num_source = source.len();

        // A: Raw PCM
        write_wav_mono(&format!("{}/10_compare/A_raw_pcm.wav", OUT_DIR), &source, 44100);

        // B: ADPCM roundtrip (encode → decode, no voice pipeline)
        let encoded = adpcm::encode_pcm_to_adpcm(&source, None, None);
        let num_blocks = encoded.len() / 16;
        let mut decoded_rt = Vec::new();
        {
            let mut prev1: i16 = 0;
            let mut prev2: i16 = 0;
            for b in 0..num_blocks {
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&encoded[b * 16..(b + 1) * 16]);
                let block = AdpcmBlock::from_bytes(&bytes);
                let mut output = [0i16; 28];
                adpcm::decode_block(&block, &mut prev1, &mut prev2, &mut output);
                decoded_rt.extend_from_slice(&output);
            }
        }
        let decoded_rt = &decoded_rt[..num_source.min(decoded_rt.len())];
        write_wav_mono(&format!("{}/10_compare/B_adpcm_roundtrip.wav", OUT_DIR), decoded_rt, 44100);

        // Calculate ADPCM error
        let mut errors_b: Vec<i16> = Vec::new();
        for i in 0..decoded_rt.len().min(num_source) {
            errors_b.push((source[i] as i32 - decoded_rt[i] as i32).clamp(-32768, 32767) as i16);
        }
        let snr_b = snr_db(rms(&source), rms(&errors_b));
        eprintln!("  B (ADPCM roundtrip): SNR={:.1}dB max_err={}", snr_b,
            errors_b.iter().map(|&e| (e as i32).abs()).max().unwrap_or(0));
        write_wav_mono(&format!("{}/10_compare/B_error.wav", OUT_DIR), &errors_b, 44100);

        // C: Full voice pipeline at NATIVE pitch (should reproduce 440Hz)
        let mut spu_ram = SpuRam::new();
        let (offset, length, loop_off, _) = store_in_spu_ram(
            &mut spu_ram, &source, Some(0), Some(source.len()),
        );

        let region = SampleRegion {
            spu_ram_offset: offset,
            loop_offset: loop_off,
            has_loop: true,
            adpcm_length: length,
            base_note: 69,
            base_pitch: NATIVE_PITCH,
            key_lo: 0,
            key_hi: 127,
            adsr: AdsrParams {
                attack_exp: false,
                attack_shift: 0,
                attack_step: 3,
                decay_shift: 0,
                sustain_level: 15,
                sustain_exp: false,
                sustain_decrease: false,
                sustain_shift: 31,
                sustain_step: 0,
                release_exp: false,
                release_shift: 31,
            },
            default_volume: 0x7FFF,
            fine_tune: 0,
            scale_tuning: 100,
        };

        let mut voice = Voice::new();
        voice.key_on(&region, 69, 127); // Play at root note = native pitch

        let mut voice_output = Vec::with_capacity(num_source);
        for _ in 0..num_source {
            let (left, _) = voice.tick(&spu_ram);
            voice_output.push(left.clamp(-32768, 32767) as i16);
        }
        write_wav_mono(&format!("{}/10_compare/C_voice_native.wav", OUT_DIR), &voice_output, 44100);

        // Calculate voice pipeline error vs raw PCM
        let mut errors_c: Vec<i16> = Vec::new();
        for i in 0..voice_output.len().min(num_source) {
            errors_c.push((source[i] as i32 - voice_output[i] as i32).clamp(-32768, 32767) as i16);
        }
        let snr_c = snr_db(rms(&source), rms(&errors_c));
        let voice_rms = rms(&voice_output[200..]);
        let source_rms = rms(&source[200..]);
        eprintln!(
            "  C (voice native):    SNR={:.1}dB max_err={} src_rms={:.0} out_rms={:.0} ratio={:.3}",
            snr_c, errors_c.iter().map(|&e| (e as i32).abs()).max().unwrap_or(0),
            source_rms, voice_rms, voice_rms / source_rms,
        );
        write_wav_mono(&format!("{}/10_compare/C_error.wav", OUT_DIR), &errors_c, 44100);

        // D: Voice at octave up (note 81, should be 880Hz)
        let mut voice2 = Voice::new();
        voice2.key_on(&region, 81, 127);
        let mut out_d = Vec::with_capacity(num_source);
        for _ in 0..num_source {
            let (left, _) = voice2.tick(&spu_ram);
            out_d.push(left.clamp(-32768, 32767) as i16);
        }
        write_wav_mono(&format!("{}/10_compare/D_voice_octave_up.wav", OUT_DIR), &out_d, 44100);
        let freq_d = measure_frequency(&out_d[200..], 44100);
        eprintln!("  D (voice octave up): measured_freq={:.1}Hz (expected 880Hz) rms={:.0}", freq_d, rms(&out_d[200..]));

        // E: Voice at octave down (note 57, should be 220Hz)
        let mut voice3 = Voice::new();
        voice3.key_on(&region, 57, 127);
        let mut out_e = Vec::with_capacity(num_source);
        for _ in 0..num_source {
            let (left, _) = voice3.tick(&spu_ram);
            out_e.push(left.clamp(-32768, 32767) as i16);
        }
        write_wav_mono(&format!("{}/10_compare/E_voice_octave_down.wav", OUT_DIR), &out_e, 44100);
        let freq_e = measure_frequency(&out_e[200..], 44100);
        eprintln!("  E (voice octave dn):  measured_freq={:.1}Hz (expected 220Hz) rms={:.0}", freq_e, rms(&out_e[200..]));

        // Key question: what's the volume ratio between raw PCM and voice output?
        eprintln!("\n  VOLUME ANALYSIS (key diagnostic):");
        eprintln!("    Raw PCM RMS:           {:.0}", source_rms);
        eprintln!("    ADPCM roundtrip RMS:   {:.0}", rms(&decoded_rt[200..]));
        eprintln!("    Voice output RMS:      {:.0}", voice_rms);
        eprintln!("    Volume loss (ADPCM):   {:.1}dB", 20.0 * (rms(&decoded_rt[200..]) / source_rms).log10());
        eprintln!("    Volume loss (voice):   {:.1}dB", 20.0 * (voice_rms / source_rms).log10());

        eprintln!("\n  WAVs written to {}/10_compare/", OUT_DIR);
        eprintln!("  -> Open these in an audio editor (Audacity) and compare waveforms!");
    }

    // =========================================================================
    // TEST 11: ADSR timing accuracy (measured vs expected)
    // =========================================================================

    #[test]
    fn spu_pipeline_11_adsr_timing_accuracy() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 11: ADSR Timing Accuracy — Measured vs Expected");
        eprintln!("{}\n", "=".repeat(70));

        // Use the same BASE_ENV_TIME formula from convert.rs to predict timing
        // Then compare with actual envelope behavior
        let base_env_time: f64 = 32767.0 / 14336.0 / 44100.0;

        // Test specific rates and measure how long they take
        let test_rates: Vec<(u8, bool, bool, &str)> = vec![
            // (rate, decreasing, exponential, description)
            (0,   false, false, "rate=0 linear increase"),
            (4,   false, false, "rate=4 linear increase"),
            (8,   false, false, "rate=8 linear increase"),
            (16,  false, false, "rate=16 linear increase"),
            (24,  false, false, "rate=24 linear increase"),
            (32,  false, false, "rate=32 linear increase"),
            (40,  false, false, "rate=40 linear increase"),
            (48,  false, false, "rate=48 linear increase"),
            (56,  false, false, "rate=56 linear increase"),
            (64,  false, false, "rate=64 linear increase"),
            (80,  false, false, "rate=80 linear increase"),
            (100, false, false, "rate=100 linear increase"),
            (127, false, false, "rate=127 linear increase"),
            // Exponential increase
            (0,   false, true, "rate=0 exp increase"),
            (24,  false, true, "rate=24 exp increase"),
            (48,  false, true, "rate=48 exp increase"),
            // Linear decrease
            (0,   true, false, "rate=0 linear decrease"),
            (24,  true, false, "rate=24 linear decrease"),
            (48,  true, false, "rate=48 linear decrease"),
            // Exponential decrease
            (0,   true, true, "rate=0 exp decrease"),
            (24,  true, true, "rate=24 exp decrease"),
            (48,  true, true, "rate=48 exp decrease"),
        ];

        eprintln!("  {:>35} {:>12} {:>12} {:>10}", "Test", "Expected ms", "Measured ms", "Ratio");
        eprintln!("  {}", "-".repeat(75));

        for (rate, decreasing, exponential, desc) in &test_rates {
            // Expected time for linear traversal of full range
            let expected_time_secs = base_env_time * 2.0_f64.powf(*rate as f64 / 4.0);
            let expected_ms = expected_time_secs * 1000.0;

            // Actually tick the envelope and measure
            let mut level: i16 = if *decreasing { 0x7FFF } else { 0 };
            let mut env = VolumeEnvelopeForTest::new();
            env.reset(*rate, 0x7F, *decreasing, *exponential);

            let max_ticks = 44100 * 30; // 30 seconds max
            let mut ticks = 0;
            let target = if *decreasing { 0 } else { 0x7FFF };

            for _ in 0..max_ticks {
                env.tick(&mut level);
                ticks += 1;
                if !*decreasing && level >= 0x7FFF { break; }
                if *decreasing && level <= 0 { break; }
            }

            let measured_ms = ticks as f64 / 44.1;
            let ratio = if expected_ms > 0.0 { measured_ms / expected_ms } else { 0.0 };

            eprintln!(
                "  {:>35} {:>12.2} {:>12.2} {:>10.3}",
                desc, expected_ms, measured_ms, ratio,
            );
        }

        eprintln!("\n  Ratio should be ~1.0 for linear. Exponential will differ.");
        eprintln!("  This validates that seconds_to_rate() produces accurate timing.");
    }

    // =========================================================================
    // TEST 12: SF2 Full Pipeline — real instruments through SPU
    // =========================================================================

    #[test]
    fn spu_pipeline_12_sf2_full_pipeline() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 12: SF2 Full Pipeline — Real Instruments Through SPU");
        eprintln!("{}\n", "=".repeat(70));

        // Load the real soundfont
        let sf2_path = find_sf2_path();
        let sf2_bytes = match std::fs::read(&sf2_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  SKIP: Could not read SF2 at {:?}: {}", sf2_path, e);
                return;
            }
        };

        let soundfont = match crate::tracker::spu::convert::parse_sf2(&sf2_bytes) {
            Ok(sf) => sf,
            Err(e) => {
                eprintln!("  SKIP: Could not parse SF2: {}", e);
                return;
            }
        };
        eprintln!("  Loaded SF2: {} bytes", sf2_bytes.len());

        // Test instruments: (program, name, test_notes)
        let test_instruments: Vec<(u8, &str, Vec<u8>)> = vec![
            (0,   "Acoustic Grand Piano",  vec![48, 60, 72, 84]),  // C3, C4, C5, C6
            (24,  "Acoustic Guitar",        vec![48, 60, 72]),
            (40,  "Violin",                 vec![55, 60, 67, 72, 79]),  // G3, C4, G4, C5, G5
            (48,  "String Ensemble 1",      vec![48, 60, 72]),
            (56,  "Trumpet",                vec![60, 67, 72]),
            (73,  "Flute",                  vec![60, 72, 84]),
            (80,  "Lead 1 (square)",        vec![48, 60, 72]),
            (11,  "Vibraphone",             vec![60, 67, 72, 79]),
            (32,  "Acoustic Bass",          vec![36, 43, 48]),  // C2, G2, C3
        ];

        let mut library = SampleLibrary::new("TimGM6mb.sf2".to_string());

        for (program, name, notes) in &test_instruments {
            eprintln!("\n  --- Program {}: {} ---", program, name);

            let success = crate::tracker::spu::convert::convert_single_program(
                &soundfont, *program, &mut library,
            );
            if !success {
                eprintln!("  FAILED to convert program {}", program);
                continue;
            }

            let bank = match library.instrument(*program) {
                Some(b) => b,
                None => {
                    eprintln!("  No instrument bank after conversion");
                    continue;
                }
            };

            eprintln!("  Loaded '{}' with {} regions", bank.name, bank.regions.len());
            for (i, r) in bank.regions.iter().enumerate() {
                eprintln!(
                    "    region {}: [{}-{}] root={} bp=0x{:04X} ft={} st={} vol={} loop={}",
                    i, r.key_lo, r.key_hi, r.base_note, r.base_pitch,
                    r.fine_tune, r.scale_tuning, r.default_volume, r.has_loop,
                );
                eprintln!(
                    "      adsr: atk={}/{}{} dec={} sus_lv={} sus={}/{}{} rel={}{}",
                    r.adsr.attack_shift, r.adsr.attack_step,
                    if r.adsr.attack_exp { "e" } else { "l" },
                    r.adsr.decay_shift,
                    r.adsr.sustain_level,
                    r.adsr.sustain_shift, r.adsr.sustain_step,
                    if r.adsr.sustain_decrease { "-" } else { "+" },
                    r.adsr.release_shift,
                    if r.adsr.release_exp { "e" } else { "l" },
                );
            }

            // Play each test note
            for &note in notes {
                let region = match bank.region_for_note(note) {
                    Some(r) => r,
                    None => {
                        eprintln!("  Note {} has no region, skipping", note);
                        continue;
                    }
                };

                let pitch = region.pitch_for_note(note);
                let expected_freq = 440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);

                let mut voice = Voice::new();
                voice.key_on(region, note, 100);

                // Render 2 seconds: 1.5s sustain + 0.5s release
                let total_samples = 88200;
                let key_off_sample = 66150; // 1.5 seconds
                let mut left_samples = Vec::with_capacity(total_samples);
                let mut right_samples = Vec::with_capacity(total_samples);
                let mut envelope = Vec::with_capacity(total_samples);

                for i in 0..total_samples {
                    if i == key_off_sample {
                        voice.key_off();
                    }
                    let (left, right) = voice.tick(&library.spu_ram);
                    left_samples.push(left.clamp(-32768, 32767) as i16);
                    right_samples.push(right.clamp(-32768, 32767) as i16);
                    envelope.push(voice.adsr_level());
                }

                // Analyze
                let skip = 500;
                let sustain_end = key_off_sample;
                let measured_freq = if sustain_end > skip {
                    measure_frequency(&left_samples[skip..sustain_end], 44100)
                } else {
                    0.0
                };
                let freq_error_pct = if expected_freq > 0.0 {
                    ((measured_freq - expected_freq) / expected_freq * 100.0).abs()
                } else {
                    0.0
                };
                let out_rms = rms(&left_samples[skip..sustain_end.min(left_samples.len())]);
                let peak_env = *envelope.iter().max().unwrap_or(&0);

                let note_name = midi_note_name(note);
                eprintln!(
                    "    note={:<3} ({:<3}) pitch=0x{:04X} expected={:>7.1}Hz measured={:>7.1}Hz err={:.2}% rms={:.0} env_peak={}",
                    note, note_name, pitch, expected_freq, measured_freq, freq_error_pct, out_rms, peak_env,
                );

                let safe_name = name.replace(' ', "_").replace('(', "").replace(')', "");
                write_wav_stereo(
                    &format!("{}/12_sf2/{}_note{:03}.wav", OUT_DIR, safe_name, note),
                    &left_samples, &right_samples, 44100,
                );

                // Write envelope
                let env_wav: Vec<i16> = envelope.iter().copied().collect();
                write_wav_mono(
                    &format!("{}/12_sf2/{}_note{:03}_env.wav", OUT_DIR, safe_name, note),
                    &env_wav, 44100,
                );
            }
        }

        eprintln!(
            "\n  SPU RAM used: {:.0}KB / {:.0}KB",
            library.spu_ram.allocated_bytes() as f64 / 1024.0,
            512.0,
        );
        eprintln!("\n  WAVs written to {}/12_sf2/", OUT_DIR);
        eprintln!("  -> Open these in an audio editor and compare to the source SF2!");
    }

    // =========================================================================
    // TEST 13: SF2 ADSR Comparison — SF2 envelope params vs mapped PS1 ADSR
    // =========================================================================

    #[test]
    fn spu_pipeline_13_sf2_adsr_comparison() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 13: SF2 ADSR Parameter Mapping — SF2 vs PS1");
        eprintln!("{}\n", "=".repeat(70));

        let sf2_path = find_sf2_path();
        let sf2_bytes = match std::fs::read(&sf2_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  SKIP: Could not read SF2: {}", e);
                return;
            }
        };

        let soundfont = match crate::tracker::spu::convert::parse_sf2(&sf2_bytes) {
            Ok(sf) => sf,
            Err(e) => {
                eprintln!("  SKIP: {}", e);
                return;
            }
        };

        // Extract SF2 raw envelope params for comparison
        let presets = soundfont.get_presets();
        let instruments = soundfont.get_instruments();

        let test_programs: Vec<(u8, &str)> = vec![
            (0,  "Acoustic Grand Piano"),
            (11, "Vibraphone"),
            (24, "Acoustic Guitar"),
            (32, "Acoustic Bass"),
            (40, "Violin"),
            (48, "String Ensemble"),
            (56, "Trumpet"),
            (73, "Flute"),
            (80, "Lead 1 (square)"),
        ];

        eprintln!("  {:<25} {:>8} {:>8} {:>8} {:>8}  |  {:>6} {:>6} {:>6} {:>6} {:>6}",
            "Instrument", "SF2_atk", "SF2_dec", "SF2_sus", "SF2_rel",
            "PS1_atk", "PS1_dec", "PS1_sus", "PS1_rel", "PS1_sLv",
        );
        eprintln!("  {}", "-".repeat(100));

        let mut library = SampleLibrary::new("TimGM6mb.sf2".to_string());

        for (program, label) in &test_programs {
            // Get SF2 raw values
            let preset = presets.iter().find(|p| {
                p.get_bank_number() == 0 && p.get_patch_number() == *program as i32
            });
            let preset = match preset {
                Some(p) => p,
                None => {
                    eprintln!("  {:<25} — not found in SF2", label);
                    continue;
                }
            };

            // Get first instrument region's envelope
            let mut sf2_atk = 0.0_f32;
            let mut sf2_dec = 0.0_f32;
            let mut sf2_sus = 0.0_f32;
            let mut sf2_rel = 0.0_f32;
            'outer: for pr in preset.get_regions() {
                let inst_id = pr.get_instrument_id();
                if inst_id >= instruments.len() { continue; }
                let inst = &instruments[inst_id];
                for ir in inst.get_regions() {
                    sf2_atk = ir.get_attack_volume_envelope();
                    sf2_dec = ir.get_decay_volume_envelope();
                    sf2_sus = ir.get_sustain_volume_envelope();
                    sf2_rel = ir.get_release_volume_envelope();
                    break 'outer;
                }
            }

            // Convert and get PS1 ADSR
            crate::tracker::spu::convert::convert_single_program(
                &soundfont, *program, &mut library,
            );

            if let Some(bank) = library.instrument(*program) {
                if let Some(region) = bank.regions.first() {
                    let adsr = &region.adsr;
                    let ps1_atk = adsr.attack_shift as u16 * 4 + adsr.attack_step as u16;
                    let ps1_dec = adsr.decay_shift as u16 * 4;
                    let ps1_sus = adsr.sustain_shift as u16 * 4 + adsr.sustain_step as u16;
                    let ps1_rel = adsr.release_shift as u16 * 4;

                    eprintln!(
                        "  {:<25} {:>8.4} {:>8.4} {:>8.1} {:>8.4}  |  {:>6} {:>6} {:>6} {:>6} {:>6}",
                        label, sf2_atk, sf2_dec, sf2_sus, sf2_rel,
                        ps1_atk, ps1_dec, ps1_sus, ps1_rel, adsr.sustain_level,
                    );

                    // Render the envelope shape for this instrument
                    let pcm = generate_sine(440.0, 3.0, 44100, 20000.0);
                    let mut spu_ram = SpuRam::new();
                    let (offset, length, _, _) = store_in_spu_ram(&mut spu_ram, &pcm, Some(0), Some(pcm.len()));

                    let test_region = SampleRegion {
                        spu_ram_offset: offset,
                        loop_offset: offset,
                        has_loop: true,
                        adpcm_length: length,
                        base_note: 69,
                        base_pitch: super::super::tables::NATIVE_PITCH,
                        key_lo: 0,
                        key_hi: 127,
                        adsr: region.adsr,
                        default_volume: 0x7FFF,
                        fine_tune: 0,
                        scale_tuning: 100,
                    };

                    let mut voice = Voice::new();
                    voice.key_on(&test_region, 69, 127);

                    let total = 132300; // 3 seconds
                    let key_off = 88200; // 2 seconds
                    let mut env_out = Vec::with_capacity(total);
                    let mut audio_out = Vec::with_capacity(total);

                    for i in 0..total {
                        if i == key_off { voice.key_off(); }
                        let (left, _) = voice.tick(&spu_ram);
                        audio_out.push(left.clamp(-32768, 32767) as i16);
                        env_out.push(voice.adsr_level());
                    }

                    let safe = label.replace(' ', "_").replace('(', "").replace(')', "");
                    write_wav_mono(
                        &format!("{}/13_adsr_compare/{}_envelope.wav", OUT_DIR, safe),
                        &env_out, 44100,
                    );
                    write_wav_mono(
                        &format!("{}/13_adsr_compare/{}_audio.wav", OUT_DIR, safe),
                        &audio_out, 44100,
                    );
                }
            }
        }

        eprintln!("\n  WAVs written to {}/13_adsr_compare/", OUT_DIR);
        eprintln!("  -> Compare envelope shapes: should match instrument character");
        eprintln!("     Piano: fast attack, moderate decay to ~60% sustain");
        eprintln!("     Strings: slower attack, high sustain");
        eprintln!("     Trumpet: fast attack, high sustain, moderate release");
    }

    // =========================================================================
    // TEST 14: SF2 vs Raw PCM Comparison — Same instrument, SPU vs direct PCM
    // =========================================================================

    #[test]
    fn spu_pipeline_14_sf2_vs_raw_pcm() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 14: SF2 vs Raw PCM — SPU Pipeline vs Direct SF2 Sample Playback");
        eprintln!("{}\n", "=".repeat(70));

        let sf2_path = find_sf2_path();
        let sf2_bytes = match std::fs::read(&sf2_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  SKIP: Could not read SF2: {}", e);
                return;
            }
        };

        let soundfont = match crate::tracker::spu::convert::parse_sf2(&sf2_bytes) {
            Ok(sf) => sf,
            Err(e) => {
                eprintln!("  SKIP: {}", e);
                return;
            }
        };

        let wave_data = soundfont.get_wave_data();
        let sample_headers = soundfont.get_sample_headers();
        let presets = soundfont.get_presets();
        let instruments = soundfont.get_instruments();

        // Test program 0 (piano) at note 60 (C4)
        let program: u8 = 0;
        let test_note: u8 = 60;

        // --- A: Extract raw PCM from SF2 for this note ---
        let preset = match presets.iter().find(|p| {
            p.get_bank_number() == 0 && p.get_patch_number() == program as i32
        }) {
            Some(p) => p,
            None => {
                eprintln!("  SKIP: No preset for program {}", program);
                return;
            }
        };

        let mut raw_pcm: Option<Vec<i16>> = None;
        let mut raw_root_key: u8 = 60;
        let mut raw_sample_rate: u32 = 44100;

        'find_sample: for pr in preset.get_regions() {
            let inst_id = pr.get_instrument_id();
            if inst_id >= instruments.len() { continue; }
            let inst = &instruments[inst_id];

            for ir in inst.get_regions() {
                let key_lo = ir.get_key_range_start().max(0).min(127) as u8;
                let key_hi = ir.get_key_range_end().max(0).min(127) as u8;
                if test_note < key_lo || test_note > key_hi { continue; }

                let sample_start = ir.get_sample_start() as usize;
                let sample_end = ir.get_sample_end() as usize;
                if sample_start >= sample_end || sample_end > wave_data.len() { continue; }

                raw_pcm = Some(wave_data[sample_start..sample_end].to_vec());

                let root = ir.get_root_key();
                raw_root_key = if root >= 0 && root <= 127 { root as u8 } else {
                    let sid = ir.get_sample_id();
                    if sid < sample_headers.len() {
                        let op = sample_headers[sid].get_original_pitch();
                        if op >= 0 && op <= 127 { op as u8 } else { 60 }
                    } else { 60 }
                };

                let sid = ir.get_sample_id();
                raw_sample_rate = if sid < sample_headers.len() {
                    sample_headers[sid].get_sample_rate() as u32
                } else { 44100 };

                eprintln!(
                    "  Found SF2 sample: {} samples, root={}, rate={}",
                    sample_end - sample_start, raw_root_key, raw_sample_rate,
                );
                break 'find_sample;
            }
        }

        let raw_pcm = match raw_pcm {
            Some(p) => p,
            None => {
                eprintln!("  SKIP: Could not find sample for note {}", test_note);
                return;
            }
        };

        // Write raw PCM at its original sample rate
        write_wav_mono(
            &format!("{}/14_sf2_vs_pcm/A_raw_sf2_pcm.wav", OUT_DIR),
            &raw_pcm,
            raw_sample_rate,
        );

        // --- B: ADPCM roundtrip of the raw PCM (no voice pipeline) ---
        let encoded = crate::tracker::spu::adpcm::encode_pcm_to_adpcm(&raw_pcm, None, None);
        let num_blocks = encoded.len() / 16;
        let mut decoded_rt = Vec::new();
        {
            let mut prev1: i16 = 0;
            let mut prev2: i16 = 0;
            for b in 0..num_blocks {
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&encoded[b * 16..(b + 1) * 16]);
                let block = AdpcmBlock::from_bytes(&bytes);
                let mut output = [0i16; 28];
                crate::tracker::spu::adpcm::decode_block(&block, &mut prev1, &mut prev2, &mut output);
                decoded_rt.extend_from_slice(&output);
            }
        }
        let decoded_rt = &decoded_rt[..raw_pcm.len().min(decoded_rt.len())];
        write_wav_mono(
            &format!("{}/14_sf2_vs_pcm/B_adpcm_roundtrip.wav", OUT_DIR),
            decoded_rt,
            raw_sample_rate,
        );

        // ADPCM quality
        let mut errors: Vec<i16> = Vec::new();
        for i in 0..decoded_rt.len().min(raw_pcm.len()) {
            errors.push((raw_pcm[i] as i32 - decoded_rt[i] as i32).clamp(-32768, 32767) as i16);
        }
        let raw_rms = rms(&raw_pcm);
        let err_rms = rms(&errors);
        let snr = snr_db(raw_rms, err_rms);
        eprintln!("  ADPCM roundtrip: SNR={:.1}dB raw_rms={:.0} err_rms={:.0}", snr, raw_rms, err_rms);

        // --- C: Full SPU pipeline at test_note ---
        let mut library = SampleLibrary::new("TimGM6mb.sf2".to_string());
        crate::tracker::spu::convert::convert_single_program(
            &soundfont, program, &mut library,
        );

        let bank = library.instrument(program).expect("Failed to load piano");
        let region = bank.region_for_note(test_note).expect("No region for note 60");

        eprintln!(
            "  Region: root={} bp=0x{:04X} ft={} st={} vol={} loop={}",
            region.base_note, region.base_pitch, region.fine_tune,
            region.scale_tuning, region.default_volume, region.has_loop,
        );

        let mut voice = Voice::new();
        voice.key_on(region, test_note, 127);

        // Render 3 seconds (2s sustain + 1s release)
        let total_samples = 132300;
        let key_off = 88200;
        let mut left_out = Vec::with_capacity(total_samples);
        let mut right_out = Vec::with_capacity(total_samples);
        let mut env_out = Vec::with_capacity(total_samples);

        for i in 0..total_samples {
            if i == key_off { voice.key_off(); }
            let (left, right) = voice.tick(&library.spu_ram);
            left_out.push(left.clamp(-32768, 32767) as i16);
            right_out.push(right.clamp(-32768, 32767) as i16);
            env_out.push(voice.adsr_level());
        }

        write_wav_stereo(
            &format!("{}/14_sf2_vs_pcm/C_spu_voice_output.wav", OUT_DIR),
            &left_out, &right_out, 44100,
        );
        write_wav_mono(
            &format!("{}/14_sf2_vs_pcm/C_envelope.wav", OUT_DIR),
            &env_out, 44100,
        );

        // Analyze SPU output
        let skip = 500;
        let voice_rms = rms(&left_out[skip..key_off]);
        let peak_env = *env_out.iter().max().unwrap_or(&0);
        let measured_freq = measure_frequency(&left_out[skip..key_off], 44100);
        let expected_freq = 440.0 * 2.0_f64.powf((test_note as f64 - 69.0) / 12.0);

        eprintln!("  SPU voice output:");
        eprintln!("    RMS:           {:.0}", voice_rms);
        eprintln!("    Peak envelope: 0x{:04X} ({})", peak_env, peak_env);
        eprintln!("    Measured freq:  {:.1}Hz (expected {:.1}Hz, err={:.2}%)",
            measured_freq, expected_freq,
            ((measured_freq - expected_freq) / expected_freq * 100.0).abs(),
        );

        // --- D: Volume comparison ---
        // What fraction of the raw PCM volume does the SPU pipeline produce?
        eprintln!("\n  VOLUME ANALYSIS:");
        eprintln!("    Raw SF2 PCM RMS:   {:.0}", raw_rms);
        eprintln!("    ADPCM roundtrip:   {:.0}", rms(decoded_rt));
        eprintln!("    SPU voice output:  {:.0}", voice_rms);
        eprintln!("    default_volume:    {} (0x{:04X})", region.default_volume, region.default_volume);
        if raw_rms > 0.0 {
            let volume_ratio = voice_rms / raw_rms;
            eprintln!("    Voice/Raw ratio:   {:.3} ({:.1}dB)", volume_ratio,
                20.0 * volume_ratio.log10());
        }

        eprintln!("\n  WAVs written to {}/14_sf2_vs_pcm/", OUT_DIR);
        eprintln!("  -> Compare A (raw SF2 PCM), B (ADPCM roundtrip), C (full SPU voice)");
        eprintln!("     A should sound like the original instrument sample");
        eprintln!("     B should sound nearly identical to A (ADPCM compression artifacts only)");
        eprintln!("     C should sound like A with PS1-style envelope applied");
    }

    // =========================================================================
    // Helper: find SF2 soundfont path
    // =========================================================================

    fn find_sf2_path() -> std::path::PathBuf {
        let candidates = [
            "assets/runtime/soundfonts/TimGM6mb.sf2",
            "../assets/runtime/soundfonts/TimGM6mb.sf2",
            "../../assets/runtime/soundfonts/TimGM6mb.sf2",
        ];
        for c in &candidates {
            let p = std::path::PathBuf::from(c);
            if p.exists() { return p; }
        }
        // Absolute path fallback
        std::path::PathBuf::from("/Users/ebonura/Desktop/repos/bonnie-32/assets/runtime/soundfonts/TimGM6mb.sf2")
    }

    /// Convert MIDI note number to note name
    fn midi_note_name(note: u8) -> String {
        let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let octave = (note as i32 / 12) - 1;
        let name = names[(note % 12) as usize];
        format!("{}{}", name, octave)
    }

    // =========================================================================
    // TEST 15: ADSR Decay Timing Verification — Isolated envelope test
    // =========================================================================

    #[test]
    fn spu_pipeline_15_adsr_decay_timing() {
        eprintln!("\n{}", "=".repeat(70));
        eprintln!("TEST 15: ADSR Decay Timing Verification");
        eprintln!("{}\n", "=".repeat(70));

        // Test the standalone VolumeEnvelopeForTest at various rates
        let test_rates: Vec<(u8, &str)> = vec![
            (20, "slow"),
            (40, "medium"),
            (60, "fast (piano decay)"),
        ];

        for (rate, label) in &test_rates {
            // Test exponential decay from 0x7FFF toward 0
            let mut level: i16 = 0x7FFF;
            let mut env = VolumeEnvelopeForTest::new();
            env.reset(*rate, 0x1F << 2, true, true); // decay params

            eprintln!("  Rate {} ({}):", rate, label);
            eprintln!("    step={} counter_increment=0x{:04X}", env.step, env.counter_increment);

            let total_ticks = 66150; // 1.5 seconds
            let mut fire_count = 0;
            let checkpoints = [4410, 22050, 44100, 66150]; // 0.1s, 0.5s, 1.0s, 1.5s
            let mut next_cp = 0;

            for tick in 0..total_ticks {
                let old = level;
                env.tick(&mut level);
                if level != old {
                    fire_count += 1;
                }

                if next_cp < checkpoints.len() && tick + 1 == checkpoints[next_cp] {
                    let t = checkpoints[next_cp] as f64 / 44100.0;
                    eprintln!(
                        "    @{:.1}s (tick {}): level={} (0x{:04X}) fires_so_far={}",
                        t, tick + 1, level, level, fire_count,
                    );
                    next_cp += 1;
                }
            }
            eprintln!("    TOTAL: {} fires in {} ticks", fire_count, total_ticks);
            eprintln!();
        }

        // Now test the FULL voice pipeline ADSR — same rate but through voice.tick()
        eprintln!("  --- Full Voice ADSR at rate 60 (matching piano) ---");

        // Create a simple looping sample for the voice
        let pcm = generate_sine(440.0, 0.1, 44100, 20000.0);
        let mut spu_ram = SpuRam::new();
        let (offset, length, _, _) = store_in_spu_ram(&mut spu_ram, &pcm, Some(0), Some(pcm.len()));

        let region = SampleRegion {
            spu_ram_offset: offset,
            loop_offset: offset,
            has_loop: true,
            adpcm_length: length,
            base_note: 69,
            base_pitch: super::super::tables::NATIVE_PITCH,
            key_lo: 0,
            key_hi: 127,
            adsr: AdsrParams {
                attack_exp: false,
                attack_shift: 4, // rate 17 (fast attack)
                attack_step: 1,
                decay_shift: 15, // rate 60 (same as piano)
                sustain_level: 0, // target 0x0800
                sustain_exp: true,
                sustain_decrease: true,
                sustain_shift: 18,
                sustain_step: 3,
                release_exp: true,
                release_shift: 12,
            },
            default_volume: 0x7FFF,
            fine_tune: 0,
            scale_tuning: 100,
        };

        let mut voice = Voice::new();
        voice.key_on(&region, 69, 127);

        let total = 66150; // 1.5 seconds
        let checkpoints = [100, 4410, 22050, 44100, 66150];
        let mut next_cp = 0;

        for tick in 0..total {
            voice.tick(&spu_ram);

            if next_cp < checkpoints.len() && tick + 1 == checkpoints[next_cp] {
                let t = checkpoints[next_cp] as f64 / 44100.0;
                let level = voice.adsr_level();
                let phase = voice.adsr_phase();
                eprintln!(
                    "    @{:.3}s (tick {}): level={} (0x{:04X}) phase={:?}",
                    t, tick + 1, level, level, phase,
                );
                next_cp += 1;
            }
        }
    }

    // =========================================================================
    // TEST 11 helper types
    // =========================================================================

    /// Standalone VolumeEnvelope for testing (mirrors the one in voice.rs)
    struct VolumeEnvelopeForTest {
        step: i32,
        counter_increment: u16,
        counter: u16,
        rate: u8,
        decreasing: bool,
        exponential: bool,
    }

    impl VolumeEnvelopeForTest {
        fn new() -> Self {
            Self { step: 0, counter_increment: 0, counter: 0, rate: 0, decreasing: false, exponential: false }
        }

        fn reset(&mut self, rate: u8, rate_mask: u8, decreasing: bool, exponential: bool) {
            self.rate = rate;
            self.decreasing = decreasing;
            self.exponential = exponential;
            self.counter = 0;
            self.counter_increment = 0x8000;

            let base_step = 7_i32 - (rate & 3) as i32;
            self.step = if decreasing { !base_step } else { base_step };

            let shift = (rate >> 2) as i32;
            if rate < 44 {
                self.step <<= 11 - shift;
            } else if rate >= 48 {
                let shift_amount = (shift - 11) as u32;
                if shift_amount >= 16 {
                    self.counter_increment = 0;
                } else {
                    self.counter_increment >>= shift_amount;
                    if (rate & rate_mask) != rate_mask {
                        self.counter_increment = self.counter_increment.max(1);
                    }
                }
            }
        }

        fn tick(&mut self, current_level: &mut i16) {
            if self.counter_increment == 0 { return; }

            let mut this_step = self.step;
            let mut this_increment = self.counter_increment as u32;

            if self.exponential {
                if self.decreasing {
                    this_step = (this_step * *current_level as i32) >> 15;
                } else {
                    if *current_level >= 0x6000 {
                        if self.rate < 40 {
                            this_step >>= 2;
                        } else if self.rate >= 44 {
                            this_increment >>= 2;
                        } else {
                            this_step >>= 1;
                            this_increment >>= 1;
                        }
                    }
                }
            }

            self.counter = self.counter.wrapping_add(this_increment as u16);
            if self.counter & 0x8000 == 0 { return; }
            self.counter = 0;

            let new_level = *current_level as i32 + this_step;
            if !self.decreasing {
                *current_level = new_level.clamp(-32768, 32767) as i16;
            } else {
                *current_level = new_level.max(0) as i16;
            }
        }
    }
}
