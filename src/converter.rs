use crate::formats::MediaType;
use crate::processor::{Quality, VideoScale, ImageScale};
use crate::util;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Global lock for LibreOffice — only one headless instance may run at a time per user profile.
static LO_LOCK: Mutex<()> = Mutex::new(());

/// Run an external command, suppress stdout/stderr, and return a nice error on failure.
fn run(cmd: &mut Command, ctx: &str) -> Result<()> {
    let output = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("Failed to launch `{}` — is it installed?", ctx))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let details = stderr
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join("\n  ");
    let details = if details.trim().is_empty() {
        "unknown error".to_string()
    } else {
        details
    };
    bail!("{ctx}:\n  {details}");
}

pub fn convert(
    input: &Path,
    output: &Path,
    media_type: MediaType,
    input_fmt: &str,
    output_fmt: &str,
    normalize_audio: bool,
    quality: Quality,
    keep_metadata: bool,
    video_scale: VideoScale,
    image_scale: ImageScale,
) -> Result<()> {
    match media_type {
        MediaType::Audio => convert_audio(input, output, output_fmt, normalize_audio, quality, keep_metadata),
        MediaType::Video => convert_video(input, output, output_fmt, quality, video_scale, keep_metadata),
        MediaType::Images => convert_image(input, output, output_fmt, quality, image_scale, keep_metadata),
        MediaType::Documents => convert_document(input, output, input_fmt, output_fmt),
    }
}

// ── Audio ──────────────────────────────────────────────────────────────

fn convert_audio(input: &Path, output: &Path, out_fmt: &str, normalize_audio: bool, quality: Quality, keep_metadata: bool) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-i"]);
    cmd.arg(input);

    match out_fmt {
        "OPUS" => { cmd.args(["-c:a", "libopus", "-b:a", "128k"]); }
        "AAC" | "M4A" => { cmd.args(["-c:a", "aac", "-b:a", "192k"]); }
        "FLAC" => { cmd.args(["-c:a", "flac"]); }
        "OGG" => { cmd.args(["-c:a", "libvorbis", "-q:a", "6"]); }
        "MP3" => { cmd.args(["-c:a", "libmp3lame", "-q:a", "2"]); }
        "WMA" => { cmd.args(["-c:a", "wmav2", "-b:a", "192k"]); }
        "WAV" | "AIFF" => {}
        _ => { cmd.args(["-q:a", "0"]); }
    }

    if normalize_audio {
        cmd.args(["-af", "loudnorm"]);
    }

    let is_inplace = input == output;
    let final_output = if is_inplace {
        output.with_extension(format!("tmp.{}", output.extension().unwrap_or_default().to_string_lossy()))
    } else {
        output.to_path_buf()
    };

    cmd.arg("-y").arg(&final_output);
    let res = run(&mut cmd, "ffmpeg");

    if is_inplace {
        if res.is_ok() {
            std::fs::rename(&final_output, output)?;
        } else {
            let _ = std::fs::remove_file(&final_output);
        }
    }

    res
}

// ── Video ──────────────────────────────────────────────────────────────

fn convert_video(input: &Path, output: &Path, out_fmt: &str, quality: Quality, scale: VideoScale, keep_metadata: bool) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-i"]);
    cmd.arg(input);

    if !keep_metadata {
        cmd.args(["-map_metadata", "-1"]);
    }

    match scale {
        VideoScale::P1080 => { cmd.args(["-vf", "scale=-2:1080"]); }
        VideoScale::P720 => { cmd.args(["-vf", "scale=-2:720"]); }
        VideoScale::Original => {}
    }

    let crf = match quality {
        Quality::High => "18",
        Quality::Medium => "23",
        Quality::Low => "28",
    };

    let threads = util::cores().to_string();

    match out_fmt {
        "WEBM" => {
            cmd.args([
                "-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0",
                "-threads", &threads,
                "-c:a", "libopus",
            ]);
        }
        "AVI" => {
            cmd.args([
                "-c:v", "mpeg4", "-q:v", "5",
                "-threads", &threads,
                "-c:a", "libmp3lame",
            ]);
        }
        "WMV" => {
            cmd.args([
                "-c:v", "wmv2", "-b:v", "2M",
                "-threads", &threads,
                "-c:a", "wmav2",
            ]);
        }
        "TS" => {
            let enc = util::best_h264_encoder();
            cmd.args(["-c:v", enc.name]);
            cmd.args(enc.quality_args);
            cmd.args(["-threads", &threads, "-c:a", "aac"]);
        }
        _ => {
            // MP4/MKV/MOV/FLV — try hardware-accelerated H.264, fall back to libx264
            let enc = util::best_h264_encoder();
            cmd.args(["-c:v", enc.name]);
            cmd.args(enc.quality_args);
            cmd.args(["-threads", &threads, "-c:a", "aac"]);
        }
    }

    cmd.arg("-y").arg(output);
    run(&mut cmd, "ffmpeg")
}

