// GRAIL kernels in Metal Shading Language (compiled WITHOUT fast math): kernels/grail.cu mapped to MSL. Each kernel
// takes its CUDA argument list as one constant struct (same order, C layout; device pointers inside it).
//
// Differences from the CUDA kernels, all from Apple GPUs having neither float64 nor 64-bit atomics:
// - sums across blocks: each block writes its float partial sums (`part[block * K + j]`) and the host adds them up in
//   double (CUDA adds the same per-block float sums with double atomics);
// - counters are 32-bit (at most one count per pixel of a window frame);
// - corr_d (the reference's float64 Sobel accumulation) accumulates in compensated float-float;
// - per-block float sums are SIMD-group reductions, not shared-memory atomics (another summation order).
// __fmul_rn (a rounded product that must not be contracted into an FMA) is fma(a, b, 0).
#include <metal_stdlib>
using namespace metal;

#define KARGS uint3 blockIdx [[threadgroup_position_in_grid]], uint3 threadIdx [[thread_position_in_threadgroup]], \
              uint3 blockDim [[threads_per_threadgroup]], uint3 gridDim [[threadgroups_per_grid]], \
              uint lane [[thread_index_in_simdgroup]], uint sg [[simdgroup_index_in_threadgroup]]
#define SYNC() threadgroup_barrier(mem_flags::mem_threadgroup)
#define IDX                                                     \
    long i = (long)blockIdx.x * blockDim.x + threadIdx.x;       \
    if (i >= n) return;
typedef device const float* cfp;
typedef device float* fp;
typedef device const uchar* cbp;
typedef device uchar* bp;

static inline float mul_rn(float a, float b) { return fma(a, b, 0.0f); }

// ------------------------------------------------------------------------------------------------ copies (driver)
struct CopyArgs { device uint* d; device const uint* s; long n; };
kernel void copy_u32(constant CopyArgs& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.d[i] = P.s[i]; }
struct CopyBArgs { bp d; cbp s; long n; };
kernel void copy_u8(constant CopyBArgs& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.d[i] = P.s[i]; }
struct FillArgs { bp d; int v; long n; };
kernel void fill_u8(constant FillArgs& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.d[i] = (uchar)P.v; }

// ------------------------------------------------------------------------------------------------ conversions, crops
struct ConvArgs { device const ushort* a; fp b; long n; };
kernel void u16_to_f32(constant ConvArgs& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.b[i] = (float)P.a[i]; }
// planes() of extract.py: astype(float32) / 4 (8-bit units)
kernel void u16_to_f32_q(constant ConvArgs& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.b[i] = (float)P.a[i] / 4.f; }

// window of a frame whose active area was denoised (extract.paste_window / crop_buf)
struct CutArgs { device const short* den; int aw, ah; device const ushort* orig; int x, y, w, h, ax, ay; device ushort* out; };
kernel void cut_window(constant CutArgs& P [[buffer(0)]], KARGS) {
    const int aw = P.aw, ah = P.ah, x = P.x, y = P.y, w = P.w, h = P.h, ax = P.ax, ay = P.ay;
    long n = (long)w * h * 3 / 2;
    IDX long ny = (long)w * h, nc = (long)(w / 2) * (h / 2);
    int k, pw, wx, wy;
    long dbase;
    if (i < ny) {
        k = 1; pw = w; wx = (int)(i % w); wy = (int)(i / w); dbase = 0;
    } else {
        long j = i - ny; int p = (int)(j / nc); j -= p * nc;
        k = 2; pw = w / 2; wx = (int)(j % pw); wy = (int)(j / pw);
        dbase = (long)aw * ah + (long)p * (aw / 2) * (ah / 2);
    }
    int X = x / k + wx - ax / k, Y = y / k + wy - ay / k, AW = aw / k, AH = ah / k;
    if (X >= 0 && Y >= 0 && X < AW && Y < AH) P.out[i] = (ushort)P.den[dbase + (long)Y * AW + X];
    else P.out[i] = P.orig[i];
}

