// GRAIL kernels (compiled WITHOUT fast math): the CuPy elementwise / ndimage operations of the Python reference,
// order statistics (radix select), grain features and the line-art path. Plain CUDA C++; hipcc compiles it too.
//
// Float conventions follow the reference where it matters: CuPy evaluates `a - b * c` on arrays as two kernels (the
// product is rounded before the subtraction), so those places use __fmul_rn / __fsub_rn to stop FMA contraction.
// Sums are accumulated in float per block and in double across blocks (the reference sums in float32 over the
// whole frame; the difference is far below every tolerance).
#include "portable.h"

#define IDX                                                     \
    int64_t i = (int64_t)blockIdx.x * blockDim.x + threadIdx.x; \
    if (i >= n) return;

// ------------------------------------------------------------------------------------------------ conversions, crops
extern "C" __global__ void u16_to_f32(const uint16_t* a, float* b, int64_t n) { IDX b[i] = (float)a[i]; }
// planes() of extract.py: astype(float32) / 4 (8-bit units)
extern "C" __global__ void u16_to_f32_q(const uint16_t* a, float* b, int64_t n) { IDX b[i] = (float)a[i] / 4.f; }

// window of a frame whose active area was denoised (extract.paste_window / crop_buf): the denoised pixel where the
// window overlaps the active area, the original window pixel elsewhere. All coordinates even; 4:2:0 planes.
extern "C" __global__ void cut_window(const int16_t* den, int aw, int ah, const uint16_t* orig, int x, int y, int w,
                                      int h, int ax, int ay, uint16_t* out) {
    int64_t n = (int64_t)w * h * 3 / 2;
    IDX int64_t ny = (int64_t)w * h, nc = (int64_t)(w / 2) * (h / 2);
    int k, pw, wx, wy;
    int64_t dbase;
    if (i < ny) {
        k = 1; pw = w; wx = (int)(i % w); wy = (int)(i / w); dbase = 0;
    } else {
        int64_t j = i - ny; int p = (int)(j / nc); j -= p * nc;
        k = 2; pw = w / 2; wx = (int)(j % pw); wy = (int)(j / pw);
        dbase = (int64_t)aw * ah + (int64_t)p * (aw / 2) * (ah / 2);
    }
    int X = x / k + wx - ax / k, Y = y / k + wy - ay / k, AW = aw / k, AH = ah / k;
    if (X >= 0 && Y >= 0 && X < AW && Y < AH) out[i] = (uint16_t)den[dbase + (int64_t)Y * AW + X];
    else out[i] = orig[i];
}

