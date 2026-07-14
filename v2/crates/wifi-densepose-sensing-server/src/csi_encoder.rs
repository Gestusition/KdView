//! ADR-024 / ADR-071 CSI contrastive encoder inference (EXPERIMENTAL).
//!
//! Loads the published `wifi-densepose-pretrained` v2 encoder (an `8 -> 64 -> 128`
//! contrastive network, `csi-embed-v2`) plus its input standardizer, runs a faithful
//! forward pass over the live 8-dim CSI feature vector, and tracks how far the current
//! embedding has drifted from a rolling "calm room" baseline (a novelty / change score).
//!
//! ## Honesty notes (why only the encoder, only for novelty)
//! * The paired **presence head** in that release is a single-class training artifact:
//!   its bias (+8.19) pins the sigmoid output to `[0.989, 1.0]` for *every* input, so it
//!   always reports "present". It is deliberately **not** used here.
//! * Only the **encoder** is used (82.3 % held-out temporal-triplet accuracy), and only
//!   for a *relative* novelty signal — the embedding moves away from the baseline when the
//!   room state changes. This is additive telemetry; it never overrides the heuristic
//!   presence/vitals path.
//! * The live 8-dim feature → encoder-input mapping follows the model card's dimension
//!   table on a best-effort basis. Because novelty is measured *relative to a
//!   self-computed baseline*, it is robust to absolute feature-scale mismatch, but it is
//!   NOT the model card's validated accuracy. Treat the number as experimental.
//!
//! The standardizer (8 means + 8 stds, fp16) is recovered from the first 32 bytes of
//! `csi-embed-v2-int4.bin`; confirmed by dequantizing the int4 encoder that follows it
//! and matching the fp32 weights in `csi-embed-v2.safetensors`.

use std::collections::HashMap;
use std::path::Path;

const D_IN: usize = 8;
const D_HID: usize = 64;
const D_EMB: usize = 128;
const BN_EPS: f32 = 1e-5;

/// Faithful reimplementation of the `csi-embed-v2` `Enc` forward pass.
pub struct CsiEncoder {
    mean: [f32; D_IN],
    std: [f32; D_IN],
    w1: Vec<f32>, // [D_HID, D_IN] row-major
    b1: Vec<f32>, // [D_HID]
    bn1_g: Vec<f32>,
    bn1_b: Vec<f32>,
    bn1_rm: Vec<f32>,
    bn1_rv: Vec<f32>,
    w2: Vec<f32>, // [D_EMB, D_HID] row-major
    b2: Vec<f32>, // [D_EMB]
    bn2_g: Vec<f32>,
    bn2_b: Vec<f32>,
    bn2_rm: Vec<f32>,
    bn2_rv: Vec<f32>,
}

impl CsiEncoder {
    /// Load the encoder from `csi-embed-v2.safetensors` and the standardizer from the
    /// first 32 bytes of `csi-embed-v2-int4.bin` (8 fp16 means + 8 fp16 stds).
    pub fn load(safetensors_path: &Path, int4_path: &Path) -> Result<Self, String> {
        let st_bytes = std::fs::read(safetensors_path)
            .map_err(|e| format!("read {}: {e}", safetensors_path.display()))?;
        let tensors = parse_safetensors(&st_bytes)?;

        let get = |name: &str, expect: usize| -> Result<Vec<f32>, String> {
            let v = tensors
                .get(name)
                .ok_or_else(|| format!("missing tensor `{name}`"))?;
            if v.len() != expect {
                return Err(format!(
                    "tensor `{name}` has {} elems, expected {expect}",
                    v.len()
                ));
            }
            Ok(v.clone())
        };

        let int4 = std::fs::read(int4_path)
            .map_err(|e| format!("read {}: {e}", int4_path.display()))?;
        if int4.len() < 32 {
            return Err(format!(
                "{} too short for a 32-byte standardizer",
                int4_path.display()
            ));
        }
        let mut mean = [0.0f32; D_IN];
        let mut std = [0.0f32; D_IN];
        for i in 0..D_IN {
            mean[i] = f16_to_f32(u16::from_le_bytes([int4[i * 2], int4[i * 2 + 1]]));
            let o = 16 + i * 2;
            std[i] = f16_to_f32(u16::from_le_bytes([int4[o], int4[o + 1]]));
        }

        Ok(Self {
            mean,
            std,
            w1: get("w1.weight", D_HID * D_IN)?,
            b1: get("w1.bias", D_HID)?,
            bn1_g: get("bn1.weight", D_HID)?,
            bn1_b: get("bn1.bias", D_HID)?,
            bn1_rm: get("bn1.running_mean", D_HID)?,
            bn1_rv: get("bn1.running_var", D_HID)?,
            w2: get("w2.weight", D_EMB * D_HID)?,
            b2: get("w2.bias", D_EMB)?,
            bn2_g: get("bn2.weight", D_EMB)?,
            bn2_b: get("bn2.bias", D_EMB)?,
            bn2_rm: get("bn2.running_mean", D_EMB)?,
            bn2_rv: get("bn2.running_var", D_EMB)?,
        })
    }

