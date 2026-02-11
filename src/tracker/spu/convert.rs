//! SF2 → PS1 SPU ADPCM converter
//!
//! Extracts PCM sample data from SF2 soundfonts (via rustysynth),
//! encodes them as PS1 4-bit ADPCM, and maps SF2 envelope parameters
//! to PS1 ADSR register values.
//!
//! Usage:
//! ```ignore
//! let library = convert_sf2_to_spu(sf2_bytes, "MyFont.sf2")?;
//! spu_core.load_sample_library(library);
//! ```

pub use rustysynth::SoundFont;
use rustysynth::LoopMode;
use std::collections::HashMap;
use super::adpcm;
use super::types::{
    AdsrParams, SampleRegion, InstrumentBank, SampleLibrary,
};
use super::tables::{NATIVE_PITCH, SAMPLES_PER_ADPCM_BLOCK};

/// Parse an SF2 soundfont from bytes without converting any samples.
/// The returned SoundFont can be stored and used for on-demand conversion.
pub fn parse_sf2(sf2_data: &[u8]) -> Result<SoundFont, String> {
    let mut cursor = std::io::Cursor::new(sf2_data);
    SoundFont::new(&mut cursor)
        .map_err(|e| format!("Failed to parse SF2: {:?}", e))
}

/// Convert a single GM program from an SF2 soundfont into SPU RAM.
///
/// Finds the bank-0 preset matching `program`, encodes its PCM samples
/// to ADPCM, and allocates them in the library's SPU RAM.
/// Returns `true` if the program was converted (or already loaded),
/// `false` if SPU RAM is full.
pub fn convert_single_program(
    soundfont: &SoundFont,
    program: u8,
    library: &mut SampleLibrary,
) -> bool {
    // Already loaded?
    if library.instrument(program).is_some() {
        return true;
    }

    let wave_data = soundfont.get_wave_data();
    let sample_headers = soundfont.get_sample_headers();
    let presets = soundfont.get_presets();
    let instruments = soundfont.get_instruments();

    // Find the bank-0 preset matching this program number
    let preset = match presets.iter().find(|p| {
        p.get_bank_number() == 0 && p.get_patch_number() == program as i32
    }) {
        Some(p) => p,
        None => {
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("SPU convert: no bank-0 preset for program {}", program);
            return false;
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    eprintln!(
        "SPU convert: loading program {} '{}' ({} regions) — {:.0}KB used",
        program, preset.get_name(), preset.get_regions().len(),
        library.spu_ram.allocated_bytes() as f64 / 1024.0,
    );

    let mut bank = InstrumentBank {
        name: preset.get_name().to_string(),
        program,
        regions: Vec::new(),
    };

    let mut spu_ram_full = false;

    // Cache: (sample_start, sample_end) → (spu_ram_offset, adpcm_length, loop_offset, has_loop)
    // Avoids re-encoding and re-allocating the same sample when multiple preset
    // regions route to the same instrument.
    let mut sample_cache: HashMap<(usize, usize), (u32, u32, u32, bool)> = HashMap::new();

    // Reference velocity for selecting velocity layers.
    // SF2 instruments often have multiple velocity layers (e.g., piano soft/loud).
    // We pick one representative layer per key range to avoid overlapping regions.
    const REF_VELOCITY: i32 = 100;

    for preset_region in preset.get_regions() {
        if spu_ram_full {
            break;
        }
        let inst_id = preset_region.get_instrument_id();
        if inst_id >= instruments.len() {
            continue;
        }
        let instrument = &instruments[inst_id];

        // Preset region constrains which notes route to this instrument
        let preset_key_lo = preset_region.get_key_range_start().max(0).min(127) as u8;
        let preset_key_hi = preset_region.get_key_range_end().max(0).min(127) as u8;

        // Velocity filtering at preset level
        let preset_vel_lo = preset_region.get_velocity_range_start();
        let preset_vel_hi = preset_region.get_velocity_range_end();
        if REF_VELOCITY < preset_vel_lo || REF_VELOCITY > preset_vel_hi {
            continue;
        }

        // Preset-level generator offsets (SF2 spec: additive with instrument values)
        let preset_coarse_tune = preset_region.get_coarse_tune();
        let preset_fine_tune = preset_region.get_fine_tune();
        let preset_attenuation = preset_region.get_initial_attenuation();

        for region in instrument.get_regions() {
            let inst_key_lo = region.get_key_range_start().max(0).min(127) as u8;
            let inst_key_hi = region.get_key_range_end().max(0).min(127) as u8;

            // Effective key range = intersection of preset and instrument ranges
            let key_lo = inst_key_lo.max(preset_key_lo);
            let key_hi = inst_key_hi.min(preset_key_hi);
            if key_lo > key_hi {
                continue;
            }

            // Velocity filtering at instrument level
            let inst_vel_lo = region.get_velocity_range_start();
            let inst_vel_hi = region.get_velocity_range_end();
            if REF_VELOCITY < inst_vel_lo || REF_VELOCITY > inst_vel_hi {
                continue;
            }

            let sample_start = region.get_sample_start() as usize;
            let sample_end = region.get_sample_end() as usize;
            if sample_start >= sample_end || sample_end > wave_data.len() {
                continue;
            }

            let pcm_data = &wave_data[sample_start..sample_end];
            if pcm_data.is_empty() {
                continue;
            }

            // Loop points
            let loop_mode = region.get_sample_modes();
            let has_loop = loop_mode != LoopMode::NoLoop;
            let loop_start_sample = if has_loop {
                let ls = region.get_sample_start_loop() as usize;
                if ls >= sample_start { Some(ls - sample_start) } else { None }
            } else {
                None
            };
            let loop_end_sample = if has_loop {
                let le = region.get_sample_end_loop() as usize;
                if le > sample_start { Some(le - sample_start) } else { None }
            } else {
                None
            };

            // Root key
            let root_key = region.get_root_key();
            let base_note = if root_key >= 0 && root_key <= 127 {
                root_key as u8
            } else {
                let sid = region.get_sample_id();
                if sid < sample_headers.len() {
                    let op = sample_headers[sid].get_original_pitch();
                    if op >= 0 && op <= 127 { op as u8 } else { 60 }
                } else {
                    60
                }
            };

            // Base pitch
            let sid = region.get_sample_id();
            let sample_rate = if sid < sample_headers.len() {
                sample_headers[sid].get_sample_rate() as f64
            } else {
                44100.0
            };
            let base_pitch_f64 = (sample_rate / 44100.0) * NATIVE_PITCH as f64;

            // Combined tuning: preset + instrument (SF2 generators are additive)
            let fine_tune = (region.get_fine_tune() + preset_fine_tune
                + (region.get_coarse_tune() + preset_coarse_tune) * 100) as i16;

            // Scale tuning: cents per semitone (preset + instrument, default 100)
            let scale_tuning = (region.get_scale_tuning()
                + preset_region.get_scale_tuning()) as i16;

            // For looped samples:
            // 1. Normalize pre-loop amplitude (prevent "falling tone" from natural decay)
            // 2. Align loop points to ADPCM block boundaries (prevent pitch error)
            //    Returns pitch_correction factor to apply to base_pitch
            let (pcm_for_encode, enc_loop_start, enc_loop_end, pitch_correction) = if has_loop {
                if let (Some(ls), Some(le)) = (loop_start_sample, loop_end_sample) {
                    let mut normalized = pcm_data.to_vec();
                    normalize_pre_loop_amplitude(&mut normalized, ls, le);
                    let (aligned, als, ale, corr) =
                        align_loop_to_adpcm_blocks(&normalized, ls, le);
                    (aligned, Some(als), Some(ale), corr)
                } else {
                    (pcm_data.to_vec(), loop_start_sample, loop_end_sample, 1.0)
                }
            } else {
                (pcm_data.to_vec(), loop_start_sample, loop_end_sample, 1.0)
            };
            let base_pitch = (base_pitch_f64 * pitch_correction) as u16;

            // Encode PCM → ADPCM (with deduplication)
            let cache_key = (sample_start, sample_end);
            let (spu_ram_offset, adpcm_length, loop_offset, cached_has_loop) =
                if let Some(&cached) = sample_cache.get(&cache_key) {
                    cached
                } else {
                    let adpcm_data = adpcm::encode_pcm_to_adpcm(
                        &pcm_for_encode,
                        enc_loop_start,
                        enc_loop_end,
                    );

                    let offset = match library.spu_ram.allocate(&adpcm_data) {
                        Some(offset) => offset,
                        None => {
                            #[cfg(not(target_arch = "wasm32"))]
                            eprintln!("    SPU RAM full at prog={}", program);
                            spu_ram_full = true;
                            break;
                        }
                    };

                    let loop_off = if has_loop {
                        let loop_block = enc_loop_start.unwrap_or(0) / 28;
                        offset + (loop_block as u32 * 16)
                    } else {
                        offset
                    };

                    let entry = (offset, adpcm_data.len() as u32, loop_off, has_loop);
                    sample_cache.insert(cache_key, entry);
                    entry
                };

            let adsr = sf2_envelope_to_adsr(region, has_loop);

            // Combined attenuation: preset + instrument (additive)
            // NOTE: rustysynth returns decibels (raw centibels * 0.1), not centibels!
            // So we use the dB→linear formula: 10^(-dB / 20)
            let attenuation_db = region.get_initial_attenuation() + preset_attenuation;
            let vol_scale = if attenuation_db <= 0.0 {
                1.0_f32
            } else {
                10.0_f32.powf(-attenuation_db / 20.0)
            };
            let default_volume = (0x7FFF as f32 * vol_scale).max(0.0).min(0x7FFF as f32) as i16;

            let sample_region = SampleRegion {
                spu_ram_offset,
                loop_offset: loop_offset,
                has_loop: cached_has_loop,
                adpcm_length,
                base_note,
                base_pitch,
                key_lo,
                key_hi,
                adsr,
                default_volume,
                fine_tune,
                scale_tuning,
            };

            // Comprehensive conversion diagnostic — dumps all critical values
            // so we can identify exactly where things go wrong at runtime
            #[cfg(not(target_arch = "wasm32"))]
            {
                // PCM data statistics
                let pcm_min = pcm_data.iter().copied().min().unwrap_or(0);
                let pcm_max = pcm_data.iter().copied().max().unwrap_or(0);
                let pcm_rms = {
                    let sum: f64 = pcm_data.iter().map(|&s| (s as f64) * (s as f64)).sum();
                    (sum / pcm_data.len().max(1) as f64).sqrt()
                };

                // ADPCM roundtrip quality check: decode what we encoded and compare
                let adpcm_start = spu_ram_offset as usize;
                let adpcm_end = adpcm_start + adpcm_length as usize;
                let num_blocks = adpcm_length as usize / 16;
                let mut rt_prev1: i16 = 0;
                let mut rt_prev2: i16 = 0;
                let mut rt_max_err: i32 = 0;
                let mut rt_sum_sq: f64 = 0.0;
                let mut rt_count: usize = 0;
                let ram_data = library.spu_ram.data();
                for b in 0..num_blocks {
                    let off = adpcm_start + b * 16;
                    if off + 16 > ram_data.len() { break; }
                    let mut bytes = [0u8; 16];
                    bytes.copy_from_slice(&ram_data[off..off + 16]);
                    let block = super::types::AdpcmBlock::from_bytes(&bytes);
                    let mut output = [0i16; 28];
                    super::adpcm::decode_block(&block, &mut rt_prev1, &mut rt_prev2, &mut output);
                    let pcm_offset = b * 28;
                    for j in 0..28 {
                        if pcm_offset + j < pcm_data.len() {
                            let err = (pcm_data[pcm_offset + j] as i32 - output[j] as i32).abs();
                            rt_max_err = rt_max_err.max(err);
                            rt_sum_sq += (err as f64) * (err as f64);
                            rt_count += 1;
                        }
                    }
                }
                let rt_rms = if rt_count > 0 { (rt_sum_sq / rt_count as f64).sqrt() } else { 0.0 };
                let snr_db = if rt_rms > 0.0 { 20.0 * (pcm_rms / rt_rms).log10() } else { 999.0 };

                // Raw SF2 envelope values (to verify units)
                let sf2_atk = region.get_attack_volume_envelope();
                let sf2_dec = region.get_decay_volume_envelope();
                let sf2_sus = region.get_sustain_volume_envelope();
                let sf2_rel = region.get_release_volume_envelope();

                // Pitch verification: at root, ±12 semitones
                let p_root = sample_region.pitch_for_note(base_note);
                let p_up12 = sample_region.pitch_for_note(base_note.saturating_add(12).min(127));
                let p_dn12 = sample_region.pitch_for_note(base_note.saturating_sub(12));

                // First 4 PCM samples (to sanity-check the data)
                let first4: Vec<i16> = pcm_data.iter().take(4).copied().collect();

                eprintln!(
                    "  DIAG [{:>3}-{:<3}] root={:<3} sr={:>5.0} bp=0x{:04X} ft={:>5} st={:>3} | pcm: n={} [{},{}] rms={:.0}",
                    key_lo, key_hi, base_note, sample_rate, base_pitch, fine_tune, scale_tuning,
                    pcm_data.len(), pcm_min, pcm_max, pcm_rms,
                );
                eprintln!(
                    "         loop={} ls={:?} le={:?} | adpcm: {}blk max_err={} rms={:.0} snr={:.1}dB",
                    has_loop, loop_start_sample, loop_end_sample,
                    num_blocks, rt_max_err, rt_rms, snr_db,
                );
                eprintln!(
                    "         vol={} atten={:.1} | env: a={:.4} d={:.4} s={:.1} r={:.4}",
                    default_volume, attenuation_db, sf2_atk, sf2_dec, sf2_sus, sf2_rel,
                );
                eprintln!(
                    "         pitch: @root=0x{:04X} @+12=0x{:04X} @-12=0x{:04X} ratio={}",
                    p_root, p_up12, p_dn12,
                    if p_root > 0 { format!("{:.4}", p_up12 as f64 / p_root as f64) } else { "N/A".to_string() },
                );
                eprintln!(
                    "         first4pcm={:?} sid={} root_key_raw={}",
                    first4, region.get_sample_id(),
                    region.get_root_key(),
                );
            }

            bank.regions.push(sample_region);

            library.sample_count += 1;
        }
    }

    if spu_ram_full && bank.regions.is_empty() {
        return false;
    }

    bank.regions.sort_by_key(|r| r.key_lo);

    if !bank.regions.is_empty() {
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!(
            "SPU convert: loaded program {} '{}' — {} regions, {:.0}KB used",
            program, bank.name, bank.regions.len(),
            library.spu_ram.allocated_bytes() as f64 / 1024.0,
        );
        library.instruments.push(bank);
        true
    } else {
        false
    }
}

/// Resample a cyclical loop to a new length using linear interpolation.
///
/// Treats `data` as one complete cycle of a periodic waveform, so the
/// interpolation wraps from the last sample back to the first.
fn resample_loop_linear(data: &[i16], target_len: usize) -> Vec<i16> {
    let src_len = data.len();
    if src_len == 0 || target_len == 0 {
        return vec![0i16; target_len];
    }
    let mut result = Vec::with_capacity(target_len);
    for i in 0..target_len {
        let src_pos = i as f64 * src_len as f64 / target_len as f64;
        let idx0 = src_pos.floor() as usize % src_len;
        let idx1 = (idx0 + 1) % src_len;
        let frac = src_pos - src_pos.floor();
        let sample = data[idx0] as f64 * (1.0 - frac) + data[idx1] as f64 * frac;
        result.push(sample.round().clamp(-32768.0, 32767.0) as i16);
    }
    result
}

/// Align loop points to ADPCM block boundaries (multiples of 28 samples).
///
/// The PS1 SPU can only loop on ADPCM block boundaries (28 samples per block).
/// If loop_start or loop_len aren't multiples of 28, the actual loop in the
/// encoded ADPCM will span more samples than intended, changing the pitch.
///
/// Earlier approaches tried repeating the loop content LCM(loop_len, 28) / loop_len
/// times to reach exact block alignment. This fails because ADPCM encoding is
/// stateful — each repetition of the loop gets different block alignment, producing
/// different decoded output. For short loops (e.g., 200 samples), this doubles the
/// effective period and drops the note an octave.
///
/// Instead, this function **resamples** the loop to the nearest multiple of 28
/// using linear interpolation. The waveform shape is preserved (the length change
/// is typically <5%), and a pitch correction factor compensates for the slight
/// length difference. This guarantees every loop iteration decodes identically.
///
/// Returns (new_pcm, aligned_loop_start, aligned_loop_end, pitch_correction).
/// The pitch_correction factor should be multiplied into base_pitch.
fn align_loop_to_adpcm_blocks(
    pcm: &[i16],
    loop_start: usize,
    loop_end: usize,
) -> (Vec<i16>, usize, usize, f64) {
    let loop_len = loop_end.saturating_sub(loop_start);
    if loop_len == 0 {
        return (pcm.to_vec(), loop_start, loop_end, 1.0);
    }

    let blk = SAMPLES_PER_ADPCM_BLOCK; // 28

    // Already perfectly aligned? No work needed.
    if loop_start % blk == 0 && loop_len % blk == 0 {
        return (pcm.to_vec(), loop_start, loop_end, 1.0);
    }

    // Step 1: Round loop_start UP to next block boundary.
    // The few original loop samples between loop_start and aligned_start
    // become non-looping pre-loop content (played once during the attack).
    let aligned_start = ((loop_start + blk - 1) / blk) * blk;
    let phase_offset = aligned_start - loop_start;

    // Step 2: Find nearest multiple of 28 for the loop length.
    let nearest_down = (loop_len / blk) * blk;
    let nearest_up = nearest_down + blk;
    let target_len = if nearest_down > 0
        && (loop_len - nearest_down) <= (nearest_up - loop_len)
    {
        nearest_down
    } else {
        nearest_up
    };

    // Pitch correction: the resampled loop has target_len samples per cycle
    // instead of loop_len. Multiply base_pitch by this to maintain correct
    // playback frequency.
    let pitch_correction = target_len as f64 / loop_len as f64;

    let aligned_end = aligned_start + target_len;

    #[cfg(not(target_arch = "wasm32"))]
    eprintln!(
        "    align_loop: {}..{} (len={}) → {}..{} (resample {}→{}, {:.2}%, {} blocks)",
        loop_start, loop_end, loop_len,
        aligned_start, aligned_end, loop_len, target_len,
        (pitch_correction - 1.0) * 100.0,
        target_len / blk,
    );

    // Step 3: Extract phase-shifted loop content.
    // Starting at phase_offset ensures seamless continuity with the pre-loop
    // (sample at aligned_start-1 is loop[phase_offset-1], first loop sample
    // is loop[phase_offset], which are consecutive in the original waveform).
    let loop_data = &pcm[loop_start..loop_end.min(pcm.len())];
    let phase_shifted: Vec<i16> = (0..loop_len)
        .map(|i| loop_data[(phase_offset + i) % loop_len])
        .collect();

    // Step 4: Resample to target_len (nearest multiple of 28).
    let resampled = if target_len == loop_len {
        phase_shifted
    } else {
        resample_loop_linear(&phase_shifted, target_len)
    };

    // Step 5: Build new PCM data.
    let mut new_pcm = Vec::with_capacity(aligned_end);

    // Pre-loop: keep original data up to aligned_start.
    for i in 0..aligned_start {
        if i < pcm.len() {
            new_pcm.push(pcm[i]);
        } else {
            let offset_in_loop = (i - loop_start) % loop_len;
            new_pcm.push(pcm[loop_start + offset_in_loop]);
        }
    }

    // Loop region: append resampled loop.
    new_pcm.extend_from_slice(&resampled);

    (new_pcm, aligned_start, aligned_end, pitch_correction)
}

/// Replace the decaying pre-loop portion with repeating loop content.
///
/// Plucked/struck instruments (bass, guitar, etc.) have a natural amplitude
/// decay in their pre-loop region that causes a "falling tone" effect during
/// playback. We fix this by preserving only the initial attack transient
/// (~4ms), then crossfading to the loop content. The rest of the pre-loop
/// is filled with copies of the loop, so the note stabilizes immediately
/// after the attack.
fn normalize_pre_loop_amplitude(pcm: &mut [i16], loop_start: usize, loop_end: usize) {
    let le = loop_end.min(pcm.len());
    if loop_start >= le {
        return;
    }
    let loop_len = le - loop_start;
    if loop_len == 0 {
        return;
    }

    // Preserve initial attack transient (~4ms at 44100Hz)
    let attack_preserve = 176.min(loop_start);
    // Crossfade duration (~2ms)
    let crossfade_len = 88.min(loop_start.saturating_sub(attack_preserve));

    if loop_start <= attack_preserve + crossfade_len {
        return; // Pre-loop too short, nothing to do
    }

    // Check if normalization is needed: compare pre-loop RMS to loop RMS
    let loop_rms: f64 = {
        let sum: f64 = pcm[loop_start..le]
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();
        (sum / loop_len as f64).sqrt()
    };
    let pre_rms: f64 = {
        let pre_start = attack_preserve + crossfade_len;
        let pre_len = loop_start - pre_start;
        if pre_len == 0 {
            return;
        }
        let sum: f64 = pcm[pre_start..loop_start]
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();
        (sum / pre_len as f64).sqrt()
    };

    // Only apply if the pre-loop differs significantly from the loop
    if pre_rms < loop_rms * 1.5 && pre_rms > loop_rms * 0.67 {
        return; // Close enough already
    }

    #[cfg(not(target_arch = "wasm32"))]
    eprintln!(
        "    normalize: pre_rms={:.0} loop_rms={:.0} replacing pre-loop with loop content",
        pre_rms, loop_rms,
    );

    // Step 1: Fill everything after attack+crossfade with repeating loop content
    // (reads from loop_start..le which we don't modify here)
    let fill_start = attack_preserve + crossfade_len;
    for i in fill_start..loop_start {
        let loop_pos = (i - fill_start) % loop_len;
        pcm[i] = pcm[loop_start + loop_pos];
    }

    // Step 2: Crossfade from original attack to the loop content
    for i in 0..crossfade_len {
        let pos = attack_preserve + i;
        if pos >= loop_start {
            break;
        }
        let t = i as f64 / crossfade_len as f64;
        let original = pcm[pos] as f64;
        let loop_pos = i % loop_len;
        let loop_sample = pcm[loop_start + loop_pos] as f64;
        pcm[pos] = (original * (1.0 - t) + loop_sample * t)
            .clamp(-32768.0, 32767.0) as i16;
    }
}

/// Convert SF2 volume envelope parameters to PS1 ADSR register values
///
/// SF2 envelope times are in seconds (float). PS1 ADSR uses shift/step
/// register values that control counter-based timing.
///
/// This is necessarily approximate — the PS1 ADSR model is quite different
/// from SF2's. We map to the closest PS1 behavior.
fn sf2_envelope_to_adsr(region: &rustysynth::InstrumentRegion, has_loop: bool) -> AdsrParams {
    // SF2 envelope times (in seconds as multiplying factors)
    let attack_time = region.get_attack_volume_envelope();
    let decay_time = region.get_decay_volume_envelope();
    // NOTE: rustysynth returns decibels (raw centibels * 0.1), not centibels!
    let sustain_db = region.get_sustain_volume_envelope(); // in dB (decibels)
    let release_time = region.get_release_volume_envelope();

    // Convert attack time to PS1 rate (0-127, lower = faster)
    let (attack_shift, attack_step) = time_to_rate(attack_time, false);

    // Attack mode: use exponential for longer attacks
    let attack_exp = attack_time > 0.1;

    // For looped instruments, cap the decay time so the note reaches its
    // sustain level quickly. SF2 instruments like bass have 10+ second decays
    // (modeling natural string decay), but for PS1/tracker playback, looped
    // samples should stabilize fast. Non-looped samples (piano, percussion)
    // keep their natural decay since the sample ends on its own.
    let effective_decay_time = if has_loop {
        decay_time.min(0.3)
    } else {
        decay_time
    };

    // Convert decay time to PS1 decay shift (0-15)
    let decay_shift = time_to_decay_rate(effective_decay_time);

    // Convert sustain level (rustysynth returns decibels of attenuation)
    // 0 dB = full volume, 100 dB = essentially silent
    // PS1 sustain_level 0-15: level = (sustain_level + 1) * 0x800
    let sustain_ratio = if sustain_db <= 0.0 {
        1.0
    } else {
        10.0_f32.powf(-sustain_db / 20.0) // Convert decibels to linear amplitude
    };
    let sustain_level_raw = ((sustain_ratio * 15.0).round() as u8).min(15);

    // For looped instruments, enforce a minimum sustain level so the ADSR
    // contributes minimal volume drop. Level 14 → target 0x7800 (94% of max).
    // Combined with pre-loop amplitude normalization, this keeps notes at
    // a consistent volume throughout playback.
    let sustain_level = if has_loop {
        sustain_level_raw.max(14)
    } else {
        sustain_level_raw
    };

    // Convert release time to PS1 release shift (0-31)
    let release_shift = time_to_release_rate(release_time);
    let release_exp = true; // Most PS1 games use exponential release

    // Sustain phase behavior:
    // SF2 sustain model: always hold at the sustain level. During the sustain
    // phase, the volume stays constant until key-off triggers release. SF2
    // instruments achieve natural decay through the decay phase (which brings
    // the volume from peak down to the sustain level). We never use
    // sustain_decrease for SF2 conversion because it would cause the volume
    // to drain toward zero during sustained notes, producing a "note goes
    // down" effect that doesn't match SF2 behavior.
    let (sustain_decrease, sustain_exp, sustain_shift, sustain_step) =
        (false, false, 31u8, 0u8);

    AdsrParams {
        attack_exp,
        attack_shift,
        attack_step,
        decay_shift,
        sustain_level,
        sustain_exp,
        sustain_decrease,
        sustain_shift: sustain_shift.min(31),
        sustain_step: sustain_step.min(3),
        release_exp,
        release_shift,
    }
}

/// Base time for PS1 envelope at rate 0 (fastest):
/// At rate 0, step = 7 << 11 = 14336, counter fires every tick.
/// Linear traversal of full range (32767): 32767/14336 ticks = 2.285 ticks.
/// In seconds: 2.285 / 44100 ≈ 0.0000518s
///
/// The PS1 ADSR doubles the envelope time every 4 rate units:
///   time(rate) ≈ BASE_ENV_TIME * 2^(rate / 4)
///
/// Therefore: rate = 4 * log2(time / BASE_ENV_TIME)
const BASE_ENV_TIME: f64 = 32767.0 / 14336.0 / 44100.0;

/// Convert time (seconds) to PS1 ADSR rate (0-127)
///
/// The PS1 ADSR doubles the envelope time every 4 rate units.
/// rate = 4 * log2(time / BASE_ENV_TIME)
fn seconds_to_rate(time: f32) -> u8 {
    if time <= 0.0 {
        return 0;
    }
    let rate = 4.0 * (time as f64 / BASE_ENV_TIME).log2();
    (rate.round() as i32).clamp(0, 127) as u8
}

/// Convert an envelope time (seconds) to PS1 attack rate (shift, step)
/// PS1 rate = shift * 4 + step (0-127), lower = faster
fn time_to_rate(time: f32, _decreasing: bool) -> (u8, u8) {
    let rate = seconds_to_rate(time);
    let shift = rate / 4;
    let step = rate % 4;
    (shift, step)
}

/// Convert decay time to PS1 decay shift (0-15)
/// Decay only has a 4-bit shift (0-15), effective rate = shift * 4 (0-60)
fn time_to_decay_rate(time: f32) -> u8 {
    let rate = seconds_to_rate(time);
    // Decay rate = shift * 4, shift is 0-15
    (rate / 4).min(15)
}

/// Convert release time to PS1 release shift (0-31)
/// Release only has a 5-bit shift (0-31), effective rate = shift * 4 (0-124)
fn time_to_release_rate(time: f32) -> u8 {
    let rate = seconds_to_rate(time);
    // Release rate = shift * 4, shift is 0-31
    (rate / 4).min(31)
}

/// Standard GM instrument names (bank 0, programs 0-127)
pub const GM_NAMES: [&str; 128] = [
    "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano",
    "Honky-tonk Piano", "Electric Piano 1", "Electric Piano 2", "Harpsichord",
    "Clavinet", "Celesta", "Glockenspiel", "Music Box", "Vibraphone",
    "Marimba", "Xylophone", "Tubular Bells", "Dulcimer", "Drawbar Organ",
    "Percussive Organ", "Rock Organ", "Church Organ", "Reed Organ",
    "Accordion", "Harmonica", "Tango Accordion", "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)", "Electric Guitar (jazz)", "Electric Guitar (clean)",
    "Electric Guitar (muted)", "Overdriven Guitar", "Distortion Guitar",
    "Guitar Harmonics", "Acoustic Bass", "Electric Bass (finger)",
    "Electric Bass (pick)", "Fretless Bass", "Slap Bass 1", "Slap Bass 2",
    "Synth Bass 1", "Synth Bass 2", "Violin", "Viola", "Cello", "Contrabass",
    "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
    "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2",
    "Choir Aahs", "Voice Oohs", "Synth Voice", "Orchestra Hit", "Trumpet",
    "Trombone", "Tuba", "Muted Trumpet", "French Horn", "Brass Section",
    "Synth Brass 1", "Synth Brass 2", "Soprano Sax", "Alto Sax", "Tenor Sax",
    "Baritone Sax", "Oboe", "English Horn", "Bassoon", "Clarinet", "Piccolo",
    "Flute", "Recorder", "Pan Flute", "Blown Bottle", "Shakuhachi", "Whistle",
    "Ocarina", "Lead 1 (square)", "Lead 2 (sawtooth)", "Lead 3 (calliope)",
    "Lead 4 (chiff)", "Lead 5 (charang)", "Lead 6 (voice)", "Lead 7 (fifths)",
    "Lead 8 (bass + lead)", "Pad 1 (new age)", "Pad 2 (warm)", "Pad 3 (polysynth)",
    "Pad 4 (choir)", "Pad 5 (bowed)", "Pad 6 (metallic)", "Pad 7 (halo)",
    "Pad 8 (sweep)", "FX 1 (rain)", "FX 2 (soundtrack)", "FX 3 (crystal)",
    "FX 4 (atmosphere)", "FX 5 (brightness)", "FX 6 (goblins)", "FX 7 (echoes)",
    "FX 8 (sci-fi)", "Sitar", "Banjo", "Shamisen", "Koto", "Kalimba",
    "Bagpipe", "Fiddle", "Shanai", "Tinkle Bell", "Agogo", "Steel Drums",
    "Woodblock", "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
    "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet",
    "Telephone Ring", "Helicopter", "Applause", "Gunshot",
];