// ------------------------------------------------------------------------------------------------ flowdegrain2 (CuPy)
struct EW2 { cfp a; cfp b; fp c; long n; };
struct EW1 { cfp a; fp c; long n; };
kernel void ew_absdiff(constant EW2& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.c[i] = fabs(P.a[i] - P.b[i]); }
kernel void ew_energy(constant EW2& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX float d = P.a[i] - P.b[i]; P.c[i] = d * d;
}
kernel void ew_sub(constant EW2& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.c[i] = P.a[i] - P.b[i]; }
kernel void ew_sqr(constant EW1& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.c[i] = P.a[i] * P.a[i]; }
kernel void ew_abs(constant EW1& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.c[i] = fabs(P.a[i]); }
// c = a / b[i % m]
struct DivArgs { cfp a; cfp b; fp c; long n; long m; };
kernel void ew_div_bc(constant DivArgs& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.c[i] = P.a[i] / P.b[i % P.m]; }
// motion pyramid base: clip(rint(g / 4), 0, 255)
kernel void ew_q8(constant EW1& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX float v = rint(P.a[i] / 4.f); P.c[i] = v < 0.f ? 0.f : (v > 255.f ? 255.f : v);
}
kernel void ew_rint(constant EW1& P [[buffer(0)]], KARGS) { long n = P.n; IDX P.c[i] = rint(P.a[i]); }
// GUIDE: v = max(m2 - m m, 0); a = v / (v + eps); b = m - a m   (eps per plane: eps[i / ps])
struct GuideArgs { cfp m; cfp m2; cfp eps; fp a; fp b; long n; long ps; };
kernel void ew_guide(constant GuideArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX float e = P.eps[i / P.ps], mm = P.m[i];
    float v = fmax(P.m2[i] - mm * mm, 0.f);
    float aa = v / (v + e);
    P.a[i] = aa; P.b[i] = mm - aa * mm;
}
// ALPHA: a = sat((fallback - ws^2 / sq) / (fallback - 1))
struct AlphaArgs { cfp ws; cfp sq; float fallback; fp a; long n; };
kernel void ew_alpha(constant AlphaArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; const float fb = P.fallback;
    IDX P.a[i] = fmin(1.f, fmax(0.f, (fb - P.ws[i] * P.ws[i] / P.sq[i]) / (fb - 1)));
}
// BLEND: y = x + a (ga x + gb - x), alpha broadcast (a[i % am])
struct BlendArgs { cfp x; cfp a; cfp ga; cfp gb; fp y; long n; long am; };
kernel void ew_blend(constant BlendArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX float xx = P.x[i]; P.y[i] = xx + P.a[i % P.am] * (P.ga[i] * xx + P.gb[i] - xx);
}
// FINISH (detail gate) -> 10-bit luma
struct FinishArgs { cfp y_; cfp lo_; cfp hc_; cfp hs_; cfp ha_; cfp sq_; cfp ws_; cfp e_; cfp efp; float c0, boost;
                    device short* out; long n; };
kernel void ew_finish(constant FinishArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; const float c0 = P.c0, boost = P.boost;
    IDX float y = P.y_[i], lo = P.lo_[i], hc = P.hc_[i], hs = P.hs_[i], ha = P.ha_[i], sq = P.sq_[i], ws = P.ws_[i], e = P.e_[i];
    float ef = *P.efp;
    float cn = sqrt(sq) / ws;
    float co = fmin(1.f, fmax(0.f, (hs / fmax(ha, 1e-6f) - cn) / fmax(1 - cn, .05f)));
    float tm = fmin(1.f, fmax(0.f, (e / fmax(ef, 1e-6f) - 3) / 3));
    float gate = fmin(1.f, fmax(0.f, (co - c0) / (1 - c0))) * tm;
    float z = lo + gate * (1 + (boost - 1) * tm) * hc + (1 - gate) * (y - lo);
    P.out[i] = (short)(int)rint(fmin(1023.f, fmax(0.f, z)));
}
struct PackArgs { cfp a; device short* b; long n; };
kernel void ew_pack(constant PackArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX P.b[i] = (short)(int)rint(fmin(1023.f, fmax(0.f, P.a[i])));
}