// ------------------------------------------------------------------------------------------------ flowdegrain2 (CuPy)
extern "C" __global__ void ew_absdiff(const float* a, const float* b, float* c, int64_t n) { IDX c[i] = fabsf(a[i] - b[i]); }
extern "C" __global__ void ew_energy(const float* a, const float* b, float* c, int64_t n) {
    IDX float d = a[i] - b[i]; c[i] = d * d;
}
extern "C" __global__ void ew_sub(const float* a, const float* b, float* c, int64_t n) { IDX c[i] = a[i] - b[i]; }
extern "C" __global__ void ew_sqr(const float* a, float* c, int64_t n) { IDX c[i] = a[i] * a[i]; }
extern "C" __global__ void ew_abs(const float* a, float* c, int64_t n) { IDX c[i] = fabsf(a[i]); }
// c = a / b[i % m]  (oy = acc / ws; oc = ac / wc with wc broadcast over both chroma planes)
extern "C" __global__ void ew_div_bc(const float* a, const float* b, float* c, int64_t n, int64_t m) {
    IDX c[i] = a[i] / b[i % m];
}
// motion pyramid base: clip(rint(g / 4), 0, 255)
extern "C" __global__ void ew_q8(const float* g, float* o, int64_t n) {
    IDX float v = rintf(g[i] / 4.f); o[i] = v < 0.f ? 0.f : (v > 255.f ? 255.f : v);
}
extern "C" __global__ void ew_rint(const float* a, float* o, int64_t n) { IDX o[i] = rintf(a[i]); }
// GUIDE: v = max(m2 - m m, 0); a = v / (v + eps); b = m - a m   (eps per plane: eps[i / ps])
extern "C" __global__ void ew_guide(const float* m, const float* m2, const float* eps, float* a, float* b, int64_t n,
                                    int64_t ps) {
    IDX float e = eps[i / ps], mm = m[i];
    float v = fmaxf(m2[i] - mm * mm, 0.f);
    float aa = v / (v + e);
    a[i] = aa; b[i] = mm - aa * mm;
}
// ALPHA: a = sat((fallback - ws^2 / sq) / (fallback - 1))
extern "C" __global__ void ew_alpha(const float* ws, const float* sq, float fallback, float* a, int64_t n) {
    IDX a[i] = fminf(1.f, fmaxf(0.f, (fallback - ws[i] * ws[i] / sq[i]) / (fallback - 1)));
}
// BLEND: y = x + a (ga x + gb - x), alpha broadcast (a[i % am])
extern "C" __global__ void ew_blend(const float* x, const float* a, const float* ga, const float* gb, float* y,
                                    int64_t n, int64_t am) {
    IDX float xx = x[i]; y[i] = xx + a[i % am] * (ga[i] * xx + gb[i] - xx);
}
// FINISH (detail gate) -> 10-bit luma
extern "C" __global__ void ew_finish(const float* y_, const float* lo_, const float* hc_, const float* hs_,
                                     const float* ha_, const float* sq_, const float* ws_, const float* e_,
                                     const float* efp, float c0, float boost, int16_t* out, int64_t n) {
    IDX float y = y_[i], lo = lo_[i], hc = hc_[i], hs = hs_[i], ha = ha_[i], sq = sq_[i], ws = ws_[i], e = e_[i];
    float ef = *efp;
    float cn = sqrtf(sq) / ws;
    float co = fminf(1.f, fmaxf(0.f, (hs / fmaxf(ha, 1e-6f) - cn) / fmaxf(1 - cn, .05f)));
    float tm = fminf(1.f, fmaxf(0.f, (e / fmaxf(ef, 1e-6f) - 3) / 3));
    float gate = fminf(1.f, fmaxf(0.f, (co - c0) / (1 - c0))) * tm;
    float z = lo + gate * (1 + (boost - 1) * tm) * hc + (1 - gate) * (y - lo);
    out[i] = (short)__float2int_rn(fminf(1023.f, fmaxf(0.f, z)));
}
extern "C" __global__ void ew_pack(const float* a, int16_t* b, int64_t n) {
    IDX b[i] = (short)__float2int_rn(fminf(1023.f, fmaxf(0.f, a[i])));
}

