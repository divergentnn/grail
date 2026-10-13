#!/usr/bin/env python3
"""Lab tool: the README's illustrated examples (docs/images/example-*.png and docs/images/examples.json).

Each example is one scene of a blind review round: the source and two encodes the viewer judged, at the same frame
and region. The scores come from this repository's grail (full mode) on exactly the frames and window the viewer saw
(the round's own export: ref / encode lead frames, start and end, crop); the verdicts from the round's verdict file.
Rerun with --rescore after a model update to refresh the numbers in the images and in examples.json (then update the README text).

    GRAIL_LAB=<grain-lab dir> GRAIL_FULLGRAIN_LIB=<av1-fullgrain dav1d build/src> python3 tools/make_examples.py
        [--only ID] [--rescore] [--dump FRAME]

    --dump FRAME   write the whole shown window of that frame (source and each encode) to results/examples/ instead,
                   for choosing a frame and region

Environment: GRAIL_LAB (the lab directory with the review rounds and review/ingest.py; required), GRAIL_FULLGRAIN_LIB
(required: AV1 film grain is rendered full-frame, in the scores and in the pictures), GRAIL_BIN (default
target/release/grail), GRAIL_FONT / GRAIL_FONT_BOLD (TrueType fonts for the labels). Needs numpy, Pillow, and ffmpeg
with libdav1d and zscale. Grail runs at nice 15 with one decoder thread per stream.
"""
import argparse
import json
import os
import subprocess
import sys
from fractions import Fraction
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

REPO = Path(__file__).resolve().parent.parent
OUT_IMG = REPO / "docs" / "images"
CACHE = REPO / "results" / "examples"

# frame: index into the shown window (0 = its first frame); region: x, y, w, h inside the shown window; zoom: nearest
# neighbour factor. Labels are what the encodes are, never what the scores say.
EXAMPLES = [
    {"id": "grain", "film": "The Lighthouse (2019)", "round": "recal2-gate", "group": "recal2-gate-g05",
     "panels": [{"cand": "80e758ce12", "label": "A: grain smoothed away", "what": "AV1, no grain synthesis"},
                {"cand": "0c7e063304", "label": "B: denoise + AV1 grain", "what": "denoised, AV1 film grain"}],
     "frame": 40, "region": [470, 170, 144, 196], "zoom": 2},
    {"id": "lineart", "film": "Your Name (2016)", "round": "metric-m1-p14", "group": "metric-m1-p14-g01",
     "panels": [{"cand": "b0ac390c54", "label": "A: SVT-AV1 preset 6", "what": "SVT-AV1 4.2.0 preset 6"},
                {"cand": "da8b0a4c40", "label": "B: SVT-AV1 preset 2", "what": "SVT-AV1 4.2.0 preset 2"}],
     "frame": 17, "region": [1792, 448, 96, 130], "zoom": 3},
]

GAP = 8                      # px between panels
PAD = 6                      # px around the caption text
BG = (255, 255, 255)
FG = (20, 20, 20)
DIM = (90, 90, 90)
BAD = (176, 32, 32)
GOOD = (24, 110, 48)


def env_path(name, default=None):
    v = os.environ.get(name, default)
    if not v:
        sys.exit(f"set {name} (see the docstring)")
    return Path(v)


def lab_showings(lab, rnd, group):
    """the group's shown frames and windows, as the review tool exported them to the viewer"""
    sys.path.insert(0, str(lab / "review"))
    import ingest                                            # noqa: E402  (lab module)
    d = lab / rnd
    ex = ingest.export(d)
    remote = {m["local"]: m["remote"] for m in ex["media"]}
    g = next(g for g in ex["groups"] if g["id"] == group)
    verd = {}
    for ln in (d / "verdicts.candidates.jsonl").read_text().splitlines():
        v = json.loads(ln)
        if v["group"] == group:
            verd[v["candidate"]] = v
    ref = g["reference"]
    out = {"fps": float(Fraction(str(g["fps"]))), "start": g["start_frame"], "end": g["end_frame"], "colour": g["colour"],
           "ref": {"path": remote[ref["src"]], "lead": ref["lead_frames"], "crop": ref["crop"]}, "cands": {}}
    for c in g["candidates"]:
        out["cands"][c["id"]] = {"path": remote[c["src"]], "lead": c["lead_frames"], "crop": c["crop"],
                                 "verdict": (verd.get(c["id"]) or {}).get("verdict")}
    return out