// ------------------------------------------------------------------------------------------------ order statistics
// Radix select on order-preserving uint32 keys of floats: 4 passes of 8 bits, state on the device (no host sync).
static inline uint fkey(float f) {
    uint u = as_type<uint>(f);
    return (u & 0x80000000u) ? ~u : (u | 0x80000000u);
}
static inline float fkey_inv(uint k) {
    uint u = (k & 0x80000000u) ? (k & 0x7fffffffu) : ~k;
    return as_type<float>(u);
}
// out[p * m + j] = |x[p * ps + j * stride]|  (hp[:, ::stride] of the reference, abs) or the value itself (absv = 0)
struct GatherArgs { cfp x; fp out; long ps; long stride; long m; int planes, absv; };
kernel void gather_stride(constant GatherArgs& P [[buffer(0)]], KARGS) {
    long n = P.m * P.planes;
    IDX long p = i / P.m, j = i % P.m;
    float v = P.x[p * P.ps + j * P.stride];
    P.out[i] = P.absv ? fabs(v) : v;
}
struct SelInitArgs { device uint* st; device uint* hist; uint k; };
kernel void sel_init(constant SelInitArgs& P [[buffer(0)]], KARGS) {
    uint t = threadIdx.x;
    if (t == 0) { P.st[0] = 0; P.st[1] = P.k; }
    for (uint b = t; b < 256; b += blockDim.x) P.hist[b] = 0;
}
struct SelHistArgs { cfp v; long n; device const uint* st; device atomic_uint* hist; int shift; };
kernel void sel_hist(constant SelHistArgs& P [[buffer(0)]], KARGS) {
    threadgroup atomic_uint h[256];
    for (uint b = threadIdx.x; b < 256; b += blockDim.x) atomic_store_explicit(&h[b], 0u, memory_order_relaxed);
    SYNC();
    const int shift = P.shift;
    uint prefix = P.st[0];
    uint maskhi = shift >= 24 ? 0u : (0xffffffffu << (shift + 8));
    for (long i = (long)blockIdx.x * blockDim.x + threadIdx.x; i < P.n; i += (long)gridDim.x * blockDim.x) {
        uint k = fkey(P.v[i]);
        if ((k & maskhi) == (prefix & maskhi)) atomic_fetch_add_explicit(&h[(k >> shift) & 255u], 1u, memory_order_relaxed);
    }
    SYNC();
    for (uint b = threadIdx.x; b < 256; b += blockDim.x) {
        uint c = atomic_load_explicit(&h[b], memory_order_relaxed);
        if (c) atomic_fetch_add_explicit(&P.hist[b], c, memory_order_relaxed);
    }
}
struct SelScanArgs { device uint* st; device uint* hist; int shift; };
kernel void sel_scan(constant SelScanArgs& P [[buffer(0)]], KARGS) {
    if (threadIdx.x != 0) return;
    uint k = P.st[1], c = 0;
    for (int b = 0; b < 256; b++) {
        uint hb = P.hist[b];
        if (c + hb > k) { P.st[0] |= ((uint)b << P.shift); P.st[1] = k - c; break; }
        c += hb;
    }
    for (int b = 0; b < 256; b++) P.hist[b] = 0;
}
struct SelValueArgs { device const uint* st; fp out; };
kernel void sel_value(constant SelValueArgs& P [[buffer(0)]], KARGS) {
    if (threadIdx.x == 0) *P.out = fkey_inv(P.st[0]);
}
// flowdegrain2 noise(): var = max((1.4826 med)^2, 0.001)
struct NoiseArgs { cfp med; fp var; float mult; };
kernel void noise_var(constant NoiseArgs& P [[buffer(0)]], KARGS) {
    if (threadIdx.x == 0) { float t = 1.4826f * *P.med; *P.var = fmax(t * t, 0.001f) * P.mult; }
}
// cupy quantile, method 'linear', float32: lo, hi = sorted values at floor(idx), +1
struct QuantArgs { cfp lo; cfp hi; float w; fp out; };
kernel void quantile_finish(constant QuantArgs& P [[buffer(0)]], KARGS) {
    if (threadIdx.x != 0) return;
    float a = *P.lo, b = *P.hi, diff = b - a, w = P.w;
    *P.out = w < 0.5f ? a + diff * w : b - diff * (1 - w);
}

