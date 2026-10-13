#!/usr/bin/env python3
"""Lab tool: the README's illustrated examples (docs/images/example-*.png and docs/images/examples.json).

Each example is one scene of a blind review round: the source and two encodes the viewer judged, at the same frame
and region. The scores come from this repository's grail (full mode) on exactly the frames and window the viewer saw
(the round's own export: ref / encode lead frames, start and end, crop); the verdicts from the round's verdict file.
Rerun with --rescore after a model update to refresh the numbers in the images and in examples.json (then update the
README and docs/examples.md text). Three kinds: a review round's group (source and encodes, the viewer's verdicts);
"seeds" (one AV1 grain encode with its seeds rewritten, as standard players and as GRAILS render it, scored against the
scene's source; the viewer's ordering from the lab's seed-round record); "table" (a round's group, scores only).

    GRAIL_LAB=<grain-lab dir> GRAIL_FULLGRAIN_LIB=<av1-fullgrain dav1d build/src> python3 tools/make_examples.py
        [--only ID] [--rescore] [--dump FRAME] [--animate]

    --dump FRAME   write the whole shown window of that frame (source and each encode) to results/examples/ instead,
                   for choosing a frame and region
    --animate      also the looping animated AVIFs (docs/images/example-ID.avif) of the examples with "animate":
                   the same panels, captions and crops over a few seconds; libaom, 10-bit 4:4:4, no film-grain
                   synthesis, the best quality within AVIF_MAX bytes whose every frame scores SSIMULACRA2 >= 90
                   (FFVship) against the lossless composite (else fewer frames)

Environment: GRAIL_LAB (the lab directory with the review rounds and review/ingest.py; required), GRAIL_FULLGRAIN_LIB
(required: AV1 film grain is rendered full-frame, in the scores and in the pictures), GRAIL_BIN (default
target/release/grail), GRAIL_FONT / GRAIL_FONT_BOLD (TrueType fonts for the labels), GRAIL_MARK_FONT (a font with
the check and cross marks; default DejaVu Sans Bold). Needs numpy, Pillow, and ffmpeg
with libdav1d and zscale. Grail runs at nice 15 with one decoder thread per stream.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
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
     "frame": 40, "region": [470, 170, 144, 196], "zoom": 2, "animate": [6, 48]},
    {"id": "grain-bs", "film": "Black Swan (2010)", "round": "recal-fix3", "group": "fix3-g04",
     "panels": [{"cand": "84dcc56b6d", "label": "A: AV1 grain, half strength",
                 "what": "AV1 film grain at half strength"},
                {"cand": "b8ebb10f32", "label": "B: AV1 grain, full strength",
                 "what": "AV1 film grain fitted to the source"}],
     "frame": 40, "region": [940, 200, 144, 196], "zoom": 2},
    {"id": "grain-hunt", "film": "The Hunt (2012)", "round": "recal2-gate", "group": "recal2-gate-g03",
     "panels": [{"cand": "2f3e7ed90e", "label": "A: grain smoothed away", "what": "AV1, no grain synthesis"},
                {"cand": "0bbfd715e2", "label": "B: denoise + AV1 grain", "what": "denoised, AV1 film grain"}],
     "frame": 75, "region": [640, 140, 96, 130], "zoom": 3},
    {"id": "lineart", "film": "Your Name (2016)", "round": "metric-m1-p14", "group": "metric-m1-p14-g01",
     "panels": [{"cand": "b0ac390c54", "label": "A: SVT-AV1 preset 6", "what": "SVT-AV1 4.2.0 preset 6"},
                {"cand": "da8b0a4c40", "label": "B: SVT-AV1 preset 2", "what": "SVT-AV1 4.2.0 preset 2"}],
     "frame": 17, "region": [1792, 448, 96, 130], "zoom": 3, "animate": [6, 54]},
    # the same AV1 film-grain encode with its seeds rewritten in place (no re-encode): the worst- and best-rated seed
    # per frame, as standard players render it, and the worst-seed copy rendered full-frame (what GRAILS scores); the
    # viewer's ordering of these versions comes from the lab's seed-round record. Second row: the grain alone.
    {"id": "seeds", "kind": "seeds", "film": "Black Swan (2010)", "ref": "scenes/black_swan_5786/src.mkv",
     "versions": {"worst": "denoise/seeds/seeds_tmpl_worst.mkv", "best": "denoise/seeds/seeds_tmpl_best.mkv",
                  "default": "denoise/gd/black_swan_5786_gd2_crf31.mkv"},
     "ordering": ["grainscore/seed_rounds_check.json", "BS 5786 template round"],
     "panels": [{"version": "worst", "render": "standard", "label": "A: worst-rated seeds",
                 "sub": "standard AV1 grain"},
                {"version": "best", "render": "standard", "label": "B: best-rated seeds", "sub": "standard AV1 grain"},
                {"version": "worst", "render": "full", "label": "A, full-frame grain", "sub": "what GRAILS scores"}],
     "crop": [0, 140, 1920, 800], "frame": 58, "region": [816, 64, 144, 120], "zoom": 2, "grain_gain": 6,
     "animate": [40, 48]},
    # clean footage: scores only (GRAILS equals SSIMULACRA2 when the source has no grain and no line art)
    {"id": "clean", "kind": "table", "film": "1917 (2019)", "round": "metric-m1-p3", "group": "metric-m1-p3-g01",
     "panels": [{"cand": "96995966a8", "label": "x264, CRF 13", "what": "x264 CRF 13"},
                {"cand": "9b90513e25", "label": "x265, CRF 17", "what": "x265 CRF 17"},
                {"cand": "bfce4f74b4", "label": "SVT-AV1, CRF 10", "what": "SVT-AV1 CRF 10"},
                {"cand": "2c313ff973", "label": "SVT-AV1, CRF 17", "what": "SVT-AV1 CRF 17"}]},
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
    fr = Fraction(str(g["fps"])).limit_denominator(1001)
    out = {"fps": float(fr), "fps_str": str(fr), "start": g["start_frame"], "end": g["end_frame"], "colour": g["colour"],
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


def score_files(grail, fg, ref, dist, key, rescore):
    """a whole scene file against its source (all frames, the active picture)"""
    CACHE.mkdir(parents=True, exist_ok=True)
    f = CACHE / f"{key}.json"
    if f.exists() and not rescore:
        return json.loads(f.read_text())
    cmd = ["nice", "-n", "15", str(grail), "score", "--ref", str(ref), "--dist", str(dist), "--threads", "1",
           "--fullgrain-lib", str(fg), "--json"]
    print("scoring", key, flush=True)
    res = json.loads(subprocess.run(cmd, check=True, capture_output=True, text=True).stdout)
    f.write_text(json.dumps(res))
    return res


def video_mbps(path, fps):
    """the video stream's bitrate: its packets' bytes over its frame count / fps (container overhead and other tracks
    left out)"""
    o = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "packet=size", "-of",
                        "csv=p=0", str(path)], check=True, capture_output=True, text=True).stdout.split()
    sizes = [int(x.strip(",")) for x in o if x.strip(",").isdigit()]
    return sum(sizes) * 8 / (len(sizes) / fps) / 1e6


def is_av1(path):
    o = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name",
                        "-of", "csv=p=0", path], check=True, capture_output=True, text=True).stdout
    return o.strip() == "av1"


def frame_rgb(path, n, crop, colour, fg, grain="full"):
    return frames_rgb(path, n, 1, crop, colour, fg, grain)[0]


def frames_rgb(path, n, count, crop, colour, fg, grain="full"):
    """frames n .. n + count - 1 of the file, cropped, as 8-bit RGB [count, h, w, 3]; AV1 film grain rendered
    full-frame (the
    av1-fullgrain dav1d replaces the system libdav1d) or, with grain="standard", as standard AV1 grain (the
    av1-fullgrain build in its conformant mode, identical to stock dav1d); PQ tone-mapped for the picture only (scores
    use PQ as is)"""
    x, y, w, h = crop
    env = {k: v for k, v in os.environ.items() if not k.startswith("DAV1D_GRAIN")}
    pre = []
    if is_av1(path) and grain == "none":                 # the grain-free decode (as grail's structure path)
        pre = ["-c:v", "libdav1d", "-filmgrain", "0"]
        env["DAV1D_GRAIN_FORCE"] = "0"
    elif is_av1(path):
        pre = ["-c:v", "libdav1d"]
        env["LD_LIBRARY_PATH"] = f"{fg}:{env['LD_LIBRARY_PATH']}" if env.get("LD_LIBRARY_PATH") else str(fg)
        env["DAV1D_GRAIN_MODE"], env["DAV1D_GRAIN_FORCE"] = grain, "1"
    if colour == "pq":
        conv = ("zscale=tin=smpte2084:min=2020_ncl:pin=2020:rin=limited:t=linear:npl=100,format=gbrpf32le,"
                "zscale=p=709,tonemap=tonemap=hable:desat=0,zscale=t=709:m=709:r=full,format=rgb24")
    else:
        conv = "zscale=min=709:tin=709:pin=709:rin=limited:m=709:t=709:p=709:r=full,format=rgb24"
    vf = f"trim=start_frame={n}:end_frame={n + count},setpts=PTS-STARTPTS,crop={w}:{h}:{x}:{y},{conv}"
    raw = subprocess.run(["nice", "-n", "15", "ffmpeg", "-v", "error", "-nostdin", "-threads", "4", *pre, "-i", path,
                          "-map", "0:v:0", "-vf", vf, "-frames:v", str(count), "-f", "rawvideo", "-"],
                         check=True, capture_output=True, env=env).stdout
    a = np.frombuffer(raw, np.uint8)
    if a.size != count * w * h * 3:
        raise RuntimeError(f"{path}: got {a.size // (w * h * 3)} of {count} frames")
    return a.reshape(count, h, w, 3)


def font(bold, size):
    for p in ([os.environ.get("GRAIL_FONT_BOLD" if bold else "GRAIL_FONT")] +
              (["/usr/share/fonts/TTF/DejaVuSans-Bold.ttf", "/usr/share/fonts/noto/NotoSans-Bold.ttf"] if bold else
               ["/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/noto/NotoSans-Regular.ttf"])):
        if p and Path(p).is_file():
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()


CHECK, CROSS = "\u2713", "\u2717"   # need a font with both glyphs (DejaVu Sans has them)


def mark_font(size):
    for p in (os.environ.get("GRAIL_MARK_FONT"), "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
              "/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
              "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"):
        if p and Path(p).is_file():
            return ImageFont.truetype(p, size)
    sys.exit("no font with check and cross marks: set GRAIL_MARK_FONT (e.g. DejaVu Sans)")
RANK = {"pass": 2, "borderline": 1, "fail": 0}


def marks(rows):
    """for a pair the viewer judged differently: per metric, a green check on the score of the encode it rightly
    prefers, or a red cross on the score of the encode it wrongly prefers; nothing for a tie, a single encode or a
    missing verdict. rows: verdict and the UNROUNDED scores"""
    out = [{} for _ in rows]
    if len(rows) != 2 or any(r["verdict"] not in RANK for r in rows):
        return out
    good, bad = sorted(range(2), key=lambda i: -RANK[rows[i]["verdict"]])
    if RANK[rows[good]["verdict"]] == RANK[rows[bad]["verdict"]]:
        return out
    for m in ("ssimulacra2", "grails"):
        if rows[good][m] > rows[bad][m]:
            out[good][m] = CHECK
        elif rows[bad][m] > rows[good][m]:
            out[bad][m] = CROSS
    return out


def compose(tiles, captions, path, row2=None, row2_caption=""):
    im = compose_image(tiles, captions, row2, row2_caption)
    im.save(path, optimize=True)
    return im.size


def compose_image(tiles, captions, row2=None, row2_caption=""):
    """tiles side by side, each with a caption block above: [(text, colour, bold), ...] per tile, or a line of such
    segments (a list); row2: a second row of tiles under a one-line caption"""
    th, tw = tiles[0].shape[:2]
    f_b, f_r, f_m = font(True, 15), font(False, 14), mark_font(14)
    lh = 20
    cap_h = PAD + lh * max(len(c) for c in captions) + PAD
    W = len(tiles) * tw + (len(tiles) - 1) * GAP
    r2_h = (PAD + lh + PAD + th) if row2 else 0
    im = Image.new("RGB", (W, cap_h + th + r2_h), BG)
    d = ImageDraw.Draw(im)
    for i, (t, cap) in enumerate(zip(tiles, captions)):
        x0 = i * (tw + GAP)
        for j, line in enumerate(cap):
            xx = x0 + 2
            for text, col, bold in (line if isinstance(line, list) else [line]):
                fo = f_m if text.strip() in (CHECK, CROSS) else f_b if bold else f_r
                d.text((xx, PAD + j * lh), text, fill=col, font=fo)
                xx += d.textlength(text, font=fo)
        im.paste(Image.fromarray(t), (x0, cap_h))
    if row2:
        d.text((2, cap_h + th + PAD), row2_caption, fill=DIM, font=f_r)
        for i, t in enumerate(row2):
            im.paste(Image.fromarray(t), (i * (tw + GAP), cap_h + th + PAD + lh + PAD))
    return im


AVIF_MAX = 3_000_000              # bytes per animation
AVIF_MIN_SSIMU2 = 90.0            # every frame of the animation against its lossless composite
AVIF_CRFS = (2, 4, 6, 10, 14, 18, 22, 26, 30)
BT709_FULL = ["-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "pc"]


def ffvship_frames(ref, dist):
    """per-frame SSIMULACRA2 (FFVship) of two videos with the same frames"""
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / "s.json"
        subprocess.run(["nice", "-n", "15", "FFVship", "-s", str(ref), "-e", str(dist), "-m", "SSIMULACRA2", "--json",
                        str(out), "-t", "2", "-g", "3"], check=True, capture_output=True)
        return [f[0] if isinstance(f, list) else f for f in json.loads(out.read_text())]


def animate(name, make, n, fps):
    """make(k) -> the first k composites (PIL images, one per video frame); -> docs/images/<name>.avif, a looping
    animated AVIF:
    libaom AV1, 10-bit 4:4:4, BT.709 full range, no film-grain synthesis; the lowest CRF (best quality) that fits
    AVIF_MAX, checked frame by frame against the lossless composites with SSIMULACRA2 (FFVship); if no CRF passes both,
    fewer frames"""
    out = OUT_IMG / f"{name}.avif"
    while True:
        frames = make(n)
        with tempfile.TemporaryDirectory(dir=CACHE) as td:
            td = Path(td)
            for i, im in enumerate(frames):
                im.save(td / f"{i:04d}.png")
            ref = td / "ref.mkv"
            # FFVship reads YUV only: the lossless composites as 10-bit 4:4:4 full range BT.709, the same conversion
            # the AVIF gets, so the check measures the AV1 coding loss
            subprocess.run(["nice", "-n", "15", "ffmpeg", "-v", "error", "-nostdin", "-y", "-framerate", fps, "-i",
                            str(td / "%04d.png"), "-vf",
                            "scale=out_color_matrix=bt709:out_range=full,format=yuv444p10le",
                            "-c:v", "ffv1", *BT709_FULL, str(ref)], check=True)
            tried = []
            for crf in AVIF_CRFS:
                tmp = td / "a.avif"
                subprocess.run(["nice", "-n", "15", "ffmpeg", "-v", "error", "-nostdin", "-y", "-framerate", fps,
                                "-i", str(td / "%04d.png"), "-vf",
                                "scale=out_color_matrix=bt709:out_range=full,format=yuv444p10le",
                                "-c:v", "libaom-av1", "-crf", str(crf), "-b:v", "0", "-cpu-used", "4", "-row-mt", "1",
                                "-threads", "2", "-denoise-noise-level", "0", *BT709_FULL,
                                "-loop", "0", str(tmp)], check=True)
                size = tmp.stat().st_size
                if size > AVIF_MAX:
                    tried.append((crf, size, None))
                    continue
                dist = td / "dist.mkv"
                # an animated AVIF holds a still item (stream 0) and the sequence (the stream with the most frames)
                st = json.loads(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=index,nb_frames",
                                                "-of", "json", str(tmp)], check=True, capture_output=True,
                                               text=True).stdout)["streams"]
                seq_i = max(st, key=lambda x: int(x.get("nb_frames") or 0))["index"]
                subprocess.run(["nice", "-n", "15", "ffmpeg", "-v", "error", "-nostdin", "-y", "-i", str(tmp),
                                "-map", f"0:{seq_i}", "-pix_fmt", "yuv444p10le", "-c:v", "ffv1", *BT709_FULL,
                                str(dist)], check=True)
                sc = ffvship_frames(ref, dist)
                if len(sc) != len(frames):
                    raise RuntimeError(f"{name}: {len(sc)} decoded frames, expected {len(frames)}")
                tried.append((crf, size, round(min(sc), 2), sc.index(min(sc))))
                if min(sc) >= AVIF_MIN_SSIMU2:
                    out.write_bytes(tmp.read_bytes())
                    return {"file": f"docs/images/{out.name}", "frames": len(frames), "fps": fps, "crf": crf,
                            "bytes": size, "ssimulacra2_min": round(min(sc), 2),
                            "ssimulacra2_mean": round(sum(sc) / len(sc), 2), "tried": tried}
                break                       # the best quality that fits misses the guard: fewer frames
        if len(frames) <= 24:
            raise RuntimeError(f"{name}: no CRF meets {AVIF_MIN_SSIMU2} within {AVIF_MAX} bytes: {tried}")
        print(f"  {name}: {len(frames)} frames do not fit ({tried}); trying fewer", flush=True)
        n = n * 3 // 4


def seeds_example(ex, lab, fg, grail, rescore, anim=False):
    """the same AV1 grain encode with different seeds: standard vs full-frame rendering, and the grain alone"""
    ref = lab / ex["ref"]
    rep = {k: score_files(grail, fg, ref, lab / v, f"{ex['id']}_{k}", rescore) for k, v in ex["versions"].items()}
    order = next(k for k in json.loads((lab / ex["ordering"][0]).read_text()) if k.startswith(ex["ordering"][1]))
    x, y, w, h = ex["region"]
    cx, cy = ex["crop"][0] + x, ex["crop"][1] + y
    z, f = ex["zoom"], ex["frame"]
    up = lambda a: np.repeat(np.repeat(a, z, 0), z, 1)
    tiles, grain, caps = [], [], []
    for p in ex["panels"]:
        path = str(lab / ex["versions"][p["version"]])
        shown = frame_rgb(path, f, [cx, cy, w, h], "sdr", fg, p["render"]).astype(np.float32)
        clean = frame_rgb(path, f, [cx, cy, w, h], "sdr", fg, "none").astype(np.float32)
        g = (shown - clean).mean(2)
        tiles.append(up(shown.astype(np.uint8)))
        grain.append(up(np.repeat(np.clip(128 + ex["grain_gain"] * g, 0, 255).astype(np.uint8)[:, :, None], 3, 2)))
        r = rep[p["version"]]
        sv = r["std_decoder"]
        third = (f"seed view {sv['seed_pct_mean']:.0f} (0 best, 100 worst)" if p["render"] == "standard" else
                 f"GRAILS {r['score']:.1f}")
        caps.append([(p["label"], FG, True), (p["sub"], DIM, True), (third, DIM, False)])
    img = OUT_IMG / f"example-{ex['id']}.png"
    size = compose(tiles, caps, img, grain,
                   f"the grain alone (shown minus the grain-free decode), contrast x{ex['grain_gain']}")
    rows = [{"version": k, "ssimulacra2": round(v["ssimulacra2"], 2), "grails": round(v["score"], 2), "path": v["path"],
             "frames_scored": v["frames_scored"],
             "seed_view": {kk: v["std_decoder"].get(kk) for kk in ("seed_frames", "seed_pct_mean", "seed_pct_p90",
                                                                     "seed_pct_worst", "seed_bad_share",
                                                                     "seed_degenerate")}}
            for k, v in rep.items()]
    any_r = next(iter(rep.values()))
    animation = None
    if anim and ex.get("animate"):
        f0, n = ex["animate"]
        seqs = [frames_rgb(str(lab / ex["versions"][p["version"]]), f0, n, [cx, cy, w, h], "sdr", fg, p["render"])
                for p in ex["panels"]]
        def make(k):
            acaps = [c + ([(f"frames {f0}-{f0 + k - 1}, looping", DIM, False)] if i == 0 else [])
                     for i, c in enumerate(caps)]
            return [compose_image([up(sq[i]) for sq in seqs], acaps) for i in range(k)]

        animation = {**animate(f"example-{ex['id']}", make, n, "24000/1001"), "first_frame": f0}
        print(f"  animation: {animation}")
    out = {"film": ex["film"], "image": f"docs/images/{img.name}", "image_size": list(size), "frame": f,
           "region_in_picture": ex["region"], "zoom": z, "grain_gain": ex["grain_gain"],
           "viewer_ordering": order.split("(", 1)[1].rstrip(")"), "model_version": any_r["model_version"],
           "tool_version": any_r["tool_version"], "mode": "full", "versions": rows,
           **({"animation": animation} if animation else {})}
    print(f"{ex['film']}: {img} {size[0]}x{size[1]}, {img.stat().st_size // 1024} KB; viewer: {out['viewer_ordering']}")
    for r in rows:
        print(f"  {r['version']:8s} SSIMULACRA2 {r['ssimulacra2']:6.2f}  GRAILS {r['grails']:6.2f}  path {r['path']}  "
              f"seed view {r['seed_view']}")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only")
    ap.add_argument("--rescore", action="store_true")
    ap.add_argument("--dump", type=int, metavar="FRAME")
    ap.add_argument("--animate", action="store_true", help="also the looping AVIF animations (examples with 'animate')")
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
        if a.dump is not None and ex.get("kind") in ("seeds", "table"):
            continue                                  # --dump is for the round examples' frame and region
        if ex.get("kind") == "seeds":
            prev = summary.get(ex["id"], {}).get("animation")
            summary[ex["id"]] = seeds_example(ex, lab, fg, grail, a.rescore, a.animate)
            if prev and "animation" not in summary[ex["id"]]:
                summary[ex["id"]]["animation"] = prev       # keep the last animation's record without --animate
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
        rows, raw = [], []
        for p in ex["panels"]:
            res = score(grail, fg, sh, p["cand"], a.rescore)
            c = sh["cands"][p["cand"]]
            mbps = video_mbps(c["path"], sh["fps"])
            raw.append({"verdict": c["verdict"], "ssimulacra2": res["ssimulacra2"], "grails": res["score"]})
            rows.append({"label": p["label"], "what": p["what"], "verdict": c["verdict"],
                         "ssimulacra2": round(res["ssimulacra2"], 2), "grails": round(res["score"], 2),
                         "structure": round(res["structure"], 2), "grain_term": round(res["grain"], 2),
                         "lineart_term": round(res["lineart"]["term"], 2), "path": res["path"],
                         "g": round(res["g"], 3), "line_art_a": round(res["lineart"]["a"], 3),
                         "mbps": round(mbps, 2), "frames_scored": res["frames_scored"]})
            model, tool = res["model_version"], res["tool_version"]
        if ex.get("kind") == "table":
            summary[ex["id"]] = {"film": ex["film"], "window_frames": sh["end"] - sh["start"], "model_version": model,
                                 "tool_version": tool, "mode": "full", "encodes": rows}
            print(f"{ex['film']} (scores only)")
            for r in rows:
                print(f"  {r['label']:28s} viewer {r['verdict']:10s} SSIMULACRA2 {r['ssimulacra2']:6.2f}  "
                      f"GRAILS {r['grails']:6.2f}  (g {r['g']}, a {r['line_art_a']})")
            continue
        # the picture: the same frame and region of the source and each encode
        x, y, w, h = ex["region"]
        z = ex["zoom"]

        def tile(v):
            cx, cy = v["crop"][0] + x, v["crop"][1] + y
            t = frame_rgb(v["path"], v["lead"] + sh["start"] + ex["frame"], [cx, cy, w, h], sh["colour"], fg)
            return np.repeat(np.repeat(t, z, 0), z, 1)

        tiles = [tile(sh["ref"])] + [tile(sh["cands"][p["cand"]]) for p in ex["panels"]]
        caps = [[("Source", FG, True), (ex["film"], DIM, True), (f"frame {ex['frame']}, enlarged {z}x", DIM, False)]]
        mk = marks(raw)
        for r, m in zip(rows, mk):
            score_line = [(f"SSIMULACRA2 {r['ssimulacra2']:.1f}", DIM, False)]
            if "ssimulacra2" in m:
                score_line.append((" " + m["ssimulacra2"], GOOD if m["ssimulacra2"] == CHECK else BAD, True))
            score_line.append((f"   GRAILS {r['grails']:.1f}", DIM, False))
            if "grails" in m:
                score_line.append((" " + m["grails"], GOOD if m["grails"] == CHECK else BAD, True))
            caps.append([(r["label"], FG, True),
                         (f"viewer: {r['verdict']}", BAD if r["verdict"] == "fail" else GOOD if r["verdict"] == "pass"
                          else DIM, True),
                         score_line])
            r["marks"] = {k: ("wrong" if v == CROSS else "right") for k, v in m.items()}
        img = OUT_IMG / f"example-{ex['id']}.png"
        size = compose(tiles, caps, img)
        animation = summary.get(ex["id"], {}).get("animation")
        if a.animate and ex.get("animate"):
            f0, n = ex["animate"]

            def seq(v):
                cx, cy = v["crop"][0] + x, v["crop"][1] + y
                return frames_rgb(v["path"], v["lead"] + sh["start"] + f0, n, [cx, cy, w, h], sh["colour"], fg)

            seqs = [seq(sh["ref"])] + [seq(sh["cands"][p["cand"]]) for p in ex["panels"]]
            up = lambda t: np.repeat(np.repeat(t, z, 0), z, 1)

            def make(k):
                acaps = [[caps[0][0], caps[0][1], (f"frames {f0}-{f0 + k - 1}, enlarged {z}x", DIM, False)]] + caps[1:]
                return [compose_image([up(sq[i]) for sq in seqs], acaps) for i in range(k)]

            animation = {**animate(f"example-{ex['id']}", make, n, sh["fps_str"]), "first_frame_in_window": f0}
            print(f"  animation: {animation}")
        summary[ex["id"]] = {"film": ex["film"], "image": f"docs/images/{img.name}", "image_size": list(size),
                             "frame_in_window": ex["frame"], "region_in_window": ex["region"], "zoom": z,
                             "window_frames": sh["end"] - sh["start"], "model_version": model, "tool_version": tool,
                             "mode": "full", "encodes": rows, **({"animation": animation} if animation else {})}
        print(f"{ex['film']}: {img} {size[0]}x{size[1]}, {img.stat().st_size // 1024} KB")
        for r in rows:
            print(f"  {r['label']:28s} viewer {r['verdict']:10s} SSIMULACRA2 {r['ssimulacra2']:6.2f}  "
                  f"GRAILS {r['grails']:6.2f}  ({r['mbps']:.2f} Mb/s)")
    if a.dump is None:
        jf.write_text(json.dumps(summary, indent=1) + "\n")


if __name__ == "__main__":
    main()