def score(grail, fg, sh, cid, rescore):
    CACHE.mkdir(parents=True, exist_ok=True)
    f = CACHE / f"{cid}.json"
    if f.exists() and not rescore:
        return json.loads(f.read_text())
    c, r = sh["cands"][cid], sh["ref"]
    cmd = ["nice", "-n", "15", str(grail), "score", "--ref", r["path"], "--dist", c["path"],
           "--start", str(r["lead"] + sh["start"]), "--dist-start", str(c["lead"] + sh["start"]),
           "--frames", str(sh["end"] - sh["start"]), "--crop", ",".join(map(str, r["crop"])),
           "--dist-crop", ",".join(map(str, c["crop"])), "--colour", sh["colour"], "--threads", "1",
           "--fullgrain-lib", str(fg), "--json"]
    print("scoring", cid, flush=True)
    res = json.loads(subprocess.run(cmd, check=True, capture_output=True, text=True).stdout)
    f.write_text(json.dumps(res))
    return res


def is_av1(path):
    o = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name",
                        "-of", "csv=p=0", path], check=True, capture_output=True, text=True).stdout
    return o.strip() == "av1"


def frame_rgb(path, n, crop, colour, fg):
    """frame n of the file, cropped to the shown window, as 8-bit RGB; AV1 film grain rendered full-frame (the
    av1-fullgrain dav1d replaces the system libdav1d), PQ tone-mapped for the picture only (scores use PQ as is)"""
    x, y, w, h = crop
    env = {k: v for k, v in os.environ.items() if not k.startswith("DAV1D_GRAIN")}
    pre = []
    if is_av1(path):
        pre = ["-c:v", "libdav1d"]
        env["LD_LIBRARY_PATH"] = f"{fg}:{env['LD_LIBRARY_PATH']}" if env.get("LD_LIBRARY_PATH") else str(fg)
        env["DAV1D_GRAIN_MODE"], env["DAV1D_GRAIN_FORCE"] = "full", "1"
    if colour == "pq":
        conv = ("zscale=tin=smpte2084:min=2020_ncl:pin=2020:rin=limited:t=linear:npl=100,format=gbrpf32le,"
                "zscale=p=709,tonemap=tonemap=hable:desat=0,zscale=t=709:m=709:r=full,format=rgb24")
    else:
        conv = "zscale=min=709:tin=709:pin=709:rin=limited:m=709:t=709:p=709:r=full,format=rgb24"
    vf = f"trim=start_frame={n}:end_frame={n + 1},setpts=PTS-STARTPTS,crop={w}:{h}:{x}:{y},{conv}"
    raw = subprocess.run(["nice", "-n", "15", "ffmpeg", "-v", "error", "-nostdin", "-threads", "4", *pre, "-i", path,
                          "-map", "0:v:0", "-vf", vf, "-frames:v", "1", "-f", "rawvideo", "-"],
                         check=True, capture_output=True, env=env).stdout
    return np.frombuffer(raw, np.uint8)[: w * h * 3].reshape(h, w, 3)


def font(bold, size):
    for p in ([os.environ.get("GRAIL_FONT_BOLD" if bold else "GRAIL_FONT")] +
              (["/usr/share/fonts/TTF/DejaVuSans-Bold.ttf", "/usr/share/fonts/noto/NotoSans-Bold.ttf"] if bold else
               ["/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/noto/NotoSans-Regular.ttf"])):
        if p and Path(p).is_file():
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()


