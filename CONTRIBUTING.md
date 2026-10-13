# Contributing to GRAIL

There are two ways to help: judge encodes for the calibration of GRAILS (needed most), and contribute code.

## Help calibrate GRAILS

### Why

GRAILS (model 0.2.1) is fitted on the blind judgments of one viewer: 574 showings of 13 films, on one display. That
is enough to rank encodes of the same scene well, but not to say where an average viewer starts to see a problem.
GRAILS 1.0 needs:

- **more viewers**, so the scores reflect more than one person's tolerance;
- **more films**;
- **more kinds of content**: grainy live action, anime and other 2D animation, clean CGI, clean digital live action;
- **more displays**: TVs and monitors, SDR and HDR, different sizes and viewing distances.

### You share no video

Contributors never send video, frames or screenshots. You:

1. pick a few scenes from films you have, and make several encodes of each;
2. judge the encodes blind against the source, following the protocol below;
3. score each encode with `grail score --json`, on exactly the frames and the window you judged;
4. send in the JSON reports with your verdicts.

The JSON report holds every number the model is fitted on, so the lab can refit GRAILS from it without the video.
For model 0.2.1, these fields matter:

| field | what it is |
|---|---|
| `ssimulacra2`, `per_frame.ssimulacra2` | plain SSIMULACRA2 of the source and the encode as watched |
| `s_d`, `s_dp` (AV1 film-grain encodes), `path`, `grain_capture_r`, `structure` | the structure (picture without grain) comparison |
| `features`: `sigma_g`, `flat_frac`, `P_s`, `P_c`, `n_lum`, `chroma_s`, `chroma_c`, `stick_s`, `stick_c`, `stickc_s`, `stickc_c`, `tex_energy_ratio` | the grain statistics of the source and the encode, per brightness level |
| `lineart`: `a`, `inkflat_s`, `tex6`, `peak`, `p_l`; `per_frame.lineart_near_top` | the line-art detector and the ringing measure |
| `model_version`, `tool_version`, `mode`, `frames`, `frames_scored`, `window`, `active`, `colour`, `codec` | what was scored, and how |

Send the whole report anyway: it is small, and the rest (`score`, `g`, `penalties`, `per_frame`) makes checking
easier. The report holds no file names (batch output without `--json` does: it lists `ref` and `dist`; use `--json`).
Keep your sources and encodes: a later model may measure something new, and then the lab may ask you to rescore them
with a newer `grail`.

### The review protocol

These are the conditions the current verdicts were made under. Verdicts made otherwise can't be mixed with them.

- **Blind.** The versions of a scene are shuffled and shown under neutral labels (A, B, C ...). You learn which is
  which only after all verdicts for the scene are saved, and you don't change a verdict after that.
- **The source is always there.** You can flip between the source and the version you're judging at any moment, on
  the same frame and the same spot.
- **1:1 pixels, no scaling.** One video pixel per screen pixel. For a 4K source on a smaller screen, judge a
  1920x1080 window of the picture, and score that same window (`grail score --crop x,y,w,h`).
- **Short scenes, looping.** A few seconds (about 3 to 5) per scene, playing in a loop. Watch it in normal playback,
  and pause or step frame by frame as you like.
- **AV1 film grain rendered over the whole frame.** See the box below.

