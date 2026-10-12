//! Standard-decoder seed view (PORT_SPEC.md section 10; evaluate.seed_view): reported separately, never folded into
//! the score, uncalibrated. The table (seed_scores.npy, float32 [65536, 2]: badness, percentile 0 best .. 100 worst)
//! is embedded.
use serde::Serialize;
use std::sync::OnceLock;

static NPY: &[u8] = include_bytes!("../data/seed_scores.npy");
const TPL_SAMPLES: i64 = 73 * 82;

fn table() -> &'static Vec<[f32; 2]> {
    static T: OnceLock<Vec<[f32; 2]>> = OnceLock::new();
    T.get_or_init(|| {
        // .npy v1: magic (6) + version (2) + header length (2, little endian) + header; then raw little-endian f32
        let hl = u16::from_le_bytes([NPY[8], NPY[9]]) as usize;
        let data = &NPY[10 + hl..];
        assert_eq!(data.len(), 65536 * 2 * 4, "seed_scores.npy size");
        (0..65536).map(|i| {
            let f = |j: usize| f32::from_le_bytes(data[(i * 2 + j) * 4..(i * 2 + j) * 4 + 4].try_into().unwrap());
            [f(0), f(1)]
        }).collect()
    })
}

fn lfsr() -> &'static Vec<i64> {
    static T: OnceLock<Vec<i64>> = OnceLock::new();
    T.get_or_init(|| {
        let mut pos = vec![-1i64; 65536];
        let mut r: u32 = 1;
        for i in 0..65535 {
            pos[r as usize] = i;
            let bit = (r ^ (r >> 1) ^ (r >> 3) ^ (r >> 12)) & 1;
            r = (r >> 1) | (bit << 15);
        }
        pos
    })
}

pub fn stripe_degenerate(seed: u32, height: usize) -> bool {
    seed == 0 || (0..height.div_ceil(32) as u32).any(|n| (seed ^ (((n * 37 + 178) & 255) << 8) ^ ((n * 173 + 105) & 255)) == 0)
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct SeedView {
    pub seed_frames: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_pct_mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_pct_p90: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_pct_worst: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_bad_share: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_degenerate: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_overlap_share: Option<f64>,
}

fn round(x: f64, d: i32) -> f64 {
    let m = 10f64.powi(d);
    (x * m).round() / m
}

/// numpy.percentile (linear) of a sorted copy
fn percentile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = q / 100.0 * (s.len() as f64 - 1.0);
    let lo = idx.floor() as usize;
    let hi = (lo + 1).min(s.len() - 1);
    s[lo] + (s[hi] - s[lo]) * (idx - lo as f64)
}

/// seeds: grain seed per shown frame (None = no grain on that frame); height: the candidate's frame height
pub fn seed_view(seeds: &[Option<u32>], height: usize) -> SeedView {
    let fr: Vec<u32> = seeds.iter().flatten().copied().collect();
    if fr.is_empty() {
        return SeedView::default();
    }
    let tab = table();
    let pct: Vec<f64> = fr.iter().map(|&s| if stripe_degenerate(s, height) { 100.0 } else { tab[s as usize][1] as f64 }).collect();
    let pos: Vec<i64> = fr.iter().map(|&s| lfsr()[s as usize]).collect();
    let mut over = 0;
    for j in 0..pos.len() {
        for k in j.saturating_sub(3)..j {
            let mut dp = (pos[j] - pos[k]).abs();
            dp = dp.min(65535 - dp);
            if pos[j] >= 0 && pos[k] >= 0 && (dp as f64) < TPL_SAMPLES as f64 / 2.0 {
                over += 1;
                break;
            }
        }
    }
    let n = fr.len() as f64;
    SeedView {
        seed_frames: fr.len(),
        seed_pct_mean: Some(round(pct.iter().sum::<f64>() / n, 2)),
        seed_pct_p90: Some(round(percentile(&pct, 90.0), 2)),
        seed_pct_worst: Some(round(pct.iter().cloned().fold(f64::MIN, f64::max), 2)),
        seed_bad_share: Some(round(pct.iter().filter(|&&p| p > 90.0).count() as f64 / n, 3)),
        seed_degenerate: Some(fr.iter().filter(|&&s| stripe_degenerate(s, height)).count()),
        seed_overlap_share: Some(round(over as f64 / n, 3)),
    }
}
