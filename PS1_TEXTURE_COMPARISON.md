# PS1 GPU Texture System: Research & Comparison

## PS1 Hardware Specifications

### Texture Color Modes (PS1)
| Mode | Description | Storage | Palette |
|------|-------------|---------|---------|
| **4-bit CLUT** | 16 colors | 4 pixels per 16-bit word | 16x1 = 32 bytes |
| **8-bit CLUT** | 256 colors | 2 pixels per 16-bit word | 256x1 = 512 bytes |
| **15-bit Direct** | 32,768 colors | 1 pixel per 16-bit word | None |

### RGB555 Format (PS1)
```
Bit:   15   14-10   9-5    4-0
       STP   Red    Green  Blue
       (1)   (5)    (5)    (5)
```

- **Bit 15 (STP)**: Semi-Transparency Processing flag
- **0x0000**: Fully transparent (not drawn) - color key transparency
- **0x8000**: Drawable black (STP=1, RGB=0,0,0)

### Semi-Transparency Blending Modes (PS1)
| Mode | Formula | Use Case |
|------|---------|----------|
| **0** | `0.5×B + 0.5×F` | 50/50 blend (water, glass) |
| **1** | `1.0×B + 1.0×F` | Additive (glow, fire, explosions) |
| **2** | `1.0×B - 1.0×F` | Subtractive (shadows) |
| **3** | `1.0×B + 0.25×F` | Subtle glow (fog, particles) |

Where B = Background pixel, F = Foreground pixel. Results clamped to 0-255.

### How Semi-Transparency Works on PS1
1. **Per-Primitive Mode**: The blend mode is set per drawing command (polygon)
2. **Per-Pixel Activation**: Bit 15 (STP) in CLUT entries determines which pixels USE the blend mode
3. **Indexed Textures**: Each palette color can individually enable/disable semi-transparency

---

## Your Current Implementation

### What You Have (Correct)

| Feature | Status | Notes |
|---------|--------|-------|
| **ClutDepth enum** | `Bpp4`, `Bpp8` | Both modes defined |
| **Color15 format** | RGB555 + bit 15 | Correct PS1 layout |
| **STP bit handling** | `is_semi_transparent()` | Bit 15 check works |
| **BlendMode enum** | All 5 modes | Opaque, Average, Add, Subtract, AddQuarter |
| **0x0000 transparency** | ✓ | Correct color-key behavior |
| **0x8000 drawable black** | ✓ | Correctly handled |
| **UserTexture** | Indices + Palette | Correct CLUT structure |

### What's Missing / Needs UI

| Feature | Issue | Fix Needed |
|---------|-------|------------|
| **8-bit CLUT selection** | No UI to create/switch to 8-bit | Add depth toggle in texture editor |
| **Texture blend mode** | No per-texture blend mode | Add blend mode field to UserTexture |
| **Per-palette STP flag** | No UI to set bit 15 per color | Add checkbox per palette entry |

---

## Proposed Changes

### 1. Add CLUT Depth Selector to Texture Editor

**Location**: Texture editor panel (when creating or in properties)

**UI Design**:
```
CLUT Depth: [4-bit (16)] [8-bit (256)]
            ^^selected^^
```

**Behavior**:
- Switching from 8-bit → 4-bit: Remap indices (mod 16), truncate palette
- Switching from 4-bit → 8-bit: Expand palette with grayscale, indices unchanged
- Warn if texture has pixels using indices > 15 when downgrading

### 2. Add Texture Blend Mode

**Add to `UserTexture`**:
```rust
/// Default blend mode for this texture (when STP bit is set)
/// Applies to pixels where palette entry has bit 15 = 1
#[serde(default)]
pub blend_mode: BlendMode,
```

**UI Design** (in texture properties):
```
Blend Mode: [Opaque ▼]
            - Opaque (no blend)
            - Average (50/50)
            - Additive (glow)
            - Subtract (shadow)
            - Add 25% (subtle)
```

**Behavior**:
- Affects ALL semi-transparent pixels in this texture
- Renderer uses texture's blend_mode when pixel's STP=1

### 3. Add Per-Color STP Toggle in Palette

**UI Design** (palette editor):
```
[█] 0: Transparent
[█] 1: #1F1F1F  [☐ Semi]
[█] 2: #3E3E3E  [☑ Semi]  ← This color will blend
[█] 3: #5D5D5D  [☐ Semi]
...
```

**Behavior**:
- Checkbox sets/clears bit 15 of that Color15 entry
- Semi-transparent colors blend using texture's blend mode
- Visual indicator: slight highlight or icon on semi-transparent colors

---

## Implementation Priority

### Phase 1: CLUT Depth Selection (High Priority)
- [ ] Add depth toggle buttons to texture editor header
- [ ] Implement depth conversion functions
- [ ] Update texture creation to support both depths
- [ ] Add depth badge to texture list/browser

### Phase 2: Texture Blend Mode (Medium Priority)
- [ ] Add `blend_mode` field to `UserTexture`
- [ ] Add dropdown in texture properties panel
- [ ] Update renderer to use per-texture blend mode
- [ ] Handle serialization (backward compat with default Opaque)

### Phase 3: Per-Color STP Flag (Low Priority - Enhancement)
- [ ] Add STP checkbox to palette color editor
- [ ] Show visual indicator for semi-transparent palette entries
- [ ] Preview semi-transparency effect in texture canvas

---

## Technical Notes

### Backward Compatibility
- New `blend_mode` field uses `#[serde(default)]` → defaults to `Opaque`
- Existing textures will load correctly with opaque behavior

### Renderer Integration
When rendering a face with a UserTexture:
1. Sample pixel index from texture
2. Look up Color15 from palette
3. If `Color15.is_semi_transparent()` (bit 15 = 1):
   - Use `texture.blend_mode` for blending
4. If `Color15.is_transparent()` (0x0000):
   - Skip pixel entirely

### PS1 Authenticity
This implementation matches PS1 behavior:
- Blend mode set per-primitive (in our case, per-texture)
- STP flag per-palette-entry activates blending for individual pixels
- Same 4 blend formulas as PS1 GPU

---

## Sources
- [psx-spx GPU Specifications](https://psx-spx.consoledev.net/graphicsprocessingunitgpu/)
- [PS1 Developer Wiki - GPU](https://www.psdevwiki.com/ps1/GPU)
- [PlayStation Technical Specifications](https://en.wikipedia.org/wiki/PlayStation_technical_specifications)
- [Textures, TPages and CLUTs Tutorial](http://rsync.irixnet.org/tutorials/pstutorials/chapter1/3-textures.html)
