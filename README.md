# GRAIL: quality scores that understand film grain (beta)

Most video quality metrics punish film grain. Grain is random, so a faithful encode of a grainy film never matches
the source pixel for pixel. Its score stays low however good it looks, while an encode that smears the grain away
often scores *higher*. For grainy content the high scores are out of reach, and tools that chase a target score end
up wasting bits or wiping the grain off.

**GRAIL** (Grain- and Ringing-Aware Image Likeness) **fixes that.** It adapts SSIMULACRA2 to judge grain the way you
see it: is the grain there, at the right strength and texture, and does it move like the source's? It still catches
blur, blocking and smearing. It also checks what SSIMULACRA2 is weak on, such as ringing (halos) around the lines of
2D animation. On clean footage with no grain and no line art, it gives exactly the SSIMULACRA2 score you already know.
With GRAIL you can trust your scores where SSIMULACRA2 falls short.

The score is called **GRAILS**: GRAIL built on **S**SIMULACRA2.

```
$ grail score --ref source.mkv --dist encode.mkv
GRAILS 87.49  (full mode, GRAILS model 0.2.2, grail 0.1.0-beta.2)
  SSIMULACRA2          70.42
  ...
```

Here plain SSIMULACRA2 says 70: it marks the encode down for grain that doesn't match pixel for pixel. GRAILS says
87: the picture's structure is intact, and only a small penalty remains for how the grain differs from the
source's.

## Three examples

Three scenes from the blind tests. Each image shows the same frame and spot of the source and of two encodes,
enlarged without smoothing so the grain stays visible. The scores are this tool's (full mode), for the whole
3.3-second clip the viewer judged; "viewer" is the verdict the viewer gave each encode, blind, against the source;
bitrates are the video stream's. For each metric, a green ✓ marks its score for the encode it rightly prefers (the
one the viewer passed), and a red ✗ its score for the encode it wrongly prefers.

**Film grain: The Lighthouse (2019).** Look at the skin of the forehead and cheeks. Both encodes are AV1 with the
same settings and CRF. Encode A, a plain encode, smoothed the grain away. Encode B removed the grain before encoding
and lets the player re-create it with AV1 film-grain synthesis. The viewer failed A and passed B, which is also a
quarter smaller. Plain SSIMULACRA2 prefers A, because a smooth picture is closer to the source pixel for pixel than
new random grain; GRAILS agrees with the viewer. In motion (the animation under the picture), the source's grain and
B's are new in every frame, while A's face stays waxy smooth. (This scene is one of those GRAILS was fitted on.)

![The Lighthouse (2019), 2x crops of the same frame: the source, encode A with the grain smoothed away (viewer: fail, SSIMULACRA2 81.2 marked wrong with a red cross, GRAILS 58.0), encode B denoised with AV1 film grain (viewer: pass, SSIMULACRA2 75.7, GRAILS 93.3 marked right with a green check)](docs/images/example-grain.png)

