#!/usr/bin/env python3
"""Lab tool: GRAILS Rust beta speed on the Python reference's benchmark clips (grain-lab/grainscore/bench_grail.py):
88 frames of the whole active picture + 3 context frames each side, one AV1 film-grain encode against its source,
full and --fast, software decode and NVDEC for the 4K source. The whole process runs on cores 8-11 (the reference
pinned its ffmpeg decodes there), nice 19.

    bench.py LAB_DIR GRAIL_BINARY FULLGRAIN_LIB OUT.json [--reps 2]
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

lab, binary, fg, out = (Path(a) for a in sys.argv[1:5])
reps = int(sys.argv[sys.argv.index("--reps") + 1]) if "--reps" in sys.argv else 2
N = 88


def clip(round_dir, group):
    rj = json.loads((lab / round_dir / "round.json").read_text())
    g = next(x for x in rj["groups"] if x["id"] == group)
    ref, c = g["reference"]["source"], g["candidates"][0]
    return {"ref": str(lab / round_dir / ref["src"]), "ref_lead": ref["lead_frames"], "cand": str(lab / round_dir / c["src"]),
            "cand_lead": c["lead_frames"], "active": g["active_crop"]}


clips = {"1080p": clip("bs-target-r1", "bs-target-r1-g01"), "4K": clip("basterds-full-r2", "basterds-full-r2-g01")}
runs = []
load0 = Path("/proc/loadavg").read_text().split()[:3]
for label, c in clips.items():
    for dec in ("none", "auto"):
        if label == "1080p" and dec == "auto":
            continue                     # auto = software at 1080p
        for mode in ("full", "fast"):
            best = None
            for _ in range(reps):
                cmd = ["nice", "-n", "19", "taskset", "-c", "8-11", str(binary), "score", "--ref", c["ref"], "--dist", c["cand"],
                       "--start", str(c["ref_lead"]), "--dist-start", str(c["cand_lead"]), "--frames", str(N),
                       "--crop", ",".join(map(str, c["active"])), "--threads", "4", "--hwdec", dec,
                       "--fullgrain-lib", str(fg), "--json"] + (["--fast"] if mode == "fast" else [])
                t = time.time()
                p = subprocess.run(cmd, capture_output=True, text=True)
                wall = time.time() - t
                if p.returncode:
                    raise SystemExit(p.stderr)
                r = json.loads(p.stdout)
                tm = r["timings"]
                dec_s = tm.get("decode_reference_scan", 0) + sum(v for k, v in tm.items() if k.startswith("decode_wait"))
                row = {"label": label, "mode": mode, "decode": "NVDEC ref" if dec == "auto" else "software",
                       "frames": N, "process_s": round(wall, 2), "score_s": round(tm["total"], 2),
                       "fps_process": round(N / wall, 1), "fps": round(N / tm["total"], 1),
                       "decode_share": round(dec_s / tm["total"], 3), "timings": tm, "score": r["score"],
                       "S0": r["ssimulacra2"], "active": r["active"]}
                print(json.dumps({k: v for k, v in row.items() if k != "timings"}), flush=True)
                if best is None or row["score_s"] < best["score_s"]:
                    best = row
            runs.append(best)
load1 = Path("/proc/loadavg").read_text().split()[:3]
out.write_text(json.dumps({"load_start": load0, "load_end": load1, "runs": runs}, indent=1))
print("load", load0, "->", load1)