// ------------------------------------------------------------------------------------------------ ndimage filters
// scipy.ndimage correlate1d, mode "reflect" (d c b a | a b c d), weights wt[0 .. 2r], accumulated in tap order.
static inline int reflect_idx(int i, int n) {
    if (n == 1) return 0;
    while (i < 0 || i >= n) {
        if (i < 0) i = -i - 1;
        if (i >= n) i = 2 * n - i - 1;
    }
    return i;
}
struct CorrArgs { cfp a; fp b; int W, H; long n; cfp wt; int r, axis; };
kernel void corr_f(constant CorrArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; const int W = P.W, H = P.H, r = P.r;
    IDX int x = (int)(i % W), y = (int)((i / W) % H);
    long base = i - (long)y * W - x;
    float s = 0.f;
    if (P.axis == 0) {
        for (int j = -r; j <= r; j++) s += P.a[base + (long)reflect_idx(y + j, H) * W + x] * P.wt[j + r];
    } else {
        cfp row = P.a + base + (long)y * W;
        for (int j = -r; j <= r; j++) s += row[reflect_idx(x + j, W)] * P.wt[j + r];
    }
    P.b[i] = s;
}
// float64 accumulation of the reference -> float-float (TwoSum on exact products: the weights are small integers)
static inline void two_add(thread float& hi, thread float& lo, float v) {
    float s = hi + v, bb = s - hi, e = (hi - (s - bb)) + (v - bb);
    hi = s; lo += e;
}
kernel void corr_d(constant CorrArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; const int W = P.W, H = P.H, r = P.r;
    IDX int x = (int)(i % W), y = (int)((i / W) % H);
    long base = i - (long)y * W - x;
    float hi = 0.f, lo = 0.f;
    if (P.axis == 0) {
        for (int j = -r; j <= r; j++) two_add(hi, lo, mul_rn(P.a[base + (long)reflect_idx(y + j, H) * W + x], P.wt[j + r]));
    } else {
        cfp row = P.a + base + (long)y * W;
        for (int j = -r; j <= r; j++) two_add(hi, lo, mul_rn(row[reflect_idx(x + j, W)], P.wt[j + r]));
    }
    P.b[i] = hi + lo;
}
// grey dilation / erosion with a square footprint along one axis
struct MinMaxArgs { cfp a; fp b; int W, H, r, axis, is_max; };
kernel void minmax1d(constant MinMaxArgs& P [[buffer(0)]], KARGS) {
    const int W = P.W, H = P.H, r = P.r, axis = P.axis, is_max = P.is_max;
    long n = (long)W * H;
    IDX int x = (int)(i % W), y = (int)(i / W);
    float v = is_max ? -3.4e38f : 3.4e38f;
    for (int j = -r; j <= r; j++) {
        int xx = axis == 1 ? x + j : x, yy = axis == 0 ? y + j : y;
        if (xx < 0 || yy < 0 || xx >= W || yy >= H) continue;
        float u = P.a[(long)yy * W + xx];
        v = is_max ? fmax(v, u) : fmin(v, u);
    }
    P.b[i] = v;
}

