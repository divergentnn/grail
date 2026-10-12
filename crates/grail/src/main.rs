//! grail: GRAILS, the grain- and line-art-aware quality score on the SSIMULACRA2 scale (beta).
mod check;
mod decode;
mod model;
mod score;
mod seed;
mod vship;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "grail", version, about = "GRAILS: grain- and line-art-aware video quality (SSIMULACRA2 scale)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// score an encode against its source
    Score {
        /// the source (reference)
        #[arg(long = "ref")]
        reference: PathBuf,
        /// the encode (candidate), decoded as it will be watched (AV1 film grain rendered full-frame)
        #[arg(long)]
        dist: PathBuf,
        /// every 4th frame plus its successor; AV1 synthetic-grain candidates are not denoised
        #[arg(long)]
        fast: bool,
        /// with --fast: denoise AV1 synthetic-grain candidates too (subsampling only; recommended for encode targets)
        #[arg(long)]
        denoise_candidate: bool,
        /// first scored frame of the source (file frame number)
        #[arg(long, default_value_t = 0)]
        start: usize,
        /// first scored frame of the encode (default: --start)
        #[arg(long)]
        dist_start: Option<usize>,
        /// number of frames to score (default: to the end)
        #[arg(long)]
        frames: Option<usize>,
        /// scored window x,y,w,h in the source, or auto (the active picture, black bars removed)
        #[arg(long, default_value = "auto")]
        crop: String,
        /// the window in the encode's coordinates (default: the same window, or the whole encode if it is the window)
        #[arg(long)]
        dist_crop: Option<String>,
        /// sdr or pq (default: from the source's transfer characteristics)
        #[arg(long)]
        colour: Option<String>,
        /// directory with the av1-fullgrain build of libdav1d (default: $GRAIL_FULLGRAIN_LIB)
        #[arg(long)]
        fullgrain_lib: Option<PathBuf>,
        /// ffmpeg decoder threads per stream
        #[arg(long, default_value_t = 4)]
        threads: usize,
        /// JSON output with per-frame values
        #[arg(long)]
        json: bool,
        /// synchronize the GPU between stages to time them (slightly slower)
        #[arg(long)]
        timings: bool,
    },
    /// run the cases of the Python reference's port_reference.json and compare every intermediate value
    CheckReference {
        /// port_reference.json
        file: PathBuf,
        /// only cases whose name contains this
        #[arg(long)]
        only: Option<String>,
        /// directory with the av1-fullgrain build of libdav1d
        #[arg(long)]
        fullgrain_lib: Option<PathBuf>,
        #[arg(long, default_value_t = 2)]
        threads: usize,
        /// write the comparison as JSON
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn parse_rect(s: &str) -> Result<[usize; 4]> {
    let v: Vec<usize> = s.split(',').map(|x| x.trim().parse()).collect::<Result<_, _>>()?;
    if v.len() != 4 {
        bail!("expected x,y,w,h: {s}");
    }
    Ok([v[0], v[1], v[2], v[3]])
}

fn main() {
    if let Err(e) = run() {
        eprintln!("grail: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let model = model::Model::builtin();
    match cli.cmd {
        Cmd::Score { reference, dist, fast, denoise_candidate, start, dist_start, frames, crop, dist_crop, colour,
                     fullgrain_lib, threads, json, timings } => {
            let o = score::Opts {
                reference,
                dist,
                fast,
                denoise_candidate,
                start,
                dist_start,
                frames,
                crop: if crop == "auto" { score::Crop::Auto } else { score::Crop::Rect(parse_rect(&crop)?) },
                dist_crop: dist_crop.map(|s| parse_rect(&s)).transpose()?,
                colour: match colour.as_deref() {
                    None => None,
                    Some("sdr") | Some("bt709") => Some(vship::Colour::Sdr),
                    Some("pq") | Some("hdr10") => Some(vship::Colour::Pq),
                    Some(x) => bail!("--colour: sdr or pq, not {x}"),
                },
                fullgrain_lib,
                threads,
                timings,
            };
            let r = score::score(&o, &model)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                print_text(&r);
            }
        }
        Cmd::CheckReference { file, only, fullgrain_lib, threads, out } => {
            check::run(&file, only.as_deref(), fullgrain_lib, threads, out.as_deref(), &model)?;
        }
    }
    Ok(())
}

fn print_text(r: &score::Report) {
    println!("{} {:.2}  ({} mode, GRAIL model {}, grail {})", r.flavour, r.score, r.mode, r.grail_version, r.tool_version);
    println!("  SSIMULACRA2       {:8.2}", r.ssimulacra2);
    println!("  structure         {:8.2}  (path {})", r.structure, r.path);
    println!("  grain             {:8.2}", r.grain);
    println!("  line art          {:8.2}  (a {:.2}, p_L {:.3})", r.lineart.term, r.lineart.a, r.lineart.p_l);
    println!("  g {:.3}  sigma_g {:.3}  frames {}  window {:?}", r.g, r.sigma_g, r.frames_scored, r.window);
    if r.std_decoder.seed_frames > 0 {
        println!("  standard-decoder seed view (uncalibrated): mean percentile {:.1}, overlap share {:.3}",
                 r.std_decoder.seed_pct_mean.unwrap_or(f64::NAN), r.std_decoder.seed_overlap_share.unwrap_or(f64::NAN));
    }
}