    /// Run the encoder: standardize → w1 → bn1(eval) → gelu → w2 → bn2(eval) → L2-norm.
    /// Returns a 128-dim unit embedding.
    pub fn embed(&self, feat8: &[f32; D_IN]) -> [f32; D_EMB] {
        // Standardize. Dims whose training std is ~0 (person-count / fall / phase-var
        // were constant in the single-sleeper training capture) contribute nothing —
        // feeding a varying value through a ~0 std would explode the input off-manifold.
        let mut x = [0.0f32; D_IN];
        for i in 0..D_IN {
            x[i] = if self.std[i] > 1e-6 {
                (feat8[i] - self.mean[i]) / self.std[i]
            } else {
                0.0
            };
        }

        // Layer 1: linear → batchnorm(eval) → gelu.
        let mut h = [0.0f32; D_HID];
        for o in 0..D_HID {
            let mut acc = self.b1[o];
            let row = o * D_IN;
            for i in 0..D_IN {
                acc += self.w1[row + i] * x[i];
            }
            let norm = self.bn1_g[o] * (acc - self.bn1_rm[o]) / (self.bn1_rv[o] + BN_EPS).sqrt()
                + self.bn1_b[o];
            h[o] = gelu(norm);
        }

        // Layer 2: linear → batchnorm(eval).
        let mut z = [0.0f32; D_EMB];
        for o in 0..D_EMB {
            let mut acc = self.b2[o];
            let row = o * D_HID;
            for i in 0..D_HID {
                acc += self.w2[row + i] * h[i];
            }
            z[o] = self.bn2_g[o] * (acc - self.bn2_rm[o]) / (self.bn2_rv[o] + BN_EPS).sqrt()
                + self.bn2_b[o];
        }

        // L2-normalize onto the unit hypersphere.
        let mut nrm = 0.0f32;
        for v in &z {
            nrm += v * v;
        }
        let nrm = nrm.sqrt().max(1e-12);
        for v in z.iter_mut() {
            *v /= nrm;
        }
        z
    }
}

/// Rolling "calm room" baseline + cosine-novelty score.
///
/// During warm-up the baseline is the running mean of embeddings (assumes a quiet room at
/// startup, matching the 60 s calibration convention). Afterwards, novelty = `1 - cos(emb,
/// baseline)`; the baseline is only nudged toward the current embedding while things look
/// calm (`novelty < calm_threshold`), so a present person does not pollute the baseline but
/// slow environmental drift is still tracked.
pub struct NoveltyTracker {
    baseline: [f32; D_EMB],
    warmup_remaining: usize,
    warmup_seen: usize,
    calm_threshold: f32,
    alpha: f32,
}