// ------------------------------------------------------------------------------------------------ grain features
struct LocArgs { cfp m1; cfp m2; fp loc; long n; };
kernel void local_sd(constant LocArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX float a = P.m1[i];
    P.loc[i] = sqrt(fmax(P.m2[i] - mul_rn(a, a), 0.f));
}
struct CountArgs { cfp a; float th; long n; device atomic_uint* out; };
kernel void count_lt(constant CountArgs& P [[buffer(0)]], KARGS) {
    uint k = 0;
    for (long i = (long)blockIdx.x * blockDim.x + threadIdx.x; i < P.n; i += (long)gridDim.x * blockDim.x)
        k += P.a[i] < P.th;
    k = simd_sum(k);
    if (lane == 0) atomic_fetch_add_explicit(P.out, k, memory_order_relaxed);
}
// flat mask m = loc < th (th = *thp), luma bin = clip(floor(low / 32), 0, 7)
struct FlatArgs { cfp loc; cfp low; cfp thp; bp m; bp lum; long n; };
kernel void flat_mask(constant FlatArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX float th = *P.thp;
    P.m[i] = P.loc[i] < th;
    float k = floor(P.low[i] / 32.f);
    P.lum[i] = (uchar)(k < 0.f ? 0.f : (k > 7.f ? 7.f : k));
}
struct FlatThArgs { cfp p10; int use_p10; fp th; };
kernel void flat_threshold(constant FlatThArgs& P [[buffer(0)]], KARGS) {
    if (threadIdx.x != 0) return;
    *P.th = P.use_p10 ? fmin(fmax(1.2f, 1.5f * *P.p10), 3.0f) : 1.2f;
}

