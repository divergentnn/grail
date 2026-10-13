#!/usr/bin/env python3
"""Lab tool: fills docs/technical.md's exactness / whole-set / speed placeholders from the check and bench results.

    readme_tables.py tools/technical.template.md tools/README.correlation.md results/ docs/technical.md
(README.correlation.md: the lab's README_GRAIL_section.md, the two validation rounds labelled by film)
"""
import json
import sys
from pathlib import Path

head, section, res, out = (Path(a) for a in sys.argv[1:5])
txt = head.read_text()
STAGES = [("S0", "plain SSIMULACRA2, window mean"), ("S0_frame", "plain SSIMULACRA2, worst frame (reference rounded to 0.001)"),
          ("active", "active picture, mismatches"), ("S_D", "SSIMULACRA2(Ds, Dc), window mean"),
          ("S_D_frame", "SSIMULACRA2(Ds, Dc), worst frame"), ("S_Dp", "SSIMULACRA2(Ds, grain-free decode), window mean"),
          ("S_Dp_frame", "SSIMULACRA2(Ds, grain-free decode), worst frame"), ("sigma_g", "source grain rms sigma_g"),
          ("P_rel", "band powers per luma bin (relative)"), ("n_lum_rel", "flat pixels per luma bin (relative)"),
          ("chroma_rel", "chroma residual power (relative)"), ("stick", "stick / stickc correlations"),
          ("tex_ratio", "texture energy ratio"), ("R", "grainy-base capture ratio R"), ("path", "structure path, mismatches"),
          ("S_struct", "structure term"), ("g", "grain weight g"), ("penalty_sum", "weighted penalties (points)"),
          ("lineart_a", "line-art confidence a"), ("near_top_frame", "line-art ringing peak, worst frame"),
          ("p_L", "line-art penalty p_L"), ("Q", "GRAILS")]
c = json.loads((res / "check_reference.json").read_text())
w = c["worst"]
fast = json.loads((res / "check_fast.json").read_text()) if (res / "check_fast.json").exists() else None
lines = ["| stage | worst error over the 13 cases |", "|---|---|"]
for k, d in STAGES:
    v = w.get(k)
    cell = '-' if v is None else ('%d' % v if k in ('active', 'path') else ('%.1e' % v if v and v < 1e-3 else '%.4f' % v))
    lines.append(f"| {d} | {cell} |")
lines += ["", "| case | reference Q | port Q | dQ (full) | dQ (fast, vs the reference's fast inputs) |", "|---|---|---|---|---|"]
fq = {r["key"]: r for r in (fast or {}).get("cases", [])}
sg = lambda x: ("%+.4f" % x).replace("-0.0000", "0.0000").replace("+0.0000", "0.0000")
for r in c["cases"]:
    f = fq.get(r["key"])
    lines.append(f"| {r['case']} | {r['Q_ref']:.3f} | {r['Q_port']:.3f} | {sg(r['dQ'])} | {sg(f['dQ']) if f else '-'} |")
txt = txt.replace("EXACTNESS_TABLE", "\n".join(lines))
wfiles = [f for f in [res / "whole_subset.json", *sorted(res.glob("whole_rest_*.json"))] if f.exists()]
if wfiles:
    rows, fails = [], []
    for f in wfiles:
        d = json.loads(f.read_text())
        rows += d["cases"]
        fails += d.get("failures", [])
    d = {"cases": rows, "failures": fails}
    dq = sorted(abs(r["dQ"]) for r in rows)
    import statistics
    qa = [r["Q_ref"] for r in rows]; qb = [r["Q_port"] for r in rows]

    def ranks(v):
        o = sorted(range(len(v)), key=lambda i: v[i]); r = [0.0] * len(v); i = 0
        while i < len(o):
            j = i
            while j + 1 < len(o) and v[o[j + 1]] == v[o[i]]:
                j += 1
            for k in range(i, j + 1):
                r[o[k]] = (i + j) / 2
            i = j + 1
        return r
    ra, rb = ranks(qa), ranks(qb)
    ma, mb = statistics.mean(ra), statistics.mean(rb)
    sp = sum((x - ma) * (y - mb) for x, y in zip(ra, rb)) / (sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb)) ** 0.5
    nf = len(d.get("failures", []))
    films = sorted({r["case"] for r in rows})
    part = "all" if len(rows) + nf >= 574 else f"every 4th ({len(rows)})"
    txt = txt.replace("WHOLESET_LINE", f"Whole-set check (PORT_SPEC gate: Spearman >= 0.995): {part} of the 574 judged showings "
                      f"({len(films)} films, full mode): Spearman between the port's and the reference's Q "
                      f"{sp:.6f}; |dQ| median {dq[len(dq)//2]:.1e}, max {dq[-1]:.4f}; {nf} failed to run.")
else:
    txt = txt.replace("WHOLESET_LINE\n", "")
b = res / "bench.json"
if b.exists():
    d = json.loads(b.read_text())
    ref = {("1080p", "full", "software"): 13.8, ("1080p", "fast", "software"): 22.4, ("4K", "full", "software"): 4.8,
           ("4K", "fast", "software"): 7.5, ("4K", "full", "NVDEC ref"): 5.3, ("4K", "fast", "NVDEC ref"): 8.8}
    L = ["| picture | mode | decode | Rust beta frames/s | Python reference frames/s | speed-up | decode share (Rust) |",
         "|---|---|---|---|---|---|---|"]
    for r in d["runs"]:
        p = ref.get((r["label"], r["mode"], r["decode"]))
        L.append(f"| {r['label']} | {r['mode']} | {r['decode']} | {r['fps']} | {p if p else '-'} | "
                 f"{'%.1fx' % (r['fps'] / p) if p else '-'} | {round(100 * r['decode_share'])} % |")
    L.append("")
    L.append(f"1-minute load at start {d['load_start'][0]}, at end {d['load_end'][0]}.")
    txt = txt.replace("SPEED_SECTION", "\n".join(L))
out.write_text(txt + "\n" + section.read_text())
print("written", out)
