#!/usr/bin/env python3
"""Lab tool: plain SSIMULACRA2 speed (FFVship) next to GRAILS full and --fast, on the same clips and frames, for the
README's Speed section.

Clips: the benchmark clips of bench.py / bench_macos.py, as listed in a bench_clips.json ({"clips": {label: {"ref",
"ref_lead", "cand", "cand_lead", "active", "frames"}}}, media paths relative to that file): 88 frames of the whole
active picture, one AV1 film-grain encode against its source.

1. Frame-exact windows. The 88 shown frames of the source and of the encode (AV1 film grain rendered full-frame by
   av1-fullgrain, as GRAILS renders it) are written to FFV1 files, cropped to the active picture, 10-bit,
   colour-tagged (as grain-lab/grainscore/baseline.py does for every metric of the calibration). FFVship (FFMS2)
   numbers the frames of stream-copied H.264 cuts differently from ffmpeg, so only these windows are certain to hold
   the same frames for both tools.
2. Alignment check (not timed): FFVship's SSIMULACRA2 on the windows must match GRAILS's plain SSIMULACRA2 (S0) on
   the original files within TOL (default 0.1), and GRAILS's S0 on the windows must match it too. A clip that fails
   the check is reported as such and its speeds must not be quoted.
3. Speeds, REPS runs each, the fastest kept, every run pinned to cores 8-11 at nice 19 (as bench.py):
   - FFVship on the windows (the metric alone on exactly these frames; decoding FFV1);
   - FFVship on the original files, end to end (--start / --end / --encoded-offset and crops for the same window;
     its frames may be off, see 1; its SSIMULACRA2 is recorded, not trusted);
   - GRAILS full and fast on the original files, end to end: software decoding, and NVDEC for the 4K source.
   FFVship: SSIMULACRA2, 2 decoder processes and 3 GPU threads (its recommended settings), FFMS2 indexes built
   beforehand (not timed). Speeds are frames/s over the whole process (start-up included, indexing excluded), and for
   GRAILS also inside `grail score`. Load average and GPU use (utilization, memory, other compute processes) are
   recorded before and after each run: the comparison is only fair on an otherwise idle GPU.

    bench_ffvship.py BENCH_CLIPS_JSON GRAIL_BINARY FULLGRAIN_LIB OUT.json [--reps 2] [--tol 0.1]
        [--ffvship /usr/local/bin/FFVship]
"""
import argparse
import json
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path

PIN = ["nice", "-n", "19", "taskset", "-c", "8-11"]


def machine():
    load = Path("/proc/loadavg").read_text().split()[:3]
    q = lambda *a: subprocess.run(["nvidia-smi", *a, "--format=csv,noheader,nounits"], capture_output=True,
                                  text=True).stdout.strip()
    return {"t": time.strftime("%F %T"), "load": load, "gpu_util_mem": q("--query-gpu=utilization.gpu,memory.used"),
            "gpu_apps": q("--query-compute-apps=pid,process_name,used_memory").splitlines()}


def probe(path):
    o = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                        "stream=codec_name,width,height,color_transfer", "-of", "json", str(path)],
                       check=True, capture_output=True, text=True).stdout
    return json.loads(o)["streams"][0]


def fullgrain_env(fg):
    env = {k: v for k, v in os.environ.items() if not k.startswith("DAV1D_GRAIN")}
    env["LD_LIBRARY_PATH"] = f"{fg}:{env['LD_LIBRARY_PATH']}" if env.get("LD_LIBRARY_PATH") else str(fg)
    env["DAV1D_GRAIN_MODE"], env["DAV1D_GRAIN_FORCE"] = "full", "1"
    return env