impl NoveltyTracker {
    pub fn new(warmup_frames: usize) -> Self {
        Self {
            baseline: [0.0; D_EMB],
            warmup_remaining: warmup_frames.max(1),
            warmup_seen: 0,
            calm_threshold: 0.12,
            alpha: 0.02,
        }
    }

    /// Feed the current embedding, return novelty in `[0, 1]` (0 during warm-up).
    pub fn update(&mut self, emb: &[f32; D_EMB]) -> f32 {
        if self.warmup_remaining > 0 {
            // Accumulate the running mean of the warm-up embeddings.
            self.warmup_seen += 1;
            let n = self.warmup_seen as f32;
            for i in 0..D_EMB {
                self.baseline[i] += (emb[i] - self.baseline[i]) / n;
            }
            self.warmup_remaining -= 1;
            if self.warmup_remaining == 0 {
                renormalize(&mut self.baseline);
            }
            return 0.0;
        }

        let sim = dot(emb, &self.baseline);
        let novelty = (1.0 - sim).clamp(0.0, 1.0);

        if novelty < self.calm_threshold {
            for i in 0..D_EMB {
                self.baseline[i] = (1.0 - self.alpha) * self.baseline[i] + self.alpha * emb[i];
            }
            renormalize(&mut self.baseline);
        }
        novelty
    }

    pub fn is_warming_up(&self) -> bool {
        self.warmup_remaining > 0
    }
}

fn dot(a: &[f32; D_EMB], b: &[f32; D_EMB]) -> f32 {
    let mut s = 0.0;
    for i in 0..D_EMB {
        s += a[i] * b[i];
    }
    s
}

fn renormalize(v: &mut [f32; D_EMB]) {
    let mut n = 0.0f32;
    for x in v.iter() {
        n += x * x;
    }
    let n = n.sqrt().max(1e-12);
    for x in v.iter_mut() {
        *x /= n;
    }
}

/// Exact (erf-based) GELU matching PyTorch's default `F.gelu`.
fn gelu(x: f32) -> f32 {
    0.5 * x * (1.0 + erf(x / std::f32::consts::SQRT_2))
}

/// Abramowitz & Stegun 7.1.26 erf approximation (max abs error ~1.5e-7).
fn erf(x: f32) -> f32 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

/// Decode an IEEE-754 half (fp16) bit pattern to f32.
fn f16_to_f32(h: u16) -> f32 {
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1f;
    let mant = h & 0x3ff;
    let val = if exp == 0 {
        if mant == 0 {
            0.0
        } else {
            (mant as f32) * 2.0f32.powi(-24) // subnormal
        }
    } else if exp == 0x1f {
        if mant == 0 {
            f32::INFINITY
        } else {
            f32::NAN
        }
    } else {
        (1.0 + mant as f32 / 1024.0) * 2.0f32.powi(exp as i32 - 15)
    };
    if sign == 1 {
        -val
    } else {
        val
    }
}

