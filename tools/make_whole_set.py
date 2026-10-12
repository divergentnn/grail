#!/usr/bin/env python3
"""Lab tool: every judged showing of the Python reference as a check-reference case file (the port_reference.json
format), for the whole-set check (PORT_SPEC.md section 12: Spearman >= 0.995 between the port's Q and the reference's).

    make_whole_set.py LAB_GRAINSCORE_DIR OUT.json
"""
import json
import sys
from pathlib import Path

lab = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(lab))
import evaluate as E  # noqa: E402

model = json.loads((lab / "weights.json").read_text())
cases = []
for r in E.load("full"):
    ft = r["ft"]
    if not ft:
        continue
    q, g, a = E.model_score(ft, r["bl"]["ssimu2"], model, r["la"])
    st, path = E.structure(ft, model["struct"])
    p = E.penalties(ft)
    la = r["la"] or {}
    cases.append({
        "case": r["film"], "key": r["k"],
        "inputs": {"ref": r["ref"], "cand": r["dist"], "start_frame": r["start_frame"], "end_frame": r["end_frame"],
                   "colour": r["colour"]},
        "S0_ssimulacra2": r["bl"]["ssimu2"], "S0_frames": r["bl"].get("ssimu2_frames"),
        "S_D": ft["sd_ssimu2"], "S_D_frames": ft.get("sd_ssimu2_frames"),
        "S_Dp": ft.get("sdp_ssimu2"), "S_Dp_frames": ft.get("sdp_ssimu2_frames"),
        "grain_capture_R": round(E.grain_capture(ft), 5), "path_used": path, "S_struct": st, "g": g,
        "features": {k: ft.get(k) for k in ("sigma_g", "P_s", "P_c", "n_lum", "chroma_s", "chroma_c", "stick_c",
                                            "stick_s", "stickc_c", "stickc_s", "tex_energy_ratio", "flat_frac",
                                            "active", "cand_active", "path", "codec")},
        "penalties": {t: p[t] for t in model["terms"]},
        "weighted_penalty_sum": float(sum(w * p[t] for t, w in zip(model["terms"], model["weights"]))),
        "lineart": {"a": a, "inkflat_s": la.get("inkflat_s"), "tex6": la.get("tex6"),
                    "near_top_frames": la.get("near_top_frames"), "p_L": E.lineart_penalty(la) if la else 0.0},
        "Q": q,
        "seed_view": {k: v for k, v in E.seed_view(ft.get("seeds") or [], ft["cand_full"][1], ft.get("seed_pct")).items()
                      if k != "seed_penalty_frames"},
    })
Path(sys.argv[2]).write_text(json.dumps({"model": model, "cases": cases}))
print(len(cases), "cases")