def window(src, lead, n, active, pq, fg, out):
    """frames [lead, lead + n) of src, cropped to the active picture, as a colour-tagged 10-bit FFV1 file"""
    x, y, w, h = active
    tags = ("color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc" if pq else
            "color_primaries=bt709:color_trc=bt709:colorspace=bt709")
    dec = ["-c:v", "libdav1d"] if probe(src)["codec_name"] == "av1" else []
    subprocess.run([*PIN, "ffmpeg", "-v", "error", "-nostdin", "-y", *dec, "-i", str(src), "-map", "0:v:0", "-vf",
                    f"trim=start_frame={lead}:end_frame={lead + n},setpts=PTS-STARTPTS,crop={w}:{h}:{x}:{y},"
                    f"format=yuv420p10le,setparams=range=tv:{tags}", "-c:v", "ffv1", "-level", "3", "-threads", "4",
                    str(out)], check=True, env=fullgrain_env(fg))
    o = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries",
                        "stream=nb_read_packets", "-of", "csv=p=0", str(out)], capture_output=True, text=True).stdout
    got = int((re.findall(r"\d+", o) or [0])[0])
    if got != n:
        raise SystemExit(f"window {src}: {got} frames, expected {n}")


def crop_args(path, active, side):
    """FFVship crops (top / bottom / left / right) that cut the active picture out of this file"""
    p = probe(path)
    W, H = p["width"], p["height"]
    x, y, w, h = active
    if (W, H) == (w, h):
        return []
    return [f"--cropTop{side}", str(y), f"--cropBottom{side}", str(H - y - h), f"--cropLeft{side}", str(x),
            f"--cropRight{side}", str(W - x - w)]


def run_ffvship(ffv, ref, cand, n, fg, idx, extra=()):
    out = idx / "ffv.json"
    cmd = [*PIN, ffv, "-s", str(ref), "-e", str(cand), "-m", "SSIMULACRA2", "-t", "2", "-g", "3",
           "--source-index", str(idx / f"{Path(ref).name}.ffindex"),
           "--encoded-index", str(idx / f"{Path(cand).name}.ffindex"), "--cache-index", "--json", str(out), *extra]
    m0 = machine()
    t = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True, env=fullgrain_env(fg))
    wall = time.time() - t
    if p.returncode:
        raise SystemExit(f"FFVship failed: {p.stderr[-800:]}")
    v = [f[0] if isinstance(f, list) else f for f in json.loads(out.read_text())]
    if len(v) != n:
        raise SystemExit(f"FFVship scored {len(v)} frames, expected {n}")
    return {"tool": "FFVship", "frames": len(v), "process_s": round(wall, 2), "fps_process": round(len(v) / wall, 1),
            "ssimulacra2": sum(v) / len(v), "machine_before": m0, "machine_after": machine(),
            "cmd": " ".join(map(str, cmd))}


def run_grail(binary, ref, cand, start, dist_start, n, crop, fg, mode, hwdec):
    cmd = [*PIN, str(binary), "score", "--ref", str(ref), "--dist", str(cand), "--start", str(start),
           "--dist-start", str(dist_start), "--frames", str(n), "--threads", "4", "--hwdec", hwdec,
           "--fullgrain-lib", str(fg), "--json"] + (["--crop", ",".join(map(str, crop))] if crop else []) + \
          (["--fast"] if mode == "fast" else [])
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


