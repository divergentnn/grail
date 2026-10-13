#!/usr/bin/env python3
"""Lab tool: plain SSIMULACRA2 speed (FFVship) next to GRAILS full and --fast, on the same clips, windows and machine
state, for the README's Speed section ("FFVship: N frames/s on the same clips").

Clips: the benchmark clips of bench.py / bench_macos.py, as listed in a bench_clips.json ({"clips": {label: {"ref",
"ref_lead", "cand", "cand_lead", "active", "frames"}}}, media paths relative to that file): 88 frames of the whole
active picture, one AV1 film-grain encode against its source. Every run is pinned to cores 8-11 at nice 19 (as
bench.py), REPS runs each, the fastest kept. FFVship: SSIMULACRA2, 2 decoder processes and 3 GPU threads (its
recommended settings), FFMS2 indexes built once beforehand (not timed), the AV1 encode decoded with the same
av1-fullgrain libdav1d (full-frame grain) as GRAILS uses. GRAILS: software decoding, and NVDEC for the 4K source
(--hwdec auto), as in the README's table.

Speeds: frames/s over the whole process (wall clock, FFVship and GRAILS alike: start-up, indexing excluded for
FFVship, GPU initialization included for both), and for GRAILS also frames/s inside `grail score` (the README's
number). Load average and GPU use (utilization, memory, other compute processes) are recorded before and after each
run: the comparison is only fair on an otherwise idle GPU.

    bench_ffvship.py BENCH_CLIPS_JSON GRAIL_BINARY FULLGRAIN_LIB OUT.json [--reps 2] [--ffvship /usr/local/bin/FFVship]

The mean SSIMULACRA2 of each tool is printed as an alignment check only: FFVship (FFMS2) may number the frames of
stream-copied H.264 cuts differently from ffmpeg, so its window can be off by a few frames.
"""
import argparse
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

PIN = ["nice", "-n", "19", "taskset", "-c", "8-11"]


def machine():
    load = Path("/proc/loadavg").read_text().split()[:3]
    q = lambda *a: subprocess.run(["nvidia-smi", *a, "--format=csv,noheader,nounits"], capture_output=True,
                                  text=True).stdout.strip()
    util = q("--query-gpu=utilization.gpu,memory.used")
    apps = q("--query-compute-apps=pid,process_name,used_memory")
    return {"t": time.strftime("%F %T"), "load": load, "gpu_util_mem": util, "gpu_apps": apps.splitlines()}


def dims(path):
    o = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height",
                        "-of", "csv=p=0", str(path)], check=True, capture_output=True, text=True).stdout
    w, h = o.strip().split(",")[:2]
    return int(w), int(h)


def crop_args(path, active, side):
    """FFVship crops (top / bottom / left / right) that cut the active picture out of this file"""
    W, H = dims(path)
    x, y, w, h = active
    if (W, H) == (w, h):
        return []
    return [f"--cropTop{side}", str(y), f"--cropBottom{side}", str(H - y - h), f"--cropLeft{side}", str(x),
            f"--cropRight{side}", str(W - x - w)]


def fullgrain_env(fg):
    env = {k: v for k, v in os.environ.items() if not k.startswith("DAV1D_GRAIN")}
    env["LD_LIBRARY_PATH"] = f"{fg}:{env['LD_LIBRARY_PATH']}" if env.get("LD_LIBRARY_PATH") else str(fg)
    env["DAV1D_GRAIN_MODE"], env["DAV1D_GRAIN_FORCE"] = "full", "1"
    return env


def run_ffvship(ffv, c, n, fg, tmp, timed=True):
    ref, cand = c["ref_path"], c["cand_path"]
    out = tmp / "ffv.json"
    cmd = [*PIN, ffv, "-s", str(ref), "-e", str(cand), "-m", "SSIMULACRA2", "-t", "2", "-g", "3",
           "--start", str(c["ref_lead"]), "--end", str(c["ref_lead"] + n),
           "--encoded-offset", str(c["cand_lead"] - c["ref_lead"]),
           "--source-index", str(tmp / "ref.ffindex"), "--encoded-index", str(tmp / "cand.ffindex"), "--cache-index",
           "--json", str(out), *crop_args(ref, c["active"], "Source"), *crop_args(cand, c["active"], "Encoded")]
    m0 = machine()
    t = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True, env=fullgrain_env(fg))
    wall = time.time() - t
    if p.returncode:
        raise SystemExit(f"FFVship failed: {p.stderr[-800:]}")
    v = [f[0] if isinstance(f, list) else f for f in json.loads(out.read_text())]
    return {"tool": "FFVship", "mode": "SSIMULACRA2", "decode": "software (FFMS2)", "frames": len(v),
            "process_s": round(wall, 2), "fps_process": round(len(v) / wall, 1), "ssimulacra2": sum(v) / len(v),
            "machine_before": m0, "machine_after": machine(), "cmd": " ".join(map(str, cmd))}