def compose(tiles, captions, path):
    """tiles side by side, each with a caption block above: [(text, colour, bold), ...] per tile"""
    th, tw = tiles[0].shape[:2]
    f_b, f_r = font(True, 15), font(False, 14)
    lh = 20
    cap_h = PAD + lh * max(len(c) for c in captions) + PAD
    W = len(tiles) * tw + (len(tiles) - 1) * GAP
    im = Image.new("RGB", (W, cap_h + th), BG)
    d = ImageDraw.Draw(im)
    for i, (t, cap) in enumerate(zip(tiles, captions)):
        x0 = i * (tw + GAP)
        for j, (text, col, bold) in enumerate(cap):
            d.text((x0 + 2, PAD + j * lh), text, fill=col, font=f_b if bold else f_r)
        im.paste(Image.fromarray(t), (x0, cap_h))
    im.save(path, optimize=True)
    return im.size


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only")
    ap.add_argument("--rescore", action="store_true")
    ap.add_argument("--dump", type=int, metavar="FRAME")
    a = ap.parse_args()
    lab = env_path("GRAIL_LAB")
    fg = env_path("GRAIL_FULLGRAIN_LIB")
    grail = env_path("GRAIL_BIN", str(REPO / "target" / "release" / "grail"))
    OUT_IMG.mkdir(parents=True, exist_ok=True)
    jf = OUT_IMG / "examples.json"
    summary = json.loads(jf.read_text()) if jf.exists() else {}
    for ex in EXAMPLES:
        if a.only and ex["id"] != a.only:
            continue
        sh = lab_showings(lab, ex["round"], ex["group"])
        if a.dump is not None:
            CACHE.mkdir(parents=True, exist_ok=True)
            r = sh["ref"]
            Image.fromarray(frame_rgb(r["path"], r["lead"] + sh["start"] + a.dump, r["crop"], sh["colour"], fg)).save(
                CACHE / f"{ex['id']}_f{a.dump}_source.png")
            for p in ex["panels"]:
                c = sh["cands"][p["cand"]]
                Image.fromarray(frame_rgb(c["path"], c["lead"] + sh["start"] + a.dump, c["crop"], sh["colour"],
                                          fg)).save(CACHE / f"{ex['id']}_f{a.dump}_{p['cand']}.png")
            print("dumped", ex["id"], "frame", a.dump, "->", CACHE)
            continue
        rows = []
        for p in ex["panels"]:
            res = score(grail, fg, sh, p["cand"], a.rescore)
            c = sh["cands"][p["cand"]]
            mbps = Path(c["path"]).stat().st_size * 8 / ((sh["end"] + c["lead"]) / sh["fps"]) / 1e6
            rows.append({"label": p["label"], "what": p["what"], "verdict": c["verdict"],
                         "ssimulacra2": round(res["ssimulacra2"], 2), "grails": round(res["score"], 2),
                         "structure": round(res["structure"], 2), "grain_term": round(res["grain"], 2),
                         "lineart_term": round(res["lineart"]["term"], 2), "path": res["path"],
                         "g": round(res["g"], 3), "line_art_a": round(res["lineart"]["a"], 3),
                         "mbps": round(mbps, 2), "frames_scored": res["frames_scored"]})
            model, tool = res["model_version"], res["tool_version"]
        # the picture: the same frame and region of the source and each encode
        x, y, w, h = ex["region"]
        z = ex["zoom"]

        def tile(v):
            cx, cy = v["crop"][0] + x, v["crop"][1] + y
            t = frame_rgb(v["path"], v["lead"] + sh["start"] + ex["frame"], [cx, cy, w, h], sh["colour"], fg)
            return np.repeat(np.repeat(t, z, 0), z, 1)

        tiles = [tile(sh["ref"])] + [tile(sh["cands"][p["cand"]]) for p in ex["panels"]]
        caps = [[("Source", FG, True), (ex["film"], DIM, True), (f"frame {ex['frame']}, enlarged {z}x", DIM, False)]]
        for r in rows:
            caps.append([(r["label"], FG, True),
                         (f"viewer: {r['verdict']}", BAD if r["verdict"] == "fail" else GOOD if r["verdict"] == "pass"
                          else DIM, True),
                         (f"SSIMULACRA2 {r['ssimulacra2']:.1f}   GRAILS {r['grails']:.1f}", DIM, False)])
        img = OUT_IMG / f"example-{ex['id']}.png"
        size = compose(tiles, caps, img)
        summary[ex["id"]] = {"film": ex["film"], "image": f"docs/images/{img.name}", "image_size": list(size),
                             "frame_in_window": ex["frame"], "region_in_window": ex["region"], "zoom": z,
                             "window_frames": sh["end"] - sh["start"], "model_version": model, "tool_version": tool,
                             "mode": "full", "encodes": rows}
        print(f"{ex['film']}: {img} {size[0]}x{size[1]}, {img.stat().st_size // 1024} KB")
        for r in rows:
            print(f"  {r['label']:28s} viewer {r['verdict']:10s} SSIMULACRA2 {r['ssimulacra2']:6.2f}  "
                  f"GRAILS {r['grails']:6.2f}  ({r['mbps']:.2f} Mb/s)")
    if a.dump is None:
        jf.write_text(json.dumps(summary, indent=1) + "\n")


if __name__ == "__main__":
    main()