// ------------------------------------------------------------------------------------------------ order statistics
// Radix select on order-preserving uint32 keys of floats: 4 passes of 8 bits, state on the device (no host sync).
__device__ __forceinline__ uint32_t fkey(float f) {
    uint32_t u = __float_as_uint(f);
    return (u & 0x80000000u) ? ~u : (u | 0x80000000u);
}
__device__ __forceinline__ float fkey_inv(uint32_t k) {
    uint32_t u = (k & 0x80000000u) ? (k & 0x7fffffffu) : ~k;
    return __uint_as_float(u);
}
// out[p * m + j] = |x[p * ps + j * stride]|  (hp[:, ::stride] of the reference, abs) or the value itself (absv = 0)
extern "C" __global__ void gather_stride(const float* x, float* out, int64_t ps, int64_t stride, int64_t m, int planes,
                                         int absv) {
    int64_t n = m * planes;
    IDX int64_t p = i / m, j = i % m;
    float v = x[p * ps + j * stride];
    out[i] = absv ? fabsf(v) : v;
}
// state: st[0] = key prefix, st[1] = rank still to find inside the prefix; hist: 256 counters (zeroed by sel_init)
extern "C" __global__ void sel_init(uint32_t* st, uint32_t* hist, uint32_t k) {
    int t = threadIdx.x;
    if (t == 0) { st[0] = 0; st[1] = k; }
    for (int b = t; b < 256; b += blockDim.x) hist[b] = 0;
}
extern "C" __global__ void sel_hist(const float* v, int64_t n, const uint32_t* st, uint32_t* hist, int shift) {
    __shared__ uint32_t h[256];
    for (int b = threadIdx.x; b < 256; b += blockDim.x) h[b] = 0;
    __syncthreads();
    uint32_t prefix = st[0];
    uint32_t maskhi = shift >= 24 ? 0u : (0xffffffffu << (shift + 8));
    for (int64_t i = (int64_t)blockIdx.x * blockDim.x + threadIdx.x; i < n; i += (int64_t)gridDim.x * blockDim.x) {
        uint32_t k = fkey(v[i]);
        if ((k & maskhi) == (prefix & maskhi)) atomicAdd(&h[(k >> shift) & 255u], 1u);
    }
    __syncthreads();
    for (int b = threadIdx.x; b < 256; b += blockDim.x)
        if (h[b]) atomicAdd(&hist[b], h[b]);
}
extern "C" __global__ void sel_scan(uint32_t* st, uint32_t* hist, int shift) {
    if (threadIdx.x != 0) return;
    uint32_t k = st[1], c = 0;
    for (int b = 0; b < 256; b++) {
        uint32_t hb = hist[b];
        if (c + hb > k) { st[0] |= ((uint32_t)b << shift); st[1] = k - c; break; }
        c += hb;
    }
    for (int b = 0; b < 256; b++) hist[b] = 0;
}
extern "C" __global__ void sel_value(const uint32_t* st, float* out) {
    if (threadIdx.x == 0) *out = fkey_inv(st[0]);
}
// flowdegrain2 noise(): var = max((1.4826 med)^2, 0.001)
extern "C" __global__ void noise_var(const float* med, float* var, float mult) {
    if (threadIdx.x == 0) { float t = 1.4826f * *med; *var = fmaxf(t * t, 0.001f) * mult; }
}
// cupy quantile, method 'linear', float32 (cupy_percentile_weightnening): lo, hi = sorted values at floor(idx), +1
extern "C" __global__ void quantile_finish(const float* lo, const float* hi, float w, float* out) {
    if (threadIdx.x != 0) return;
    float a = *lo, b = *hi, diff = b - a;
    *out = w < 0.5f ? a + diff * w : b - diff * (1 - w);
}

// ------------------------------------------------------------------------------------------------ ndimage filters
// scipy.ndimage correlate1d, mode "reflect" (d c b a | a b c d), weights wt[0 .. 2r], accumulator T in tap order.
__device__ __forceinline__ int reflect_idx(int i, int n) {
    if (n == 1) return 0;
    while (i < 0 || i >= n) {
        if (i < 0) i = -i - 1;
        if (i >= n) i = 2 * n - i - 1;
    }
    return i;
}
template <typename T>
__device__ void corr_dev(const float* a, float* b, int W, int H, const float* wt, int r, int axis, int64_t i) {
    int x = (int)(i % W), y = (int)((i / W) % H);
    int64_t base = i - (int64_t)y * W - x;
    T s = (T)0;
    if (axis == 0) {
        for (int j = -r; j <= r; j++) s += (T)a[base + (int64_t)reflect_idx(y + j, H) * W + x] * (T)wt[j + r];
    } else {
        const float* row = a + base + (int64_t)y * W;
        for (int j = -r; j <= r; j++) s += (T)row[reflect_idx(x + j, W)] * (T)wt[j + r];
    }
    b[i] = (float)s;
}
// n = W * H * planes
extern "C" __global__ void corr_f(const float* a, float* b, int W, int H, int64_t n, const float* wt, int r, int axis) {
    IDX corr_dev<float>(a, b, W, H, wt, r, axis, i);
}
extern "C" __global__ void corr_d(const float* a, float* b, int W, int H, int64_t n, const float* wt, int r, int axis) {
    IDX corr_dev<double>(a, b, W, H, wt, r, axis, i);
}
// grey dilation / erosion with a square footprint along one axis (reflect mode == clamped window for max / min)
extern "C" __global__ void minmax1d(const float* a, float* b, int W, int H, int r, int axis, int is_max) {
    int64_t n = (int64_t)W * H;
    IDX int x = (int)(i % W), y = (int)(i / W);
    float v = is_max ? -3.4e38f : 3.4e38f;
    for (int j = -r; j <= r; j++) {
        int xx = axis == 1 ? x + j : x, yy = axis == 0 ? y + j : y;
        if (xx < 0 || yy < 0 || xx >= W || yy >= H) continue;
        float u = a[(int64_t)yy * W + xx];
        v = is_max ? fmaxf(v, u) : fminf(v, u);
    }
    b[i] = v;
}

