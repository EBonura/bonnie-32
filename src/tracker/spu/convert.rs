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

use rustysynth::{SoundFont, LoopMode};
use super::adpcm;
use super::types::{
    AdsrParams, SampleRegion, InstrumentBank, SampleLibrary,
};
use super::tables::NATIVE_PITCH;

/// Convert an SF2 soundfont to a PS1 SPU sample library
///
/// Extracts all GM bank 0 presets (programs 0-127), encodes their
/// PCM samples to ADPCM, and stores them in virtual SPU RAM.
pub fn convert_sf2_to_spu(sf2_data: &[u8], name: &str) -> Result<SampleLibrary, String> {
    let mut cursor = std::io::Cursor::new(sf2_data);
    let soundfont = SoundFont::new(&mut cursor)
        .map_err(|e| format!("Failed to parse SF2: {:?}", e))?;

    let wave_data = soundfont.get_wave_data();
    let sample_headers = soundfont.get_sample_headers();
    let presets = soundfont.get_presets();
    let instruments = soundfont.get_instruments();

    let mut library = SampleLibrary::new(name.to_string());
    let mut total_samples = 0;

    // Process all presets in bank 0 (GM melodic)
    for preset in presets {
        if preset.get_bank_number() != 0 {
            continue;
        }

        let program = preset.get_patch_number() as u8;
        if program > 127 {
            continue;
        }

        let mut bank = InstrumentBank {
            name: preset.get_name().to_string(),
            program,
            regions: Vec::new(),
        };

        // Iterate preset regions → instruments → instrument regions
        for preset_region in preset.get_regions() {
            let inst_id = preset_region.get_instrument_id();
            if inst_id >= instruments.len() {
                continue;
            }
            let instrument = &instruments[inst_id];

            for region in instrument.get_regions() {
                // Get key range
                let key_lo = region.get_key_range_start().max(0).min(127) as u8;
                let key_hi = region.get_key_range_end().max(0).min(127) as u8;
                if key_lo > key_hi {
                    continue;
                }

                // Get sample info
                let sample_start = region.get_sample_start() as usize;
                let sample_end = region.get_sample_end() as usize;
                if sample_start >= sample_end || sample_end > wave_data.len() {
                    continue;
                }

                // Extract PCM samples
                let pcm_data = &wave_data[sample_start..sample_end];
                if pcm_data.is_empty() {
                    continue;
                }

                // Get loop points (relative to sample start)
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

                // Get root key and pitch info
                let root_key = region.get_root_key();
                let base_note = if root_key >= 0 && root_key <= 127 {
                    root_key as u8
                } else {
                    // Fall back to sample header original_pitch
                    let sid = region.get_sample_id();
                    if sid < sample_headers.len() {
                        let op = sample_headers[sid].get_original_pitch();
                        if op >= 0 && op <= 127 { op as u8 } else { 60 }
                    } else {
                        60
                    }
                };

                // Calculate base pitch register value
                // PS1 pitch 0x1000 = 44100Hz playback
                // If sample_rate != 44100, adjust: base_pitch = (sample_rate / 44100) * 0x1000
                let sid = region.get_sample_id();
                let sample_rate = if sid < sample_headers.len() {
                    sample_headers[sid].get_sample_rate() as f64
                } else {
                    44100.0
                };
                let base_pitch = ((sample_rate / 44100.0) * NATIVE_PITCH as f64) as u16;

                // Get fine tuning
                let fine_tune = (region.get_fine_tune()
                    + region.get_coarse_tune() * 100) as i16;

                // Encode PCM → ADPCM
                let adpcm_data = adpcm::encode_pcm_to_adpcm(
                    pcm_data,
                    loop_start_sample,
                    loop_end_sample,
                );

                // Allocate in SPU RAM
                let spu_ram_offset = match library.spu_ram.allocate(&adpcm_data) {
                    Some(offset) => offset,
                    None => {
                        // SPU RAM full — skip remaining samples
                        break;
                    }
                };

                // Calculate loop address in SPU RAM
                let loop_offset = if has_loop {
                    let loop_block = loop_start_sample.unwrap_or(0) / 28;
                    spu_ram_offset + (loop_block as u32 * 16)
                } else {
                    spu_ram_offset
                };

                // Convert SF2 envelope to PS1 ADSR
                let adsr = sf2_envelope_to_adsr(region);

                // Get volume from attenuation
                let attenuation_db = region.get_initial_attenuation();
                let vol_scale = 10.0_f32.powf(-attenuation_db / 20.0);
                let default_volume = (0x3FFF as f32 * vol_scale).min(0x3FFF as f32) as i16;

                bank.regions.push(SampleRegion {
                    spu_ram_offset,
                    loop_offset,
                    has_loop,
                    adpcm_length: adpcm_data.len() as u32,
                    base_note,
                    base_pitch,
                    key_lo,
                    key_hi,
                    adsr,
                    default_volume,
                    fine_tune,
                });

                total_samples += 1;
            }
        }

        // Sort regions by key_lo for efficient lookup
        bank.regions.sort_by_key(|r| r.key_lo);

        if !bank.regions.is_empty() {
            library.instruments.push(bank);
        }
    }

    library.sample_count = total_samples;

    // Log conversion stats
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!(
        "SPU: Converted {} → {} samples, {} instruments, {:.0}KB SPU RAM used",
        name,
        total_samples,
        library.instruments.len(),
        library.spu_ram.allocated_bytes() as f64 / 1024.0,
    );

    Ok(library)
}

