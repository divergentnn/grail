#!/usr/bin/env python3
"""Lab tool: GRAILS speed on macOS (Metal backend), the same protocol as bench.py on Linux: 88 frames of the whole
active picture + 3 context frames each side, one AV1 film-grain encode against its source, full and --fast, software
decode and VideoToolbox for the 4K source. macOS cannot pin a process to cores; the run is nice 19 instead.

    bench_macos.py BENCH_CLIPS_JSON GRAIL_BINARY FULLGRAIN_LIB OUT.json [--reps 2]

BENCH_CLIPS_JSON: {"clips": {"1080p": {"ref", "ref_lead", "cand", "cand_lead", "active", "frames"}, "4K": {...}}};
paths are relative to its directory.
"""
import json
import subprocess
import sys
import time
from pathlib import Path

clips_file, binary, fg, out = (Path(a) for a in sys.argv[1:5])
reps = int(sys.argv[sys.argv.index("--reps") + 1]) if "--reps" in sys.argv else 2
base = clips_file.resolve().parent
clips = json.loads(clips_file.read_text())["clips"]


def load():
    return subprocess.run(["sysctl", "-n", "vm.loadavg"], capture_output=True, text=True).stdout.strip("{} \n").split()


runs = []
load0 = load()
for label, c in clips.items():
    n = c.get("frames", 88)
    for dec in ("none", "videotoolbox"):
        if label == "1080p" and dec == "videotoolbox":
            continue                     # auto = software at 1080p (VideoToolbox H.264 is slower than software here)
        for mode in ("full", "fast"):
            best = None
            for _ in range(reps):
                cmd = ["nice", "-n", "19", str(binary), "score", "--ref", str(base / c["ref"]), "--dist",
                       str(base / c["cand"]), "--start", str(c["ref_lead"]), "--dist-start", str(c["cand_lead"]),
                       "--frames", str(n), "--crop", ",".join(map(str, c["active"])), "--threads", "4", "--hwdec", dec,
                       "--fullgrain-lib", str(fg), "--json"] + (["--fast"] if mode == "fast" else [])
                t = time.time()
                p = subprocess.run(cmd, capture_output=True, text=True)
                wall = time.time() - t
                if p.returncode:
                    raise SystemExit(p.stderr)
                r = json.loads(p.stdout)
                tm = r["timings"]
                dec_s = tm.get("decode_reference_scan", 0) + sum(v for k, v in tm.items() if k.startswith("decode_wait"))
                row = {"label": label, "mode": mode, "decode": "VideoToolbox ref" if dec == "videotoolbox" else "software",
                       "frames": n, "process_s": round(wall, 2), "score_s": round(tm["total"], 2),
                       "fps_process": round(n / wall, 1), "fps": round(n / tm["total"], 1),
                       "decode_share": round(dec_s / tm["total"], 3), "timings": tm, "score": r["score"],
                       "S0": r["ssimulacra2"], "active": r["active"], "gpu": r["gpu"]}
                print(json.dumps({k: v for k, v in row.items() if k != "timings"}), flush=True)
                if best is None or row["score_s"] < best["score_s"]:
                    best = row
            runs.append(best)
load1 = load()
out.write_text(json.dumps({"load_start": load0, "load_end": load1, "runs": runs}, indent=1))
print("load", load0, "->", load1)
