# SPU Loop Alignment & SF2 Conversion Pipeline

## Overview

This document covers the work done on the PS1 SPU engine's SF2-to-ADPCM conversion pipeline, specifically around loop point alignment, ADPCM encoding quality, and pitch accuracy.

## Problem Statement

The PS1 SPU encodes audio as 4-bit ADPCM in 16-byte blocks, each containing 28 samples. Loop points (LOOP_START/LOOP_END flags) can only be placed on block boundaries. When SF2 loop points don't align to 28-sample boundaries, the effective loop length changes, altering the pitch.

### Symptoms observed

1. **Acoustic Bass "note goes down and cracks"** - Bass notes fell in pitch during playback and produced crackling artifacts
2. **Flute "bopping noise"** - Audible click/pop at loop restart points
3. **Pitch errors** - Some instruments playing at wrong frequencies

## Root Causes & Fixes

### 1. ADPCM Block Alignment (convert.rs)

**Problem**: A 200-sample bass loop rounded to 224 samples (8 ADPCM blocks) shifted pitch by 12%.

**First attempt - LCM extension**: Repeated loop content to LCM(loop_len, 28) samples. This preserved exact pitch mathematically, but ADPCM encoding is stateful -- each repetition got different block alignment, producing different decoded output. For the bass, alternate 200-sample segments decoded with opposite polarity, doubling the effective period and dropping the note an octave.

**Final fix - Loop resampling** (`align_loop_to_adpcm_blocks`):
- Resample the loop to the nearest multiple of 28 using linear interpolation
- Apply a pitch correction factor (`target_len / original_len`) to `base_pitch`
- This guarantees every loop iteration decodes identically
- Typical resampling is <5% (e.g., 200 -> 196 samples, -2%)

### 2. ADPCM Filter 0 on Loop Start (adpcm.rs)

**Problem**: When the SPU loops back, the decoder's prev1/prev2 state differs from the initial encode state, causing pops/clicks at loop boundaries.

**Fix**: Force filter 0 (which has zero prediction coefficients) on loop start blocks. This makes the decoded output independent of prev1/prev2, eliminating state mismatch artifacts.

### 3. Pre-Loop Amplitude Normalization (convert.rs)

**Problem**: Plucked/struck instruments (bass, guitar) have natural amplitude decay in their pre-loop region. When the SPU transitions from the decaying pre-loop to the sustained loop, the volume jump causes a "falling tone" effect.

**Fix** (`normalize_pre_loop_amplitude`):
- Preserve only the initial attack transient (~4ms)
- Crossfade (~2ms) to the loop content
- Fill the rest of the pre-loop with copies of the loop
- Only applied when pre-loop RMS differs significantly from loop RMS

### 4. SF2 Envelope Mapping (convert.rs)

**Problem**: SF2 instruments with long decay times (e.g., bass with 10+ second decay) caused the ADSR envelope to keep reducing volume during sustained notes.

**Fix**: For looped instruments, cap decay time to 0.3s and enforce minimum sustain level of 14/15 (~94%). The sustain phase never decreases -- SF2 sustain model holds at the sustain level until key-off.

## Architecture

```
SF2 PCM samples
    |
    v
normalize_pre_loop_amplitude()  -- Fix amplitude discontinuity
    |
    v
align_loop_to_adpcm_blocks()    -- Resample loop to 28-sample boundary
    |                               Returns pitch_correction factor
    v
encode_pcm_to_adpcm()           -- 4-bit ADPCM encoding
    |                               Filter 0 forced on loop start block
    v
SPU RAM allocation               -- 512KB virtual SPU RAM
    |
    v
SampleRegion                     -- base_pitch *= pitch_correction
                                    Loop offset = loop_start_block * 16
```

## Known Issues / Future Work

- **Zero-crossing pitch measurement**: The test's `measure_frequency()` uses zero-crossing detection which is unreliable for harmonically rich instruments (bass shows 3x, trumpet 4x). Autocorrelation confirms correct pitch. Consider switching the test to autocorrelation-based measurement.
- **Bass sound quality**: While pitch is now correct (verified via autocorrelation: C2=64.9Hz, C3=130.5Hz vs expected 65.4/130.8), the user reports the bass still doesn't sound right. The ADPCM encoding SNR for bass is -2.3dB (poor), likely due to the complex waveform being difficult for 4-bit ADPCM. Possible improvements:
  - Try higher-quality resampling (sinc interpolation instead of linear)
  - Investigate whether the bass sample's pre-loop normalization is too aggressive
  - Consider sample-rate-specific encoding strategies
- **Extreme loop resampling**: Some regions have very small loops resampled by large factors (e.g., 10 -> 28 samples = 180% expansion, 12 -> 28 = 133%). These high-ratio resamples with linear interpolation may produce noticeable artifacts. Consider using sinc interpolation for ratios > 50%.
- **Flute loop artifacts**: The filter-0-on-loop-start fix should address the "bopping noise" but hasn't been verified by listening test.

## Test Results (Autocorrelation Pitch Measurement)

| Instrument | Note | Measured | Expected | Error |
|---|---|---|---|---|
| Square Wave | C3 | 130.5 Hz | 130.8 Hz | 0.3% |
| Square Wave | C4 | 260.9 Hz | 261.6 Hz | 0.3% |
| Square Wave | C5 | 525.0 Hz | 523.3 Hz | 0.3% |
| Trumpet | C4 | 262.5 Hz | 261.6 Hz | 0.3% |
| Trumpet | G4 | 390.3 Hz | 392.0 Hz | 0.4% |
| Flute | C4 | 262.5 Hz | 261.6 Hz | 0.3% |
| Flute | C5 | 525.0 Hz | 523.3 Hz | 0.3% |
| Bass | C2 | 64.9 Hz | 65.4 Hz | 0.8% |
| Bass | G2 | 97.8 Hz | 98.0 Hz | 0.2% |
| Bass | C3 | 130.5 Hz | 130.8 Hz | 0.3% |

SPU RAM usage: 412KB / 512KB (with TimGM6mb.sf2 test subset)