/// Minimal safetensors reader → `{ name: Vec<f32> }` for F32 tensors (others skipped).
fn parse_safetensors(data: &[u8]) -> Result<HashMap<String, Vec<f32>>, String> {
    if data.len() < 8 {
        return Err("file shorter than 8-byte header length".into());
    }
    let hlen = u64::from_le_bytes(data[0..8].try_into().unwrap()) as usize;
    let hstart: usize = 8;
    let hend = hstart
        .checked_add(hlen)
        .filter(|&e| e <= data.len())
        .ok_or("declared header length exceeds file size")?;

    // Trim trailing space/NUL padding before JSON parse.
    let raw = &data[hstart..hend];
    let mut end = raw.len();
    while end > 0 && matches!(raw[end - 1], b' ' | b'\t' | b'\n' | b'\r' | 0) {
        end -= 1;
    }
    let header: serde_json::Value =
        serde_json::from_slice(&raw[..end]).map_err(|e| format!("header JSON: {e}"))?;
    let obj = header.as_object().ok_or("header is not a JSON object")?;

    let base = hend;
    let mut out = HashMap::new();
    for (name, info) in obj {
        if name == "__metadata__" {
            continue;
        }
        if info.get("dtype").and_then(|d| d.as_str()) != Some("F32") {
            continue;
        }
        let offs = info
            .get("data_offsets")
            .and_then(|o| o.as_array())
            .ok_or_else(|| format!("tensor `{name}` missing data_offsets"))?;
        let a = offs[0].as_u64().unwrap_or(0) as usize;
        let b = offs[1].as_u64().unwrap_or(0) as usize;
        let (s, e) = (base + a, base + b);
        if e > data.len() || (e - s) % 4 != 0 {
            return Err(format!("tensor `{name}` offsets out of range"));
        }
        let vals: Vec<f32> = data[s..e]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        out.insert(name.clone(), vals);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_dir() -> std::path::PathBuf {
        // <crate>/../../../models/wifi-densepose-pretrained
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../models/wifi-densepose-pretrained")
    }

    #[test]
    fn f16_decode_matches_known_values() {
        assert!((f16_to_f32(0x3c00) - 1.0).abs() < 1e-6); // 1.0
        assert!((f16_to_f32(0x0000)).abs() < 1e-6); // 0.0
        assert!((f16_to_f32(0x4000) - 2.0).abs() < 1e-6); // 2.0
    }

    #[test]
    fn gelu_matches_reference_points() {
        assert!((gelu(0.0)).abs() < 1e-6);
        // Exact erf-based GELU: gelu(1) = 0.8413447, gelu(-1) = -0.1586553.
        assert!((gelu(1.0) - 0.8413447).abs() < 1e-4);
        assert!((gelu(-1.0) - (-0.1586553)).abs() < 1e-4);
    }

    #[test]
    fn forward_pass_matches_python_reference() {
        let st = model_dir().join("csi-embed-v2.safetensors");
        let int4 = model_dir().join("csi-embed-v2-int4.bin");
        if !st.exists() || !int4.exists() {
            eprintln!("skipping: model files absent at {}", model_dir().display());
            return;
        }
        let enc = CsiEncoder::load(&st, &int4).expect("load encoder");

        // Recovered standardizer sanity: dims 5 (person-count) & 6 (fall) had zero
        // variance in the single-sleeper training capture.
        assert!(enc.std[5] < 1e-3, "person-count std should be ~0");
        assert!(enc.std[6] < 1e-3, "fall std should be ~0");

        let input = [0.6, 0.15, 0.75, 0.70, 0.55, 1.0, 0.0, 0.64];
        let z = enc.embed(&input);

        // Reference values from the exact-erf Python forward pass.
        let expect = [
            (0, -0.178449f32),
            (1, -0.055552),
            (2, 0.000797),
            (3, 0.034180),
            (4, -0.000849),
            (5, 0.042456),
            (64, -0.019380),
            (127, 0.081123),
        ];
        for (i, e) in expect {
            assert!(
                (z[i] - e).abs() < 2e-3,
                "z[{i}] = {} expected ~{e}",
                z[i]
            );
        }

        // Unit norm.
        let n: f32 = z.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-4, "embedding not unit-norm: {n}");
    }

    #[test]
    fn novelty_zero_during_warmup_then_reacts() {
        let mut nt = NoveltyTracker::new(3);
        let calm = [0.0f32; D_EMB];
        let mut a = calm;
        a[0] = 1.0; // unit-ish
        // warm-up returns 0
        assert_eq!(nt.update(&a), 0.0);
        assert_eq!(nt.update(&a), 0.0);
        assert_eq!(nt.update(&a), 0.0);
        assert!(!nt.is_warming_up());
        // identical embedding → ~0 novelty
        assert!(nt.update(&a) < 1e-4);
        // orthogonal embedding → high novelty
        let mut b = calm;
        b[1] = 1.0;
        assert!(nt.update(&b) > 0.5);
    }
}