// ── Images ─────────────────────────────────────────────────────────────

fn convert_image(input: &Path, output: &Path, out_fmt: &str, quality: Quality, scale: ImageScale, keep_metadata: bool) -> Result<()> {
    let bin = util::magick_command();

    let mut cmd = Command::new(bin);
    cmd.arg(input);

    if !keep_metadata {
        cmd.arg("-strip");
    }

    match scale {
        ImageScale::W1920 => { cmd.args(["-resize", "1920x>"]); }
        ImageScale::W1080 => { cmd.args(["-resize", "1080x>"]); }
        ImageScale::Original => {}
    }

    let q_val = match quality {
        Quality::High => "92",
        Quality::Medium => "82",
        Quality::Low => "65",
    };

    match out_fmt {
        "JPEG" => { cmd.args(["-quality", "85", "-sampling-factor", "4:2:0", "-strip"]); }
        "PNG" => { cmd.args(["-strip"]); }
        "WEBP" => { cmd.args(["-quality", "80"]); }
        "AVIF" => { cmd.args(["-quality", "60"]); }
        "TIFF" => { cmd.args(["-compress", "lzw"]); }
        _ => {}
    }

    cmd.arg(output);
    run(&mut cmd, bin)
}

// ── Documents ──────────────────────────────────────────────────────────

fn convert_document(input: &Path, output: &Path, in_fmt: &str, out_fmt: &str) -> Result<()> {
    // PDF → PDF (Optimized): 150 PPI image downsampling, skip if no savings
    if in_fmt.eq_ignore_ascii_case("pdf") && out_fmt == "PDF (Optimized)" {
        return optimize_pdf(
            input,
            output,
            &[
                "-dDownsampleColorImages=true",
                "-dColorImageResolution=150",
                "-dColorImageDownsampleThreshold=1.0",
                "-dDownsampleGrayImages=true",
                "-dGrayImageResolution=150",
                "-dGrayImageDownsampleThreshold=1.0",
                "-dDownsampleMonoImages=true",
                "-dMonoImageResolution=150",
                "-dMonoImageDownsampleThreshold=1.0",
            ],
            true,
        );
    }

    // PDF → PDF: general optimization
    if in_fmt.eq_ignore_ascii_case("pdf") && out_fmt.eq_ignore_ascii_case("pdf") {
        return optimize_pdf(input, output, &["-dPDFSETTINGS=/ebook"], false);
    }

    // LibreOffice is NOT thread-safe — serialize with a mutex
    let _guard = LO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let out_dir = output.parent().unwrap_or(Path::new("."));
    let lo = util::lo_command();
    let mut cmd = Command::new(lo);
    cmd.args(["--headless", "--convert-to"]);
    cmd.arg(out_fmt.to_lowercase());
    cmd.arg("--outdir").arg(out_dir);
    cmd.arg(input);
    run(&mut cmd, lo)
}

/// Run Ghostscript PDF optimization. Writes to a temp file, then renames.
/// If `skip_if_larger` is true, files that don't shrink are left untouched.
fn optimize_pdf(input: &Path, output: &Path, settings: &[&str], skip_if_larger: bool) -> Result<()> {
    let tmp = output.with_extension("pdf.tmp");
    let input_size = input.metadata().map(|m| m.len()).unwrap_or(0);

    let gs = util::gs_command();
    let mut cmd = Command::new(gs);
    cmd.args([
        "-sDEVICE=pdfwrite",
        "-dCompatibilityLevel=1.4",
        "-dNOPAUSE",
        "-dQUIET",
        "-dBATCH",
    ]);
    cmd.args(settings);
    cmd.arg(format!("-sOutputFile={}", tmp.display()));
    cmd.arg(input);

    let result = run(&mut cmd, gs);
    if result.is_ok() {
        if skip_if_larger && input_size > 0 {
            let out_size = tmp.metadata().map(|m| m.len()).unwrap_or(0);
            if out_size >= input_size {
                let _ = std::fs::remove_file(&tmp);
                // Leave original untouched; copy only if paths differ
                if input != output {
                    std::fs::copy(input, output)?;
                }
                return Ok(());
            }
        }
        std::fs::rename(&tmp, output)?;
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
