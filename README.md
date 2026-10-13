# GRAIL: quality scores that understand film grain (beta)

Most video quality metrics punish film grain. Grain is random, so a faithful encode of a grainy film never matches
the source pixel for pixel. Its score stays low however good it looks, while an encode that smears the grain away
often scores *higher*. For grainy content the high scores are out of reach, and tools that chase a target score end
up wasting bits or wiping the grain off.

**GRAIL fixes that.** It adapts SSIMULACRA2 to judge grain the way you see it: is the grain there, at the right
strength and texture, and does it move like the source's? It still catches blur, blocking and smearing. It also checks
what SSIMULACRA2 is weak on, such as ringing (halos) around the lines of 2D animation. On clean footage with no grain
and no line art, it gives exactly the SSIMULACRA2 score you already know. With GRAIL you can trust your scores where
SSIMULACRA2 falls short.

The score is called **GRAILS**: GRAIL built on **S**SIMULACRA2. Always write the S ("GRAILS 87"), because other
flavours may follow and their numbers aren't interchangeable.

```
$ grail score --ref source.mkv --dist encode.mkv
GRAILS 87.49  (full mode, GRAIL model 2.1, grail 0.1.0-beta.1)
  SSIMULACRA2          70.42
  ...
```

Here plain SSIMULACRA2 says 70: it marks the encode down for grain that doesn't match pixel for pixel. GRAILS says
87: the picture's structure is intact, and only a small penalty remains for how the grain differs from the
source's.

## Why use it

- **Grainy films.** We fitted GRAILS on 574 blind comparisons by a viewer across 13 films. Then we checked how often
  the higher score picks the encode the viewer preferred, on films the model hadn't seen: GRAILS gets it right
  **68 %** of the time, plain SSIMULACRA2 **55 %** (50 % is a coin flip). The other common metrics do no better than
  SSIMULACRA2.
- **AV1 film grain synthesis.** GRAILS renders AV1's synthetic grain the way it should look, full frame, and compares
  it fairly with the source's real grain. So you can tell whether your grain settings actually work.
- **2D animation.** It adds a check for ringing (halos) around line art, a common artifact of fast AV1 presets on
  anime.
- **Clean footage costs nothing.** With no grain and no line art, GRAILS is SSIMULACRA2, exactly.

Typical uses: comparing CRFs, presets or encoders on the same scene; checking film-grain settings; scoring scenes in a
target-quality encode (batch mode and JSON output make that easy).

## Quick start

GRAIL runs on the GPU:
- **Linux** with an **NVIDIA** GPU (RTX 20 series or newer);
- **macOS** on **Apple silicon** (tested on an M1 Max).

AMD GPUs are experimental. There are no prebuilt binaries yet; you build it with Rust's `cargo`.

You need:
- `ffmpeg`;
- libvship, the GPU SSIMULACRA2 library behind FFVship;
- for AV1 encodes with film grain only: the av1-fullgrain build of dav1d.

Step-by-step instructions for each platform are in **[docs/install.md](docs/install.md)**. Once installed:

```
cargo build --release
target/release/grail score --ref source.mkv --dist encode.mkv
target/release/grail score --ref source.mkv --dist encode.mkv --fast      # about 2x faster
```

Score scenes rather than whole films: cut your film into scenes, or use `--start` / `--frames`. Every option, batch
scoring and JSON output are in **[docs/usage.md](docs/usage.md)**.

## Reading the score

- **0 to 100, higher is closer to the source**, on the same scale as SSIMULACRA2.
- **Compare encodes of the same scene.** That's where GRAILS is strongest: it picks the encode the viewer preferred
  about 4 times in 5. There is no universal pass mark yet, because where a viewer starts to see a problem still
  varies from scene to scene and film to film.
- **Full or fast mode?** Fast mode scores a sample of the frames and is about twice as fast, with the same accuracy
  overall. On very heavy, coarse grain, use full mode (or `--fast --denoise-candidate`).
- The output also shows plain SSIMULACRA2. When the two differ a lot, grain (or line art) is the reason.

## Speed

Frames per second scored, 88 frames of the whole picture (4K with hardware decoding of the source):

| | 1080p full | 1080p fast | 4K HDR full | 4K HDR fast |
|---|---|---|---|---|
| NVIDIA RTX 3090 | 24.7 | 41.7 | 8.6 | 16.9 |
| Apple M1 Max | 14.7 | 31.8 | 3.9 | 11.4 |

On a Mac that is also encoding, run grail at low priority (`taskpolicy -c utility grail ...`): the encoder keeps
most of its speed.

## Good to know

- **Beta.** GRAILS is fitted on one viewer's blind judgments (574 comparisons, 13 films). More viewers are needed
  before it can promise an absolute "transparent" threshold.
- The encode must have the source's resolution and frame rate, or be a crop of it.
- HDR: HDR10 (PQ) only.
- Scoring AV1 encodes that use film-grain synthesis needs the av1-fullgrain decoder build. Without it, grail refuses
  to score them rather than score the wrong grain.

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