// ------------------------------------------------------------------------------------------------ grain features
// local sd of the low-passed source: loc = sqrt(max(m2 - m1 m1, 0))
extern "C" __global__ void local_sd(const float* m1, const float* m2, float* loc, int64_t n) {
    IDX float a = m1[i];
    loc[i] = sqrtf(fmaxf(__fsub_rn(m2[i], __fmul_rn(a, a)), 0.f));
}
extern "C" __global__ void count_lt(const float* a, float th, int64_t n, unsigned long long* out) {
    __shared__ unsigned int c;
    if (threadIdx.x == 0) c = 0;
    __syncthreads();
    unsigned int k = 0;
    for (int64_t i = (int64_t)blockIdx.x * blockDim.x + threadIdx.x; i < n; i += (int64_t)gridDim.x * blockDim.x)
        k += a[i] < th;
    atomicAdd(&c, k);
    __syncthreads();
    if (threadIdx.x == 0) atomicAdd(out, (unsigned long long)c);
}
// flat mask m = loc < th (th = *thp), luma bin = clip(floor(low / 32), 0, 7)
extern "C" __global__ void flat_mask(const float* loc, const float* low, const float* thp, uint8_t* m, uint8_t* lum,
                                     int64_t n) {
    IDX float th = *thp;
    m[i] = loc[i] < th;
    float k = floorf(low[i] / 32.f);
    lum[i] = (uint8_t)(k < 0.f ? 0.f : (k > 7.f ? 7.f : k));
}
// threshold of the flat mask: 1.2, or min(max(1.2, 1.5 p10), 3.0) when the share of loc < 1.2 is below 3 %
extern "C" __global__ void flat_threshold(const float* p10, int use_p10, float* th) {
    if (threadIdx.x != 0) return;
    *th = use_p10 ? fminf(fmaxf(1.2f, 1.5f * *p10), 3.0f) : 1.2f;
}