// per frame: band powers per luma bin over the flat mask, stick correlations, mask counts. Per-block partials
// part[block * ACC_N + j], j: bin * 16 + (0 count, 1..5 source bands B0..B4, 6..10 candidate bands, 11..13 the
// grain-free candidate's B0..B2); at 128 the stick sums (12) and the joint-mask count, at 141 the flat count.
#define ACC_STICK 128
#define ACC_FLAT 141
#define ACC_N 144
static inline void bands5(cfp p, long n, long i, thread float* B) {
    float r = p[i], a = p[n + i], b = p[2 * n + i], c = p[3 * n + i], d = p[4 * n + i], e = p[5 * n + i];
    B[0] = r - a; B[1] = a - b; B[2] = b - c; B[3] = c - d; B[4] = d - e;
}
struct FeatArgs { cfp sp; cfp cp; cfp ngp; cbp m; cbp lum; cbp mprev; cfp prev; fp cur; long n; fp part; };
kernel void feat_accum(constant FeatArgs& P [[buffer(0)]], KARGS) {
    const long n = P.n;
    threadgroup float sh[8][ACC_N];     // one row per SIMD group (256 threads)
    for (uint j = threadIdx.x; j < 8 * ACC_N; j += blockDim.x) sh[j / ACC_N][j % ACC_N] = 0.f;
    SYNC();
    const bool has_ng = P.ngp != 0;
    float st[13];
    for (int j = 0; j < 13; j++) st[j] = 0.f;
    float flat = 0.f;
    // SIMD-uniform loop: 32 consecutive pixels per SIMD group and step
    for (long base = (long)blockIdx.x * blockDim.x + sg * 32; base < n; base += (long)gridDim.x * blockDim.x) {
        long i = base + lane;
        bool act = false;
        uint bin = 0;
        float bs[5], bc[5], bn[3] = {0.f, 0.f, 0.f};
        if (i < n) {
            bands5(P.sp, n, i, bs);
            bands5(P.cp, n, i, bc);
            float fs = bs[0] + bs[1], fc = bc[0] + bc[1], cs = bs[3] + bs[4], cc = bc[3] + bc[4];
            P.cur[i] = fs; P.cur[n + i] = fc; P.cur[2 * n + i] = cs; P.cur[3 * n + i] = cc;
            uchar mi = P.m[i];
            if (mi) {
                flat += 1.f;
                act = true;
                bin = P.lum[i];
                if (has_ng) {
                    cfp g = P.ngp;
                    float r = g[i], a = g[n + i], b = g[2 * n + i], c = g[3 * n + i];
                    bn[0] = r - a; bn[1] = a - b; bn[2] = b - c;
                }
            }
            if (P.mprev && mi && P.mprev[i]) {
                cfp pr = P.prev;
                float p0 = pr[i], p1 = pr[n + i], p2 = pr[2 * n + i], p3 = pr[3 * n + i];
                st[0] += p0 * fs; st[1] += p0 * p0; st[2] += fs * fs;     // fine, source
                st[3] += p1 * fc; st[4] += p1 * p1; st[5] += fc * fc;     // fine, candidate
                st[6] += p2 * cs; st[7] += p2 * p2; st[8] += cs * cs;     // coarse, source
                st[9] += p3 * cc; st[10] += p3 * p3; st[11] += cc * cc;   // coarse, candidate
                st[12] += 1.f;
            }
        }
        // sums per luma bin present in this SIMD group (neighbouring pixels: usually one or two bins)
        while (simd_any(act)) {
            uint b = simd_min(act ? bin : 255u);
            bool mine = act && bin == b;
            float v[14];
            v[0] = mine ? 1.f : 0.f;
            for (int k = 0; k < 5; k++) { v[1 + k] = mine ? bs[k] * bs[k] : 0.f; v[6 + k] = mine ? bc[k] * bc[k] : 0.f; }
            for (int k = 0; k < 3; k++) v[11 + k] = mine ? bn[k] * bn[k] : 0.f;
            for (int k = 0; k < 14; k++) v[k] = simd_sum(v[k]);
            if (lane == 0)
                for (int k = 0; k < 14; k++) sh[sg][b * 16 + k] += v[k];
            act = act && !mine;
        }
    }
    for (int j = 0; j < 13; j++) {
        float s = simd_sum(st[j]);
        if (lane == 0) sh[sg][ACC_STICK + j] += s;
    }
    float fl = simd_sum(flat);
    if (lane == 0) sh[sg][ACC_FLAT] += fl;
    SYNC();
    for (uint j = threadIdx.x; j < ACC_N; j += blockDim.x) {
        float s = 0.f;
        for (int g = 0; g < 8; g++) s += sh[g][j];
        P.part[(long)blockIdx.x * ACC_N + j] = s;
    }
}
// energy of the ORIGINALS in textured 64-px tiles (tex = !m & loc > 3): one block per tile; part[tile * 2 + 0/1] =
// vs, vc of tiles with >= half the pixels textured, else 0
struct TexArgs { cfp sg0; cfp sg1; cfp sg2; cfp sg3; cfp cg0; cfp cg1; cfp cg2; cfp cg3; cbp m; cfp loc; int W; fp part; };
kernel void tex_tiles(constant TexArgs& P [[buffer(0)]], KARGS) {
    threadgroup float r[3][256];
    int tx = blockIdx.x, ty = blockIdx.y, t = threadIdx.x;
    float vs = 0.f, vc = 0.f, cnt = 0.f;
    for (int k = t; k < 64 * 64; k += blockDim.x) {
        int x = tx * 64 + (k & 63), y = ty * 64 + (k >> 6);
        long i = (long)y * P.W + x;
        if (!P.m[i] && P.loc[i] > 3.0f) {
            cnt += 1.f;
            float a1 = P.sg0[i] - P.sg1[i], a2 = P.sg1[i] - P.sg2[i], a3 = P.sg2[i] - P.sg3[i];
            float b1 = P.cg0[i] - P.cg1[i], b2 = P.cg1[i] - P.cg2[i], b3 = P.cg2[i] - P.cg3[i];
            vs += a1 * a1 + a2 * a2 + a3 * a3;
            vc += b1 * b1 + b2 * b2 + b3 * b3;
        }
    }
    r[0][t] = vs; r[1][t] = vc; r[2][t] = cnt;
    SYNC();
    for (int s = blockDim.x / 2; s; s >>= 1) {
        if (t < s) { r[0][t] += r[0][t + s]; r[1][t] += r[1][t + s]; r[2][t] += r[2][t + s]; }
        SYNC();
    }
    if (t == 0) {
        bool ok = r[2][0] >= 0.5f * 64 * 64;
        long tile = (long)ty * gridDim.x + tx;
        P.part[tile * 2] = ok ? r[0][0] : 0.f;
        P.part[tile * 2 + 1] = ok ? r[1][0] : 0.f;
    }
}
// chroma residual power in flat areas (half resolution): part[block * 5 + 0..3] = sum ra^2 mh, [4] = sum mh
struct ChromaAccArgs { cfp r; cfp gr; cbp m; int W, w2, h2; fp part; };
kernel void chroma_accum(constant ChromaAccArgs& P [[buffer(0)]], KARGS) {
    threadgroup float sh[8][5];
    float a[5] = {0.f, 0.f, 0.f, 0.f, 0.f};
    const int W = P.W, w2 = P.w2, h2 = P.h2;
    long n = (long)w2 * h2;
    for (long i = (long)blockIdx.x * blockDim.x + threadIdx.x; i < n; i += (long)gridDim.x * blockDim.x) {
        int x = (int)(i % w2), y = (int)(i / w2);
        if (!P.m[(long)(2 * y) * W + 2 * x]) continue;
        a[4] += 1.f;
        for (int p = 0; p < 4; p++) { float v = P.r[p * n + i] - P.gr[p * n + i]; a[p] += v * v; }
    }
    for (int p = 0; p < 5; p++) {
        float s = simd_sum(a[p]);
        if (lane == 0) sh[sg][p] = s;
    }
    SYNC();
    if (threadIdx.x < 5) {
        float s = 0.f;
        for (uint g = 0; g < blockDim.x / 32; g++) s += sh[g][threadIdx.x];
        P.part[(long)blockIdx.x * 5 + threadIdx.x] = s;
    }
}