def best_of(reps, f):
    best = None
    for _ in range(reps):
        r = f()
        print(json.dumps({k: v for k, v in r.items() if k not in ("timings", "cmd", "machine_before",
                                                                  "machine_after")}), flush=True)
        if best is None or r["process_s"] < best["process_s"]:
            best = r
    return best


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("clips")
    ap.add_argument("binary")
    ap.add_argument("fullgrain_lib")
    ap.add_argument("out")
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--tol", type=float, default=0.1)
    ap.add_argument("--ffvship", default="/usr/local/bin/FFVship")
    a = ap.parse_args()
    cj = Path(a.clips).resolve()
    clips = json.loads(cj.read_text())["clips"]
    fg = a.fullgrain_lib
    start = machine()
    print("start", json.dumps(start), flush=True)
    res = {}
    # the FFV1 windows (about 1 GB for the 4K clip) go next to OUT, on disk, and are removed at the end
    with tempfile.TemporaryDirectory(prefix="bench_ffvship_", dir=Path(a.out).resolve().parent) as td:
        for label, c in clips.items():
            ref, cand = cj.parent / c["ref"], cj.parent / c["cand"]
            n, act = c.get("frames", 88), c["active"]
            pq = probe(ref).get("color_transfer") == "smpte2084"
            tmp = Path(td) / label
            tmp.mkdir()
            wref, wcand = tmp / "ref.ffv1.mkv", tmp / "cand.ffv1.mkv"
            window(ref, c["ref_lead"], n, act, pq, fg, wref)
            window(cand, c["cand_lead"], n, act, pq, fg, wcand)
            # 2. alignment check (FFMS2 indexes are built by these first FFVship runs)
            g_orig = run_grail(a.binary, ref, cand, c["ref_lead"], c["cand_lead"], n, act, fg, "full", "none")
            g_win = run_grail(a.binary, wref, wcand, 0, 0, n, None, fg, "full", "none")
            f_win = run_ffvship(a.ffvship, wref, wcand, n, fg, tmp)
            orig_extra = ["--start", str(c["ref_lead"]), "--end", str(c["ref_lead"] + n),
                          "--encoded-offset", str(c["cand_lead"] - c["ref_lead"]),
                          *crop_args(ref, act, "Source"), *crop_args(cand, act, "Encoded")]
            f_orig = run_ffvship(a.ffvship, ref, cand, n, fg, tmp, orig_extra)
            chk = {"grails_s0_original": g_orig["ssimulacra2"], "grails_s0_windows": g_win["ssimulacra2"],
                   "ffvship_windows": f_win["ssimulacra2"], "ffvship_original_files": f_orig["ssimulacra2"]}
            chk["aligned"] = (abs(chk["ffvship_windows"] - chk["grails_s0_original"]) <= a.tol and
                              abs(chk["grails_s0_windows"] - chk["grails_s0_original"]) <= a.tol)
            chk["ffvship_original_files_aligned"] = abs(chk["ffvship_original_files"] - chk["grails_s0_original"]) <= a.tol
            print(label, "alignment", json.dumps(chk), flush=True)
            # 3. speeds
            runs = {"ffvship_windows": best_of(a.reps, lambda: run_ffvship(a.ffvship, wref, wcand, n, fg, tmp)),
                    "ffvship_original": best_of(a.reps, lambda: run_ffvship(a.ffvship, ref, cand, n, fg, tmp,
                                                                            orig_extra))}
            hws = ["none"] + (["auto"] if probe(ref)["width"] > 2048 else [])
            for hw in hws:
                for mode in ("full", "fast"):
                    runs[f"grails_{mode}_{'nvdec' if hw == 'auto' else 'sw'}"] = best_of(
                        a.reps, lambda: run_grail(a.binary, ref, cand, c["ref_lead"], c["cand_lead"], n, act, fg,
                                                  mode, hw))
            res[label] = {"frames": n, "alignment": chk, "runs": runs}
    end = machine()
    Path(a.out).write_text(json.dumps({"start": start, "end": end, "reps": a.reps, "tol": a.tol, "clips": res},
                                      indent=1))
    for label, r in res.items():
        ch = r["alignment"]
        print(f"\n{label}: aligned {ch['aligned']} (FFVship on windows {ch['ffvship_windows']:.3f}, GRAILS S0 on "
              f"originals {ch['grails_s0_original']:.3f}, on windows {ch['grails_s0_windows']:.3f}; FFVship on the "
              f"original files {ch['ffvship_original_files']:.3f})")
        for k, v in r["runs"].items():
            print(f"  {k:20s} {v['fps_process']:6.1f} frames/s (process)" +
                  (f"  {v['fps']:6.1f} inside grail" if "fps" in v else ""))
    print("load", start["load"], "->", end["load"], "| GPU", start["gpu_util_mem"], "->", end["gpu_util_mem"])


if __name__ == "__main__":
    main()