// per frame: band powers per luma bin over the flat mask, stick correlations, mask counts.
// acc layout (double): [bin * 16 + j], j: 0 count, 1..5 source bands B0..B4, 6..10 candidate bands, 11..13 the
// grain-free candidate's B0..B2 (grainy-base check); then at 128: stick sums (12) and the joint-mask count, at 141
// the flat count. B0 = r - G0.6, Bi = G_(i-1) - G_i.
#define ACC_STICK 128
#define ACC_FLAT 141
#define ACC_N 144
struct BandPtrs { const float *r, *g0, *g1, *g2, *g3, *g4; };
__device__ __forceinline__ void bands5(const BandPtrs& p, int64_t i, float* B) {
    float r = p.r[i], a = p.g0[i], b = p.g1[i], c = p.g2[i], d = p.g3[i], e = p.g4[i];
    B[0] = r - a; B[1] = a - b; B[2] = b - c; B[3] = c - d; B[4] = d - e;
}
// S / C / NG: residual r and its Gaussians G0.6 .. G9.6 (6 consecutive planes of n floats each; NG may be 0)
extern "C" __global__ void feat_accum(const float* sp, const float* cp, const float* ngp, const uint8_t* m,
                                      const uint8_t* lum, const uint8_t* mprev, const float* prev, float* cur,
                                      int64_t n, double* acc) {
    BandPtrs S = {sp, sp + n, sp + 2 * n, sp + 3 * n, sp + 4 * n, sp + 5 * n};
    BandPtrs C = {cp, cp + n, cp + 2 * n, cp + 3 * n, cp + 4 * n, cp + 5 * n};
    BandPtrs NG = {ngp, ngp + n, ngp + 2 * n, ngp + 3 * n, ngp + 3 * n, ngp + 3 * n};
    int has_ng = ngp != 0;
    // prev / cur: 4 planes of n floats each: (B0+B1) source, (B0+B1) candidate, (B3+B4) source, (B3+B4) candidate
    __shared__ float sh[ACC_N];
    for (int j = threadIdx.x; j < ACC_N; j += blockDim.x) sh[j] = 0.f;
    __syncthreads();
    float st[13];
    for (int j = 0; j < 13; j++) st[j] = 0.f;
    unsigned int flat = 0;
    for (int64_t i = (int64_t)blockIdx.x * blockDim.x + threadIdx.x; i < n; i += (int64_t)gridDim.x * blockDim.x) {
        float bs[5], bc[5];
        bands5(S, i, bs);
        bands5(C, i, bc);
        float fs = bs[0] + bs[1], fc = bc[0] + bc[1], cs = bs[3] + bs[4], cc = bc[3] + bc[4];
        cur[i] = fs; cur[n + i] = fc; cur[2 * n + i] = cs; cur[3 * n + i] = cc;
        uint8_t mi = m[i];
        if (mi) {
            flat++;
            float* a = sh + lum[i] * 16;
            atomicAdd(a, 1.f);
            for (int b = 0; b < 5; b++) { atomicAdd(a + 1 + b, bs[b] * bs[b]); atomicAdd(a + 6 + b, bc[b] * bc[b]); }
            if (has_ng) {
                float bn[5];
                bands5(NG, i, bn);
                for (int b = 0; b < 3; b++) atomicAdd(a + 11 + b, bn[b] * bn[b]);
            }
        }
        if (mprev && mi && mprev[i]) {
            float p0 = prev[i], p1 = prev[n + i], p2 = prev[2 * n + i], p3 = prev[3 * n + i];
            st[0] += p0 * fs; st[1] += p0 * p0; st[2] += fs * fs;     // fine, source
            st[3] += p1 * fc; st[4] += p1 * p1; st[5] += fc * fc;     // fine, candidate
            st[6] += p2 * cs; st[7] += p2 * p2; st[8] += cs * cs;     // coarse, source
            st[9] += p3 * cc; st[10] += p3 * p3; st[11] += cc * cc;   // coarse, candidate
            st[12] += 1.f;
        }
    }
    for (int j = 0; j < 13; j++) atomicAdd(sh + ACC_STICK + j, st[j]);
    atomicAdd(sh + ACC_FLAT, (float)flat);
    __syncthreads();
    for (int j = threadIdx.x; j < ACC_N; j += blockDim.x)
        if (sh[j] != 0.f) atomicAdd(acc + j, (double)sh[j]);
}
// energy of the ORIGINALS in textured 64-px tiles (tex = !m & loc > 3): one block per tile;
// os: s, G0.6, G1.2, G2.4, G4.8 of the source (bands 1..3), oc likewise. acc[0] += vs, acc[1] += vc for tiles with
// >= half the pixels textured.
extern "C" __global__ void tex_tiles(const float* sg0, const float* sg1, const float* sg2, const float* sg3,
                                     const float* cg0, const float* cg1, const float* cg2, const float* cg3,
                                     const uint8_t* m, const float* loc, int W, double* acc) {
    __shared__ float r[3][256];
    int tx = blockIdx.x, ty = blockIdx.y, t = threadIdx.x;
    float vs = 0.f, vc = 0.f, cnt = 0.f;
    for (int k = t; k < 64 * 64; k += blockDim.x) {
        int x = tx * 64 + (k & 63), y = ty * 64 + (k >> 6);
        int64_t i = (int64_t)y * W + x;
        if (!m[i] && loc[i] > 3.0f) {
            cnt += 1.f;
            float a1 = sg0[i] - sg1[i], a2 = sg1[i] - sg2[i], a3 = sg2[i] - sg3[i];
            float b1 = cg0[i] - cg1[i], b2 = cg1[i] - cg2[i], b3 = cg2[i] - cg3[i];
            vs += a1 * a1 + a2 * a2 + a3 * a3;
            vc += b1 * b1 + b2 * b2 + b3 * b3;
        }
    }
    r[0][t] = vs; r[1][t] = vc; r[2][t] = cnt;
    __syncthreads();
    for (int s = blockDim.x / 2; s; s >>= 1) {
        if (t < s) { r[0][t] += r[0][t + s]; r[1][t] += r[1][t + s]; r[2][t] += r[2][t + s]; }
        __syncthreads();
    }
    if (t == 0 && r[2][0] >= 0.5f * 64 * 64) { atomicAdd(acc, (double)r[0][0]); atomicAdd(acc + 1, (double)r[1][0]); }
}
// chroma residual power in flat areas (half resolution): ra = r - G2.4(r) for 4 planes (U, V of the source residual,
// U, V of the candidate residual); mh = m[2y][2x]. acc[0..3] += sum ra^2 mh, acc[4] += sum mh
extern "C" __global__ void chroma_accum(const float* r, const float* gr, const uint8_t* m, int W, int w2, int h2,
                                        double* acc) {
    __shared__ float sh[5];
    if (threadIdx.x < 5) sh[threadIdx.x] = 0.f;
    __syncthreads();
    float a[5] = {0.f, 0.f, 0.f, 0.f, 0.f};
    int64_t n = (int64_t)w2 * h2;
    for (int64_t i = (int64_t)blockIdx.x * blockDim.x + threadIdx.x; i < n; i += (int64_t)gridDim.x * blockDim.x) {
        int x = (int)(i % w2), y = (int)(i / w2);
        if (!m[(int64_t)(2 * y) * W + 2 * x]) continue;
        a[4] += 1.f;
        for (int p = 0; p < 4; p++) { float v = r[p * n + i] - gr[p * n + i]; a[p] += v * v; }
    }
    for (int p = 0; p < 5; p++) atomicAdd(sh + p, a[p]);
    __syncthreads();
    if (threadIdx.x < 5) atomicAdd(acc + threadIdx.x, (double)sh[threadIdx.x]);
}