> [!IMPORTANT]
> **Review AV1 film-grain encodes only with full-frame grain.**
>
> An AV1 encode that uses film-grain synthesis carries no grain, only a short description of it and a random seed
> for each frame; the player makes the grain when it plays the video. Standard decoders tile one small grain patch
> across the picture. With some seeds that patch has clumps or holes, and tiling repeats them all over the frame:
> easy to spot, and not the encode's fault
> ([example](docs/examples.md#av1-grain-seeds-in-standard-players-black-swan-2010)). Viewers then fail an encode for
> the decoder's grain, not for the encode's quality, and verdicts made that way would teach GRAILS the wrong thing.
>
> So judge AV1 grain encodes only with full-frame rendering:
> [av1-fullgrain](https://github.com/divergentnn/av1-fullgrain), which draws the grain over the whole frame (it
> replaces the system's dav1d; its install guides cover mpv), or `grail review` once it exists, which will render
> the grain itself. Check before you start, with av1-fullgrain's own test (two checksums, in its install guide), and
> keep AV1 on software decoding in your player: hardware decoders don't use it.
>
> Verdicts on AV1 grain encodes reviewed with standard (tiled) grain can't be accepted. Each verdict records how the
> grain was rendered (`grain_render` below).

**Verdicts.** Judge each version on its own against the source, in normal playback and paused at 1:1. The lab's
instructions read: "Pass = a faithful watching copy." In full:

- **pass**: a faithful watching copy; you would happily watch it instead of the source;
- **borderline**: on the edge; you can find a difference, but you aren't sure it would bother you while watching;
- **fail**: a difference you wouldn't accept in a watching copy.

For borderline and fail, tag the cause: `blur` (lost sharpness or detail), `face` (face or hair detail), `chroma`
(colour), `block` (blocking), `halo` (ringing or halos around edges and lines), `pattern` (grain that looks wrong:
missing, smeared, blotchy, frozen, repeating). A short note helps ("smoothed skin", "halo next to the hair line,
frame 58").

**Pair votes** (optional). Some groups show two versions of the same scene, A and B. Vote which is closer to the
source: `A`, `B`, `tie` or `cant_tell`, with a confidence (`low`, `medium`, `high`), and still give each version
its own verdict.

**A good set** has several versions per scene (a CRF ladder, presets, grain settings, encoders), from clearly good to
clearly bad, with many near your own threshold. A hidden copy of the source among the versions is a useful check (it
should pass; mark it `"null": true` in the data).

**Viewing conditions.** Record them once per setup: the display (make and model, or type: OLED, LCD, projector), its
size and your distance from it, SDR or HDR (and the peak brightness if you know it), the room light, and the player.

### Tools

A `grail review` command is coming: a blind review tool for Linux, macOS and Windows that follows this protocol and
renders AV1 grain itself, so you won't need to install av1-fullgrain. Until then, use **mpv**, set up for the
protocol:

- **AV1 grain:** av1-fullgrain installed as described in its mpv guide, and the check in the box above;
- **1:1 pixels:** `--video-unscaled=yes`, and for 4K sources on a smaller screen, `--video-crop=1920x1080+X+Y` with
  the window you will score;
- **looping:** `--loop-file=inf`;
- **blind:** copy or link the versions under neutral names (`A.mkv`, `B.mkv` ...) with a small script that writes
  which is which to a key file you open only after judging;
- **flipping:** load the versions as extra video tracks of the source (`--external-files=A.mkv:B.mkv`) and switch
  tracks (`_` cycles the video track) to flip on the same frame.

Write the verdicts down as you go, in the format below.

### The data format

One directory per reviewer, named by a pseudonym:

```
calibration/<reviewer>/
  viewing.json          viewing conditions, one entry per setup
  verdicts.jsonl        one line per showing (one version judged in one group)
  reports/<showing>.json   the `grail score --json` report of that showing
```

`viewing.json`:

```json
{"tv": {"display": "OLED TV, 55 inch", "distance_m": 1.6, "range": "SDR", "peak_nits": null,
        "room": "dim", "player": "mpv 0.40 with av1-fullgrain"}}
```

`verdicts.jsonl`, one line per showing:

```json
{"showing": "r1-g03-B", "group": "r1-g03", "round": "r1", "letter": "B", "scene": "s07", "film": "f2",
 "title": "The Lighthouse (2019)", "content": {"type": "live action", "grain": "heavy", "hdr": false},
 "window": {"start": 1200, "frames": 96, "crop": [312, 0, 1296, 1032]},
 "encode": {"encoder": "SVT-AV1 4.2.0", "preset": 6, "crf": 31, "grain": "film-grain synthesis", "mbps": 0.9},
 "report": "reports/r1-g03-B.json", "verdict": "pass", "tags": [], "note": "",
 "vote": "B", "vote_confidence": "medium", "grain_render": "full-frame", "null": false,
 "review": {"tool": "mpv", "grain_renderer": "av1-fullgrain dav1d", "gpu": "RTX 3060", "fullframe_check": true},
 "reviewer": "<pseudonym>", "viewing": "tv", "date": "2026-10-20"}
```

| field | required | meaning |
|---|---|---|
| `showing` | yes | unique id of this showing; also the report's file name |
| `group` | yes | the versions judged together (one scene, one sitting); pair votes belong to a group |
| `round` | yes | a sitting or batch of groups |
| `letter` | yes | the blind label the version had in its group |
| `scene` | yes | your id for the scene (the same scene in several groups keeps its id) |
| `film` | yes | your id for the film (scenes of one film share it; evaluation holds out whole films) |
| `title` | no | the film's title and year, if you want to give it. Never file names, release or source details |
| `content` | yes | `type` (`live action`, `2D animation`, `CGI`), `grain` (`none`, `light`, `medium`, `heavy`), `hdr` |
| `window` | no | the frames and crop you judged and scored (also in the report: `window`, `frames`) |
| `encode` | no | what the version was, free-form (encoder, preset, CRF, grain settings, bitrate) |
| `report` | yes | path of the `grail score --json` report |
| `verdict` | yes | `pass`, `borderline` or `fail` |
| `tags`, `note` | no | the cause tags and your note |
| `vote`, `vote_confidence` | pair groups | `A` / `B` / `tie` / `cant_tell` and `low` / `medium` / `high`, the same on both lines of the pair |
| `grain_render` | yes | `full-frame` (an AV1 grain encode rendered with full-frame grain) or `not AV1 grain` |
| `null` | no | `true` if this showing is a hidden copy of the source shown as one of the versions (a check on the review) |
| `review` | no | how you reviewed: `tool` (`grail review x.y`, `mpv` ...), `grain_renderer` (`av1-fullgrain dav1d`, `av1-fullgrain-gpu 0.1` ...), `gpu`, and `fullframe_check` (`true` if you confirmed full-frame grain rendering before judging) |
| `reviewer` | yes | your pseudonym |
| `viewing` | yes | the setup in `viewing.json` |
| `date` | yes | when you judged it |

### How to submit

- A pull request adding your files under `calibration/<reviewer>/`, or
- a GitHub issue with the files attached (a zip).

Submissions are published under **CC0 1.0**: by submitting, you dedicate them to the public domain. They contain no
media: no video, frames or screenshots.

### How your verdicts are used

- **Held out first.** Your verdicts are first scored with the current model, which has never seen them: how well it
  predicts them, per reviewer and per film, says whether one viewer's calibration carries over to you.
- **Then fitted.** The next model is fitted on all reviewers, and its agreement with the verdicts is reported as in
  [docs/technical.md](docs/technical.md#how-well-grail-agrees-with-viewers): held out by film, and by reviewer.
- **A new version.** A model that moves scores gets a new version number (0.3.0, 0.4.0 ...); see the
  [version rule](docs/technical.md#version-numbers).
- **Credit.** Reviewers are credited by pseudonym, if they want.

## Code contributions

**Build and test.** Building is described in [docs/install.md](docs/install.md).

```
cargo build --release
GRAIL_CUDA_ARCHS=86 cargo build --release        # one GPU generation only: faster local builds
cargo test --release                             # unit tests
GRAIL_PORT_REFERENCE=/path/to/port_reference.json GRAIL_FULLGRAIN_LIB=DIR cargo test --release -- --ignored
```

The last line runs `grail check-reference` on the 13 cases of the Python reference and compares every stage against
it (it needs the lab's media; the maintainer runs it before merging if you can't).

**Refactors move no score.** A change to the tool that is not meant to change the model (refactoring, speed, a new
platform or backend) must leave every score as it was: on CUDA, the same scores to the last printed digit and
`check-reference` within tolerance at every stage; other backends within `check-reference`'s tolerances. A change that
moves scores is a model change: it gets a new model version and new agreement tables (see the
[version rule](docs/technical.md#version-numbers)).

**Licence.** GRAIL is GPL-3.0-only, with an additional permission for combining it with libvship
([LICENSE](LICENSE), [LICENSE-EXCEPTION.md](LICENSE-EXCEPTION.md)). Contributions are accepted under the same terms.

**Commits.** One logical change per commit. The subject names the part it touches and what changed
("decode: keep the grain-free AV1 decode grain-free under a grain-forcing libdav1d", "README: resolution and HDR
limits"); the body says why, and for anything that could touch scores, how you checked that it doesn't.
