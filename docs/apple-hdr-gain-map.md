# Apple HDR Gain Map Conversion

This document describes how tenrankai converts Apple HEIC HDR photos with gain maps to AVIF format while preserving HDR metadata.

## Background

iPhone cameras (12 and later) capture photos with embedded HDR gain maps. These gain maps allow compatible displays to show enhanced dynamic range while maintaining backward compatibility with SDR displays.

Apple's proprietary format differs significantly from the ISO 21496-1 standard used by AVIF and Android's Ultra HDR format.

## Apple's HDR Gain Map Format

### Storage

- Gain map stored as auxiliary image with type `urn:com:apple:photo:2020:aux:hdrgainmap`
- Single-channel grayscale image, typically half the resolution of the main image
- 8-bit pixel values encoded with sRGB transfer function
- Pixel value 128 represents neutral (no boost), 255 represents maximum boost

### XMP Metadata

Apple stores minimal metadata in XMP on the auxiliary image:

```xml
<HDRGainMap:HDRGainMapVersion>131072</HDRGainMap:HDRGainMapVersion>
<HDRGainMap:HDRGainMapHeadroom>6.911805</HDRGainMap:HDRGainMapHeadroom>
```

- `HDRGainMapVersion`: Format version (65536 or 131072)
- `HDRGainMapHeadroom`: Maximum brightness boost in linear scale (e.g., 6.91 = ~7x brighter)

### Application Formula

From the [apple-hdr-heic](https://github.com/johncf/apple-hdr-heic) library:

```python
# Decode both images with sRGB EOTF (gamma ~2.4)
sdr_linear = sRGB_EOTF(sdr_image)
gainmap_linear = sRGB_EOTF(gainmap)

# Compute scale factor (1.0 to headroom)
scale = 1.0 + (headroom - 1.0) * gainmap_linear

# Apply to produce HDR
hdr_linear = sdr_linear * scale
```

Key points:
- sRGB EOTF applied to gain map (approximately `pow(value, 2.4)`)
- Linear interpolation between 1.0 and headroom
- Result is a multiplicative scale factor

## ISO 21496-1 Format (AVIF/Ultra HDR)

### Formula

```
recovery = pow(gainmap_normalized, 1.0 / gamma)
log_boost = min + (max - min) * recovery
hdr = sdr * exp2(log_boost)
```

### Key Parameters

| Parameter | Description |
|-----------|-------------|
| `gamma` | Applied as `1/gamma` to decode gain map |
| `min` | Log2 of minimum scale factor |
| `max` | Log2 of maximum scale factor |
| `base_offset` | Offset for SDR values (typically 1/64) |
| `alternate_offset` | Offset for HDR values (typically 1/64) |
| `base_hdr_headroom` | Headroom of base image (0 for SDR) |
| `alternate_hdr_headroom` | Headroom of alternate (HDR) image |

## Conversion Approach

### The Challenge: Different Interpolation Curves

The formulas are mathematically different in the mid-tones:

- **Apple**: `scale = 1 + (headroom - 1) * pow(gainmap, 2.4)` (linear interpolation)
- **ISO**: `scale = exp2(log2(headroom) * pow(gainmap, 2.4))` (exponential interpolation)

At endpoints (gainmap=0 and gainmap=1), both give the same result. But in the midtones, the difference is significant:

| Gainmap Value | Apple Scale | ISO Scale | Difference |
|---------------|-------------|-----------|------------|
| 0.0 | 1.000 | 1.000 | 0% |
| 0.3 | 1.213 | 1.046 | -14% |
| 0.5 | 2.319 | 1.490 | -36% |
| 0.7 | 4.423 | 2.530 | -43% |
| 1.0 | 8.000 | 8.000 | 0% |

*(Example with headroom = 8.0)*

### Our Solution: Lookup Table Transformation

Rather than using a simple linear pixel shift, we pre-transform Apple's gain map pixels using a lookup table (LUT) that makes ISO's formula produce the same scale factors as Apple's formula.

For each input pixel value (0-255 in Apple's encoding where 128=neutral):

1. **Shift** Apple's range: `normalized = (value - 128) / 127` (128→0, 255→1)
2. **Apply sRGB EOTF** to get linear: `linear = sRGB_EOTF(normalized)`
3. **Compute target scale** using Apple's formula: `target = 1 + (headroom - 1) * linear`
4. **Solve for ISO input** that produces the same scale:
   - ISO: `scale = exp2(max * pow(gainmap, 1/gamma))`
   - Rearranging: `gainmap = (log2(target) / max)^gamma`
5. **Store** the result in the LUT

The LUT is built once per image based on its headroom value, then applied efficiently to all pixels.

### ISO Parameter Mapping

| ISO Parameter | Value | Rationale |
|---------------|-------|-----------|
| `gamma` | 1/2.4 ≈ 0.417 | Matches sRGB EOTF; ISO applies 1/gamma so recovery = pow(x, 2.4) |
| `min` | 0.0 | log2(1) = 0, no boost when gain_map = 0 |
| `max` | log2(headroom) | e.g., log2(6.91) ≈ 2.79 for full headroom |
| `base_offset` | 1/64 | Standard value for numerical stability |
| `alternate_offset` | 1/64 | Standard value |
| `base_hdr_headroom` | 0.0 | Base is SDR |
| `alternate_hdr_headroom` | log2(headroom) | Matches max |

## Implementation Notes

### Feature Flags

HEIF support requires both `heif` and `avif` features:

```toml
heif = ["tenrankai-image/heif", "tenrankai-image/avif"]
```

The `avif` feature is required because `GainMapInfo` type is defined in the AVIF module.

### Encoder Configuration

libavif requires `qualityGainMap` to be set for gain map encoding:

```rust
if has_gain_map {
    (*encoder).qualityGainMap = quality as i32;
}
```

Without this, the encoder ignores attached gain maps.

## References

### Specifications

- [ISO 21496-1:2025](https://www.iso.org/standard/86775.html) - Gain map metadata standard
- [Android Ultra HDR](https://developer.android.com/media/platform/hdr-image-format) - Detailed formula documentation

### Implementations

- [apple-hdr-heic](https://github.com/johncf/apple-hdr-heic) - Python library for Apple HDR HEIC decoding
- [heif-hdrgainmap-decode](https://github.com/m13253/heif-hdrgainmap-decode) - Earlier decoder implementation

### Articles

- [Greg Benz - ISO gain maps](https://gregbenzphotography.com/hdr-photos/iso-21496-1-gain-maps-share-hdr-photos/)
- [Greg Benz - Apple HDR updates](https://gregbenzphotography.com/hdr-photos/apple-macos-ios-hdr-iso-gain-map-21496-1/)

### Technical Discussions

- [Extracting HDR Gain Map from iOS photos](https://gist.github.com/kiding/fa4876ab4ddc797e3f18c71b3c2eeb3a)
- [Apple Developer Forums - HDR Gain Map](https://developer.apple.com/forums/thread/709331)