def run_grail(binary, c, n, fg, mode, hwdec):
    cmd = [*PIN, str(binary), "score", "--ref", str(c["ref_path"]), "--dist", str(c["cand_path"]),
           "--start", str(c["ref_lead"]), "--dist-start", str(c["cand_lead"]), "--frames", str(n),
           "--crop", ",".join(map(str, c["active"])), "--threads", "4", "--hwdec", hwdec, "--fullgrain-lib", str(fg),
           "--json"] + (["--fast"] if mode == "fast" else [])
    m0 = machine()
    t = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True)
    wall = time.time() - t
    if p.returncode:
        raise SystemExit(f"grail failed: {p.stderr[-800:]}")
    r = json.loads(p.stdout)
    return {"tool": "GRAILS", "mode": mode, "decode": "NVDEC ref" if hwdec == "auto" else "software", "frames": n,
            "process_s": round(wall, 2), "score_s": round(r["timings"]["total"], 2),
            "fps_process": round(n / wall, 1), "fps": round(n / r["timings"]["total"], 1),
            "ssimulacra2": r["ssimulacra2"], "grails": r["score"], "timings": r["timings"],
            "machine_before": m0, "machine_after": machine(), "cmd": " ".join(map(str, cmd))}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("clips")
    ap.add_argument("binary")
    ap.add_argument("fullgrain_lib")
    ap.add_argument("out")
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--ffvship", default="/usr/local/bin/FFVship")
    a = ap.parse_args()
    cj = Path(a.clips).resolve()
    clips = json.loads(cj.read_text())["clips"]
    start = machine()
    print("start", json.dumps(start), flush=True)
    runs = []
    with tempfile.TemporaryDirectory(prefix="bench_ffvship_") as td:
        for label, c in clips.items():
            c = {**c, "ref_path": cj.parent / c["ref"], "cand_path": cj.parent / c["cand"]}
            n = c.get("frames", 88)
            tmp = Path(td) / label
            tmp.mkdir()
            run_ffvship(a.ffvship, c, n, a.fullgrain_lib, tmp)          # builds the FFMS2 indexes (not kept)
            jobs = [("ffvship", None, None)] + [("grail", m, "none") for m in ("full", "fast")]
            if label != "1080p":
                jobs += [("grail", m, "auto") for m in ("full", "fast")]
            for tool, mode, hw in jobs:
                best = None
                for _ in range(a.reps):
                    r = (run_ffvship(a.ffvship, c, n, a.fullgrain_lib, tmp) if tool == "ffvship" else
                         run_grail(a.binary, c, n, a.fullgrain_lib, mode, hw))
                    r["label"] = label
                    print(json.dumps({k: v for k, v in r.items() if k not in ("timings", "cmd")}), flush=True)
                    if best is None or r["process_s"] < best["process_s"]:
                        best = r
                runs.append(best)
    end = machine()
    Path(a.out).write_text(json.dumps({"start": start, "end": end, "reps": a.reps, "runs": runs}, indent=1))
    print(f"\n{'clip':6s} {'tool':8s} {'mode':12s} {'decode':16s} {'fps (process)':>14s} {'fps (in grail)':>15s} "
          f"{'SSIMULACRA2':>12s}")
    for r in runs:
        print(f"{r['label']:6s} {r['tool']:8s} {r['mode']:12s} {r['decode']:16s} {r['fps_process']:14.1f} "
              f"{r.get('fps', float('nan')):15.1f} {r['ssimulacra2']:12.2f}")
    print("load", start["load"], "->", end["load"], "| GPU", start["gpu_util_mem"], "->", end["gpu_util_mem"])


if __name__ == "__main__":
    main()