// ------------------------------------------------------------------------------------------------ line art
// detector on one reference frame (10-bit codes): one thread per full 8x8 cell.
// out[0] += cells with mean act > 6, out[1] += stroke cells (max black top-hat >= 64), out[2] += stroke cells whose
// 25th percentile of act is <= 3, out[3] += cells
__device__ __forceinline__ float act_at(const float* y, int W, int H, int x, int yy) {
    float c = y[(int64_t)yy * W + x];
    float dx = x + 1 < W ? fabsf(y[(int64_t)yy * W + x + 1] - c) : 0.f;
    float dy = yy + 1 < H ? fabsf(y[(int64_t)(yy + 1) * W + x] - c) : 0.f;
    return dx + dy;
}
extern "C" __global__ void lineart_cells(const float* y, const float* closing, int W, int H, unsigned long long* out) {
    int cw = W / 8, ch = H / 8;
    int64_t n = (int64_t)cw * ch;
    IDX int cx = (int)(i % cw), cy = (int)(i / cw);
    float v[64];
    float sum = 0.f, bmx = -3.4e38f;
    for (int k = 0; k < 64; k++) {
        int x = cx * 8 + (k & 7), yy = cy * 8 + (k >> 3);
        float a = act_at(y, W, H, x, yy);
        v[k] = a; sum += a;
        bmx = fmaxf(bmx, closing[(int64_t)yy * W + x] - y[(int64_t)yy * W + x]);
    }
    float mean = sum / 64.f;
    if (mean > 6.f) atomicAdd(out, 1ull);
    if (bmx >= 64.f) {
        atomicAdd(out + 1, 1ull);
        // 25th percentile (cupy linear, float32): sorted v[15], v[16]; weight 0.75 -> v16 - (v16 - v15) * 0.25
        for (int a = 1; a < 64; a++) {          // insertion sort, 64 values
            float t = v[a]; int b = a - 1;
            while (b >= 0 && v[b] > t) { v[b + 1] = v[b]; b--; }
            v[b + 1] = t;
        }
        float w = 0.75f, diff = v[16] - v[15];
        float q25 = w < 0.5f ? v[15] + diff * w : v[16] - diff * (1 - w);
        if (q25 <= 3.0f) atomicAdd(out + 2, 1ull);
    }
    atomicAdd(out + 3, 1ull);
}
// gradient magnitude of the Sobel pair / 8 > EDGE_T  -> edge mask
extern "C" __global__ void edge_mask(const float* sx, const float* sy, uint8_t* e, float th, int64_t n) {
    IDX e[i] = hypotf(sx[i], sy[i]) / 8.f > th;
}
// distance to the nearest edge pixel, exact up to 6 px: column pass (|dy| <= 6, else 7), then row pass -> d^2
extern "C" __global__ void edt_col(const uint8_t* e, uint8_t* g, int W, int H) {
    int64_t n = (int64_t)W * H;
    IDX int x = (int)(i % W), y = (int)(i / W);
    int best = 7;
    for (int d = 0; d <= 6 && d < best; d++) {
        if ((y - d >= 0 && e[(int64_t)(y - d) * W + x]) || (y + d < H && e[(int64_t)(y + d) * W + x])) { best = d; break; }
    }
    g[i] = (uint8_t)best;
}
// ringing next to lines: near = 2 <= d <= 6 & lsd < flat_t; value = max(hc^2 - hs^2, 0) on near pixels, -1 elsewhere
extern "C" __global__ void ring_near(const uint8_t* g, const float* m1, const float* m2, const float* s,
                                     const float* gs, const float* c, const float* gc, int W, int H, float flat_t,
                                     float* val, unsigned long long* cnt) {
    int64_t n = (int64_t)W * H;
    IDX int x = (int)(i % W), y = (int)(i / W);
    int best = 1000;
    for (int dx = -6; dx <= 6; dx++) {
        int xx = x + dx;
        if (xx < 0 || xx >= W) continue;
        int gv = g[(int64_t)y * W + xx];
        if (gv > 6) continue;
        int d2 = dx * dx + gv * gv;
        if (d2 < best) best = d2;
    }
    float a = m1[i];
    float lsd = sqrtf(fmaxf(__fsub_rn(m2[i], __fmul_rn(a, a)), 0.f));
    bool near = best >= 4 && best <= 36 && lsd < flat_t;
    float hs = s[i] - gs[i], hc = c[i] - gc[i];
    float add = fmaxf(__fsub_rn(__fmul_rn(hc, hc), __fmul_rn(hs, hs)), 0.f);
    val[i] = near ? add : -1.f;
    if (near) atomicAdd(cnt, 1ull);
}
// sum and count of values above the threshold (*thp) -> acc[0] (sum), cnt[0]
extern "C" __global__ void sum_above(const float* v, const float* thp, int64_t n, double* acc, unsigned long long* cnt) {
    __shared__ float s;
    __shared__ unsigned int c;
    if (threadIdx.x == 0) { s = 0.f; c = 0; }
    __syncthreads();
    float th = *thp, a = 0.f;
    unsigned int k = 0;
    for (int64_t i = (int64_t)blockIdx.x * blockDim.x + threadIdx.x; i < n; i += (int64_t)gridDim.x * blockDim.x)
        if (v[i] > th) { a += v[i]; k++; }
    atomicAdd(&s, a); atomicAdd(&c, k);
    __syncthreads();
    if (threadIdx.x == 0) { atomicAdd(acc, (double)s); atomicAdd(cnt, (unsigned long long)c); }
}
