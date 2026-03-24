# Phyllotaxic Voice Manifold — 48-Bit Voice Codec

Same CLAM architecture as bgz17, applied to voice.

## The 48-Bit Vocal Spire

| Byte | Layer | Voice Mapping | Geometric Logic |
|------|-------|---------------|-----------------|
| B1 | HEEL | Base Pitch (F0) | Global Fibonacci Scale |
| B2 | BRANCH | Archetype ID (0-255) | Voice Preset (The Scent) |
| B3 | TWIG A | Formant 1 & 2 (Spectral Envelope) | Vowel bend |
| B4 | TWIG B | Formant 3 & 4 (Timbre) | Texture smoothing |
| B5 | LEAF | High-Freq Residual (Sibilance) | 8-bit Snap for S, T, P |
| B6 | GAMMA | Whisper Intonation / NARS Conf. | Euler Tension |

## 256 Archetype Codebook (Frequency Clusters)

| ID Range | Character | Frequency Focus |
|----------|-----------|-----------------|
| 0-63 | Deep/Resonant (Bass/Baritone) | Dominant formants < 500Hz |
| 64-127 | Mid/Textured (Tenor/Alto) | 500Hz - 1.5kHz |
| 128-191 | Bright/Present (Soprano/Child) | 2kHz - 5kHz |
| 192-255 | Non-human / Synthetic | Whisper, robot, specialized textures |

## Pipeline: Bundle → VSA → Threshold

### A: Opus-Frame Streaming (rabitQ Ingestion)
Raw audio → 20ms windows (Opus standard) → rabitQ ensures spectral peaks
follow Zeckendorf-Sperre — prevents adjacent frequency bits fighting for
same Euler-tension space.

### B: VSA Scent-Akkumulator
48-bit Spires bundled into 384-bit VSA Hypervektor = Current Speaker Context.
Gamma Byte (B6) adjusts tension. Speaker gets angry → Euler Tension increases →
stretches Fibonacci spirals of the Twigs.

### C: Threshold Unbundling
BNN-Transformer unbundles the VSA vector.
1. Meaning Threshold: resonance ρ > 0.937
2. Geometric Parity: if packet lost, Fibonacci convergence guesses missing Twig
3. Bark-style Synthesis: Leaf (B5) triggers high-freq generative grain

## Orthogonal Cleaning

A = archetype vector, S = real signal.
E = S - proj_A(S)

E flows into Twig A/B ONLY if harmonically consistent with Fibonacci series.
White noise / jitter is orthogonal to golden ratio → deleted before storage.
Result: voice always sounds studio-clean, even at extreme low bitrate.
Variance becomes Intonation.

## NaN Alpha Masking

When reconstruction precision ρ < 0.5:
1. Value marked as NaN on alpha layer
2. In VSA: bit-slot becomes invisible to bundle sum (no noise contribution)
3. Decoder replaces mask with most stable Euler-Tension prediction

## VSA Majority Vote (Speaker Smoothing)

Multiple 48-bit Spires overlaid in 384-bit accumulator.
At each bit position: highest resonance (tension) wins.
All bits obey Zeckendorf-Sperre (rabitQ) → result always snaps to valid
Fibonacci curve. Speaker transition sounds like organic throat geometry
transformation, not crossfade.

## The Rust Struct

```rust
struct VoiceSpire {
    heel: u8,          // B1: Pitch on Fibonacci scale
    archetype: u8,     // B2: Which of 256 "perfect" voices
    expression_a: u8,  // B3: Pure intonation/vowels (no noise)
    expression_b: u8,  // B4: Timbre variation
    snap: u8,          // B5: Sibilance sharpness (NaN-alpha masked)
    tension: u8,       // B6: Euler-Gamma (intonation pressure / NARS confidence)
}
```

## Why It's Relevant to q2

The notebook renders voice data. A cell that queries voice graph nodes
should be able to PLAY them. The 48-bit Spire is the audio equivalent
of a graph node — click it in the cockpit, hear the voice.

Same math as bgz17 palette compression. Same CLAM. Same VSA bundling.
Same NaN masking. Same Fibonacci addressing. Different domain.