/// Convert SF2 volume envelope parameters to PS1 ADSR register values
///
/// SF2 envelope times are in seconds (float). PS1 ADSR uses shift/step
/// register values that control counter-based timing.
///
/// This is necessarily approximate — the PS1 ADSR model is quite different
/// from SF2's. We map to the closest PS1 behavior.
fn sf2_envelope_to_adsr(region: &rustysynth::InstrumentRegion) -> AdsrParams {
    // SF2 envelope times (in seconds as multiplying factors)
    let attack_time = region.get_attack_volume_envelope();
    let decay_time = region.get_decay_volume_envelope();
    let sustain_db = region.get_sustain_volume_envelope(); // in cB (centibels)
    let release_time = region.get_release_volume_envelope();

    // Convert attack time to PS1 rate (0-127, lower = faster)
    let (attack_shift, attack_step) = time_to_rate(attack_time, false);

    // Attack mode: use exponential for longer attacks
    let attack_exp = attack_time > 0.1;

    // Convert decay time to PS1 decay shift (0-15)
    let decay_shift = time_to_decay_rate(decay_time);

    // Convert sustain level (SF2 sustain is in centibels of attenuation)
    // 0 cB = full volume, 1000 cB = -100 dB (essentially silent)
    // PS1 sustain_level 0-15: level = (sustain_level + 1) * 0x800
    let sustain_ratio = if sustain_db <= 0.0 {
        1.0
    } else {
        10.0_f32.powf(-sustain_db / 200.0) // Convert centibels to linear
    };
    let sustain_level = ((sustain_ratio * 15.0).round() as u8).min(15);

    // Convert release time to PS1 release shift (0-31)
    let release_shift = time_to_release_rate(release_time);
    let release_exp = true; // Most PS1 games use exponential release

    // Sustain phase: hold steady by default (no change during sustain)
    AdsrParams {
        attack_exp,
        attack_shift,
        attack_step,
        decay_shift,
        sustain_level,
        sustain_exp: false,
        sustain_decrease: false,
        sustain_shift: 31,  // Slowest rate = effectively no change
        sustain_step: 0,
        release_exp,
        release_shift,
    }
}

/// Convert an envelope time (seconds) to PS1 attack rate (shift, step)
/// PS1 rate = shift * 4 + step (0-127), lower = faster
fn time_to_rate(time: f32, _decreasing: bool) -> (u8, u8) {
    // PS1 rate 0 = instant, rate 127 = very slow
    // Approximate mapping: rate ≈ time * 30 (empirical)
    // At 44100Hz, the fastest attack (rate 0) takes ~0.002s
    // Rate 40 ≈ 0.05s, rate 80 ≈ 0.5s, rate 120 ≈ 5s
    let rate = if time <= 0.0 {
        0
    } else {
        let r = (time.log10().max(-3.0) + 3.0) * 30.0; // Log scale mapping
        (r as u8).min(127)
    };

    let shift = rate / 4;
    let step = rate % 4;
    (shift, step)
}

/// Convert decay time to PS1 decay shift (0-15)
fn time_to_decay_rate(time: f32) -> u8 {
    if time <= 0.0 {
        return 0;
    }
    let rate = (time.log10().max(-3.0) + 3.0) * 4.0;
    (rate as u8).min(15)
}

/// Convert release time to PS1 release shift (0-31)
fn time_to_release_rate(time: f32) -> u8 {
    if time <= 0.0 {
        return 0;
    }
    let rate = (time.log10().max(-3.0) + 3.0) * 8.0;
    (rate as u8).min(31)
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