![The Lighthouse (2019), animated: the same three crops over 2 seconds (frames 6-53 of the clip, enlarged 2x). The grain of the source and of B is new in every frame; A's face stays smooth](docs/images/example-grain.avif)

*Animated AVIF; if it doesn't play, see the still above.*

| The Lighthouse (2019) | viewer | SSIMULACRA2 | GRAILS | bitrate |
|---|---|---|---|---|
| A: AV1, no grain synthesis (grain smoothed away) | fail | 81.2 | 58.0 | 1.20 Mb/s |
| B: denoised, AV1 film-grain synthesis | pass | 75.7 | 93.3 | 0.90 Mb/s |

**Heavy grain: Black Swan (2010).** Look at the skin of the face. Both encodes are AV1 at CRF 33 with film-grain
synthesis. A puts the grain back at half the strength fitted to the source (and starts from a differently denoised
picture); B uses the full fitted strength. The viewer failed A and passed B, which is also 28 % smaller. On grain
this heavy, plain SSIMULACRA2 says almost nothing: it gives the passing encode 0.1, near the bottom of its scale,
and prefers A, whose weaker grain differs less from the source pixel for pixel. GRAILS scores B 87.6 and A 58.7.
Across all five encodes the viewer judged in this scene, GRAILS puts both passes above all three fails, while
SSIMULACRA2 gives its highest score to a fail. (This scene is one of those GRAILS was fitted on.)

![Black Swan (2010), 2x crops of the same frame: the source, encode A with AV1 grain at half strength (viewer: fail, SSIMULACRA2 10.2 marked wrong with a red cross, GRAILS 58.7), encode B with AV1 grain at full strength (viewer: pass, SSIMULACRA2 0.1, GRAILS 87.6 marked right with a green check)](docs/images/example-grain-bs.png)

| Black Swan (2010) | viewer | SSIMULACRA2 | GRAILS | bitrate |
|---|---|---|---|---|
| A: AV1, synthetic grain at half strength | fail | 10.2 | 58.7 | 2.46 Mb/s |
| B: AV1, synthetic grain at full strength | pass | 0.1 | 87.6 | 1.77 Mb/s |

**Line art: Your Name (2016)** (4K HDR, tone-mapped for this picture). Both encodes are SVT-AV1 4.2.0 at the same
file size: A at preset 6, B at the much slower preset 2. Look at the tear highlight and the line under it: in A they
are surrounded by a faint halo and look smeared; B keeps them clean. The viewer failed A and passed B. SSIMULACRA2
ranks them the same way (both metrics get a ✓), but its 72.6 for A doesn't tell you anything is wrong: on the other
films, encodes with an SSIMULACRA2 of 72 to 74 passed about half the time in the blind tests (15 of 29 showings).
GRAILS's line-art check measures the halos next to the lines and takes 10.9 points off A; B has none, so its GRAILS
is its SSIMULACRA2. The crop is from the frames where GRAILS measured A's strongest ringing. In motion, A's halo
flares up every few frames, with each new drawing, while B stays steady. (This scene was not among those GRAILS was
fitted on.)

![Your Name (2016), 3x crops of the same frame: the source, encode A at SVT-AV1 preset 6 (viewer: fail, SSIMULACRA2 72.6, GRAILS 61.8), encode B at SVT-AV1 preset 2 (viewer: pass, SSIMULACRA2 77.9 and GRAILS 77.9, both marked right with green checks)](docs/images/example-lineart.png)

![Your Name (2016), animated: the same three crops over 2.3 seconds (frames 6-59, enlarged 3x). The halo around A's highlight flares up every few frames, with each new drawing; B stays steady](docs/images/example-lineart.avif)

*Animated AVIF; if it doesn't play, see the still above.*

| Your Name (2016) | viewer | SSIMULACRA2 | GRAILS | bitrate |
|---|---|---|---|---|
| A: SVT-AV1 4.2.0, preset 6 | fail | 72.6 | 61.8 | 1.50 Mb/s |
| B: SVT-AV1 4.2.0, preset 2 | pass | 77.9 | 77.9 | 1.51 Mb/s |

More examples, with another grainy film, AV1 grain seeds in standard players and clean footage:
**[docs/examples.md](docs/examples.md)**.

## How it works

Grain is random, so comparing it pixel by pixel says little: two equally good copies of a grainy film disagree almost
everywhere. GRAIL compares the picture and the grain separately.

- **Separate.** A motion-compensated denoiser, [flowdenoise](https://github.com/divergentnn/flowdenoise), splits each frame
  of the source and of the encode into the picture and its grain. It looks across neighbouring frames to tell them
  apart: picture detail stays put or moves with the scene, grain changes from frame to frame.
- **Compare the pictures** with SSIMULACRA2, so blur, blocking, smearing and ringing still count. (For an AV1 encode
  with film-grain synthesis, the encode's clean picture is simply its decode without the grain.)
- **Compare the grain** by what you notice: its strength at each brightness, its size and texture, its colour, and
  whether it moves like the source's grain or sticks in place. It is measured in flat areas, where grain shows.
- **Check line art.** On 2D animation, a separate check measures ringing that the encode adds next to the source's
  ink lines.

How much the grain comparison counts depends on how grainy the source is; on a source with no grain (or only a
little, below GRAIL's threshold) and no line art, GRAILS is plain SSIMULACRA2. AV1 film-grain synthesis makes the
same split when it encodes: it removes the grain, encodes the clean picture and stores a short description of the
grain, from which the player makes new grain. That is why GRAIL can judge synthetic grain fairly: it compares grain
with grain. Not every such encode has a clean picture underneath, though: when the encoder doesn't remove the grain
first, much of it stays coded in the picture, under the synthetic grain. GRAIL notices this and then compares that
encode the way it compares any other, through the denoiser. GRAIL renders the synthetic grain with
[av1-fullgrain](https://github.com/divergentnn/av1-fullgrain), a dav1d build that draws the grain over the whole
frame. Standard players tile one small grain patch per frame across the picture instead; with an unlucky seed that
patch has clumps or holes, and tiling repeats them all over the frame
([example](docs/examples.md#av1-grain-seeds-in-standard-players-black-swan-2010)). That is the player's doing, not
the encode's, and GRAILS, which scores the full-frame rendering, leaves it out; a separate seed view in the report
rates the seeds for standard players.

## Why use it

- **Grainy films.** We fitted GRAILS on 574 blind verdicts (pass, borderline or fail) by a viewer across 13 films.
  Then we checked it on films the model hadn't seen: take two encodes of different scenes that the viewer rated
  differently; how often does the better-rated one get the higher score? GRAILS: **68 %** of the time, plain
  SSIMULACRA2: **55 %** (50 % is a coin flip). That is what a quality target needs: the same score meaning about the
  same quality from scene to scene. The other common metrics do no better than SSIMULACRA2. This is evaluated fully
  held out by film: for each film, the whole system is refitted on the other films only. With the line-art path, on
  all 616 verdicts across 15 films: 71 % against 55 %.
- **AV1 film grain synthesis.** GRAILS renders AV1's synthetic grain the way it should look, full frame
  ([av1-fullgrain](https://github.com/divergentnn/av1-fullgrain)), and compares it fairly with the source's real grain.
  So you can tell whether your grain settings actually work.
- **2D animation.** It adds a check for ringing (halos) around line art, a common artifact of fast AV1 presets on
  anime. On grainy 2D animation the line-art check doesn't fire yet, and GRAILS can favour encodes that keep the
  source's grain; rank those with plain SSIMULACRA2.
- **Clean footage costs nothing.** With no grain and no line art, GRAILS is SSIMULACRA2, exactly.

Typical uses: comparing CRFs, presets or encoders on the same scene; checking film-grain settings; scoring scenes in a
target-quality encode (batch mode and JSON output make that easy; a batch resumes where it stopped).

## Quick start

GRAIL runs on the GPU:
- **Linux** with an **NVIDIA** GPU (RTX 20 series or newer);
- **macOS** on **Apple silicon** (tested on an M1 Max).

AMD GPUs are experimental. There are no prebuilt binaries yet; you build it with Rust's `cargo`.

You need:
- `ffmpeg`;
- libvship, the GPU SSIMULACRA2 library behind FFVship;
- for AV1 encodes with film grain only: the [av1-fullgrain](https://github.com/divergentnn/av1-fullgrain) build of
  dav1d.

Step-by-step instructions for each platform are in **[docs/install.md](docs/install.md)**. Once installed:

```
cargo build --release
target/release/grail score --ref source.mkv --dist encode.mkv
target/release/grail score --ref source.mkv --dist encode.mkv --fast      # about 2x faster
```

Score scenes rather than whole films: cut your film into scenes, or use `--start` / `--frames`. Every option, batch
scoring and JSON output are in **[docs/usage.md](docs/usage.md)**.

## Reading the score

- **Up to 100, higher is closer to the source**, on the same scale as SSIMULACRA2. Like SSIMULACRA2, it has no
  lower bound: badly damaged encodes can score below 0.
- **Compare encodes of the same scene.** That's where GRAILS is strongest: of two encodes of one scene, judged in
  the same test, that the viewer rated differently, the better-rated one gets the higher GRAILS about 4 times in 5
  (81 %; plain SSIMULACRA2 76 %, on films the model hadn't seen), except on grainy 2D animation. There is no
  universal pass mark yet, because where a viewer starts to see a problem still varies from scene to scene and film
  to film; and as a predictor of the verdict itself (how likely pass, borderline or fail is), GRAILS is not clearly
  better than plain SSIMULACRA2. Its advantage is ranking.
- **Full or fast mode?** Fast mode scores a sample of the frames and is about twice as fast, with the same accuracy
  overall. On very heavy, coarse grain, use full mode (or `--fast --denoise-candidate`).
- The output also shows plain SSIMULACRA2. When the two differ a lot, grain (or line art) is the reason.

## Speed

Frames per second scored, 88 frames of the whole picture (4K with hardware decoding of the source):

| | 1080p full | 1080p fast | 4K HDR full | 4K HDR fast |
|---|---|---|---|---|
| NVIDIA RTX 3090 | 24.7 | 41.7 | 8.6 | 16.9 |
| Apple M1 Max | 14.7 | 31.8 | 3.9 | 11.4 |

Next to plain SSIMULACRA2 with FFVship, on the same 3.7-second clips (RTX 3090, frames/s including start-up, software
decoding; `tools/bench_ffvship.py`):

| | 1080p | 4K HDR |
|---|---|---|
| FFVship (SSIMULACRA2 only) | 39.4 | 18.0 |
| GRAILS full | 24.3 | 7.8 (8.7 with NVDEC) |
| GRAILS fast | 39.8 | 13.5 (14.2 with NVDEC) |

On a scene of this length, GRAILS's full mode is about 1.6x (1080p) to 2.3x (4K) slower than FFVship end to end; fast
mode keeps up with FFVship at 1080p. FFVship's lead grows on long files, where its start-up cost is spread out (not
measured here).

Why GRAILS costs more: FFVship computes the metric alone. GRAILS first has to separate the grain from the picture,
in every frame of both videos, with a motion-compensated denoiser that searches the neighbouring frames for matching
blocks: far more work than the metric itself. And the denoiser has to be good. A weak one leaves grain in the
"clean" picture, so the grain gets punished again, or smears real detail, so real damage is missed; GRAILS's scores
depend on it. That is why GRAILS needs a GPU: the same work on a CPU would run at roughly 2 frames/s at 1080p.

Where the time goes on the RTX 3090 (1080p, full mode): decoding the source, about a third; denoising, together with
decoding the encode, 43 %; the grain measurements, 19 %; the line-art check, 2 %. SSIMULACRA2 runs alongside on
helper threads; the pipeline waits for it under 2 % of the time (its own GPU time isn't measured separately).

On a Mac that is also encoding, run grail at low priority (`taskpolicy -c utility grail ...`): the encoder keeps
most of its speed.

## Good to know

- **Beta.** GRAILS is fitted on one viewer's blind judgments (574 verdicts, 13 films). More viewers are needed
  before it can promise an absolute "transparent" threshold.
- **Grainy 2D animation is a weak spot.** On hand-drawn animation with film grain (Spirited Away, for example), the
  line-art check doesn't switch on yet, and the grain model can favour an encode that keeps the grain even when it
  rings around the lines. In the blind tests, GRAILS scored a failed x265 encode of one such scene above a passing
  SVT-AV1 one (75.3 against 67.6), where plain SSIMULACRA2 had the passing one ahead, if only just (78.89 against
  78.85). For grainy animation, check plain SSIMULACRA2 and your own eyes too.
- **Inputs.** The encode must have the source's resolution and frame rate, or be a crop of it. grail checks both
  files before scoring and rejects, with a message, what it can't score faithfully: odd frame sizes or windows,
  full-range video, formats other than 8- or 10-bit YUV, and colour other than BT.709 SDR or HDR10 (PQ) unless
  `--colour sdr|pq` says how to read it ([details](docs/technical.md#limits)).
- Scoring AV1 encodes that use film-grain synthesis needs the [av1-fullgrain](https://github.com/divergentnn/av1-fullgrain)
  decoder build. Without it, grail refuses to score them rather than score the wrong grain. `grail inspect-grain
  FILE` tells you whether an AV1 file's frames apply film grain.
- **Version numbers.** Every score is printed with its model: GRAILS model 0.2.2 today (the scores of 0.2.1; 0.2.2
  revised only the separate standard-decoder seed view). While GRAILS is below 1.0, any change that moves scores
  bumps the middle number (the next model will be 0.3.0), so scores are only comparable within one minor version. 1.0
  comes when calibration from other viewers is in and the scores are stable. The tool has its own version (grail
  0.1.0-beta.2).

## Help calibrate GRAILS

GRAILS has learned from one viewer so far. To reach 1.0 it needs more viewers, more films and more kinds of content
(grainy live action, anime, clean CGI), on more displays. You can help without sharing any video: you judge your own
scenes blind, score them with `grail score --json`, and send in the JSON reports with your verdicts. How to do it, and
how to contribute code: **[CONTRIBUTING.md](CONTRIBUTING.md)**.

## Learn more

- [docs/install.md](docs/install.md): building on Linux (NVIDIA), macOS (Apple silicon) and AMD (experimental),
  plus packaging notes.
- [docs/usage.md](docs/usage.md): every command and option, batch scoring and JSON output.
- [docs/technical.md](docs/technical.md):
  - how GRAILS is computed (the grain model and the line-art check);
  - how closely this tool matches the reference implementation;
  - detailed speed numbers;
  - the full validation tables against the viewer's verdicts and other metrics.

## Licence

GPL-3.0, with an extra permission that allows combining GRAIL with libvship, which has its own licence
([LICENSE](LICENSE), [LICENSE-EXCEPTION.md](LICENSE-EXCEPTION.md)). grail loads libvship at run time and does not
include it. Third-party components are listed in
[docs/technical.md](docs/technical.md#third-party-components-and-licences).