// ------------------------------------------------------------------------------------------------ line art
// detector on one reference frame (10-bit codes): one thread per full 8x8 cell.
// out[0] += cells with mean act > 6, out[1] += stroke cells (max black top-hat >= 64), out[2] += stroke cells whose
// 25th percentile of act is <= 3, out[3] += cells
static inline float act_at(cfp y, int W, int H, int x, int yy) {
    float c = y[(long)yy * W + x];
    float dx = x + 1 < W ? fabs(y[(long)yy * W + x + 1] - c) : 0.f;
    float dy = yy + 1 < H ? fabs(y[(long)(yy + 1) * W + x] - c) : 0.f;
    return dx + dy;
}
struct CellsArgs { cfp y; cfp closing; int W, H; device atomic_uint* out; };
kernel void lineart_cells(constant CellsArgs& P [[buffer(0)]], KARGS) {
    const int W = P.W, H = P.H;
    int cw = W / 8, ch = H / 8;
    long n = (long)cw * ch;
    IDX int cx = (int)(i % cw), cy = (int)(i / cw);
    float v[64];
    float sum = 0.f, bmx = -3.4e38f;
    for (int k = 0; k < 64; k++) {
        int x = cx * 8 + (k & 7), yy = cy * 8 + (k >> 3);
        float a = act_at(P.y, W, H, x, yy);
        v[k] = a; sum += a;
        bmx = fmax(bmx, P.closing[(long)yy * W + x] - P.y[(long)yy * W + x]);
    }
    float mean = sum / 64.f;
    if (mean > 6.f) atomic_fetch_add_explicit(P.out, 1u, memory_order_relaxed);
    if (bmx >= 64.f) {
        atomic_fetch_add_explicit(P.out + 1, 1u, memory_order_relaxed);
        // 25th percentile (cupy linear, float32): sorted v[15], v[16]; weight 0.75 -> v16 - (v16 - v15) * 0.25
        for (int a = 1; a < 64; a++) {          // insertion sort, 64 values
            float t = v[a]; int b = a - 1;
            while (b >= 0 && v[b] > t) { v[b + 1] = v[b]; b--; }
            v[b + 1] = t;
        }
        float w = 0.75f, diff = v[16] - v[15];
        float q25 = w < 0.5f ? v[15] + diff * w : v[16] - diff * (1 - w);
        if (q25 <= 3.0f) atomic_fetch_add_explicit(P.out + 2, 1u, memory_order_relaxed);
    }
    atomic_fetch_add_explicit(P.out + 3, 1u, memory_order_relaxed);
}
// gradient magnitude of the Sobel pair / 8 > EDGE_T  -> edge mask
struct EdgeArgs { cfp sx; cfp sy; bp e; float th; long n; };
kernel void edge_mask(constant EdgeArgs& P [[buffer(0)]], KARGS) {
    long n = P.n; IDX float a = P.sx[i], b = P.sy[i];
    P.e[i] = precise::sqrt(a * a + b * b) / 8.f > P.th;
}
// distance to the nearest edge pixel, exact up to 6 px: column pass (|dy| <= 6, else 7), then row pass -> d^2
struct EdtArgs { cbp e; bp g; int W, H; };
kernel void edt_col(constant EdtArgs& P [[buffer(0)]], KARGS) {
    const int W = P.W, H = P.H;
    long n = (long)W * H;
    IDX int x = (int)(i % W), y = (int)(i / W);
    int best = 7;
    for (int d = 0; d <= 6 && d < best; d++) {
        if ((y - d >= 0 && P.e[(long)(y - d) * W + x]) || (y + d < H && P.e[(long)(y + d) * W + x])) { best = d; break; }
    }
    P.g[i] = (uchar)best;
}
// ringing next to lines: near = 2 <= d <= 6 & lsd < flat_t; value = max(hc^2 - hs^2, 0) on near pixels, -1 elsewhere
struct RingArgs { cbp g; cfp m1; cfp m2; cfp s; cfp gs; cfp c; cfp gc; int W, H; float flat_t; fp val; device atomic_uint* cnt; };
kernel void ring_near(constant RingArgs& P [[buffer(0)]], KARGS) {
    const int W = P.W, H = P.H;
    long n = (long)W * H;
    IDX int x = (int)(i % W), y = (int)(i / W);
    int best = 1000;
    for (int dx = -6; dx <= 6; dx++) {
        int xx = x + dx;
        if (xx < 0 || xx >= W) continue;
        int gv = P.g[(long)y * W + xx];
        if (gv > 6) continue;
        int d2 = dx * dx + gv * gv;
        if (d2 < best) best = d2;
    }
    float a = P.m1[i];
    float lsd = sqrt(fmax(P.m2[i] - mul_rn(a, a), 0.f));
    bool near = best >= 4 && best <= 36 && lsd < P.flat_t;
    float hs = P.s[i] - P.gs[i], hc = P.c[i] - P.gc[i];
    float add = fmax(mul_rn(hc, hc) - mul_rn(hs, hs), 0.f);
    P.val[i] = near ? add : -1.f;
    if (near) atomic_fetch_add_explicit(P.cnt, 1u, memory_order_relaxed);
}
// sum (per-block partial, part[block]) and count (cnt) of values above the threshold (*thp)
struct SumAboveArgs { cfp v; cfp thp; long n; fp part; device atomic_uint* cnt; };
kernel void sum_above(constant SumAboveArgs& P [[buffer(0)]], KARGS) {
    threadgroup float sh[8];
    float th = *P.thp, a = 0.f;
    uint k = 0;
    for (long i = (long)blockIdx.x * blockDim.x + threadIdx.x; i < P.n; i += (long)gridDim.x * blockDim.x)
        if (P.v[i] > th) { a += P.v[i]; k++; }
    a = simd_sum(a);
    k = simd_sum(k);
    if (lane == 0) { sh[sg] = a; atomic_fetch_add_explicit(P.cnt, k, memory_order_relaxed); }
    SYNC();
    if (threadIdx.x == 0) {
        float s = 0.f;
        for (uint g = 0; g < blockDim.x / 32; g++) s += sh[g];
        P.part[blockIdx.x] = s;
    }
}
