# Using GRAIL

```
grail score --ref SOURCE --dist ENCODE [--fast [--denoise-candidate]] [--start N --frames N]
            [--crop x,y,w,h | auto] [--json] [--fullgrain-lib DIR]
```

- `--start N --frames N`: score frames `N ..` of the source (file frame numbers; decoding starts at the beginning of
  the file, so cut long films into scenes first). `--dist-start` if the encode's frame numbers differ.
- `--crop x,y,w,h`: score this window (even coordinates, in source pixels). Default `auto`: the active picture (black
  bars removed). The denoiser always sees the whole active picture; the window is cut out afterwards. If the encode
  is the window itself (a crop encode), it is detected by its size; `--dist-crop` overrides.
- `--fast`: every 4th frame plus its successor; AV1 synthetic-grain encodes are not denoised. 1.7-2.0x faster
  (see [technical.md](technical.md#speed)), same agreement with the verdicts, but up to 20 points off on heavy, coarse grain.
  `--fast --denoise-candidate` keeps the denoise (frame subsampling only): use it, or the full mode, as an encode
  target on heavy grain.
- `--json`: everything, including per-frame SSIMULACRA2, structure and line-art values and the stage timings.
- `--colour sdr|pq`: overrides the colour handling read from the source (PQ: BT.2020 + SMPTE 2084).
- `--hwdec auto|none|cuda|videotoolbox`: NVDEC (Linux) or VideoToolbox (macOS) for non-AV1 streams (auto: above
  2048 px width). AV1 stays on dav1d.
- `--threads N`: ffmpeg threads per decoder (up to three decoders run at once).
- `--siting-left`: plain SSIMULACRA2 with chroma siting "left" on both sides instead of each file's own siting (see [technical.md](technical.md#exactness-against-the-python-reference)).

Many pairs (one GPU initialization; resumable with `--out`):

```
$ cat pairs.jsonl
{"id": "x265-crf18", "ref": "scene.mkv", "dist": "x265_crf18.mkv", "frames": 96}
{"id": "svt-crf30", "ref": "scene.mkv", "dist": "svt_crf30.mkv", "crop": [960, 540, 1920, 1080], "fast": true}
$ grail score --batch pairs.jsonl --out scores.jsonl
```

Each line takes `ref`, `dist` and optionally `id`, `start`, `dist_start`, `frames`, `crop`, `dist_crop`, `colour`,
`fast`, `denoise_candidate`; the command-line options are the defaults. Each output line holds the id, a label
("GRAILS 87.49", or "GRAILS 87.49 (fast)"), the score and its parts (the full report with `--json`) and a
`fingerprint`; a failed pair gets an `error` field and the batch goes on.

- Resuming (`--out` again): a pair is skipped only if the file already holds a successful row with the same id and
  the same fingerprint, a hash of the model version, the options that change a score, both media files' size,
  modification time and first MiB (and the av1-fullgrain build, when used). A changed file or option scores the pair
  again; the new row is appended, so take the last row of an id.
- With `--json` the output holds no file names: failed rows carry only the id and the error, with the media paths in
  the message replaced by `<ref>` / `<dist>`. Without `--json`, rows list `ref` and `dist`.

Other commands:

- `grail denoise IN.y4m OUT.y4m`: the measurement denoiser ([flowdenoise](https://github.com/divergentnn/flowdenoise))
  alone (10-bit 4:2:0 y4m; `-` for stdin / stdout).
- `grail check-reference port_reference.json`: runs the reference's test cases and compares every intermediate value
  (see [technical.md](technical.md#exactness-against-the-python-reference); needs the lab's media). It exits non-zero
  unless every case ran and every check passed ([backend acceptance](technical.md#backend-acceptance)). With
  `--fast-variants` it checks the fast mode against the reference's fast-mode inputs; `--skip-missing` lists (instead
  of failing) cases that have no fast-mode reference row.
- `grail inspect-grain FILE...`: the AV1 film grain of streams from their frame headers (whether the sequence allows
  grain, whether any frame applies it, shown frames), one JSON line per file; exits non-zero if a file cannot be read.

Environment: `GRAIL_VSHIP`, `GRAIL_FULLGRAIN_LIB`, `GRAIL_BACKEND`, `GRAIL_GPU` (device index), `GRAIL_METAL_PROFILE`
(macOS), `GRAIL_MEM_MB`
(host memory for decoded source frames, default 4096; above it the source is decoded twice).

Back to the [README](../README.md).
