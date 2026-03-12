#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediaType {
    Audio,
    Video,
    Images,
    Documents,
}

impl MediaType {
    pub const ALL: [MediaType; 4] = [
        MediaType::Audio,
        MediaType::Video,
        MediaType::Images,
        MediaType::Documents,
    ];

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Audio => "🎵",
            Self::Video => "🎬",
            Self::Images => "🖼️ ",
            Self::Documents => "📄",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Images => "Images",
            Self::Documents => "Documents",
        }
    }

    pub fn formats(self) -> &'static [&'static str] {
        match self {
            Self::Audio => &["MP3", "WAV", "AIFF", "OGG", "FLAC", "AAC", "M4A", "WMA", "OPUS"],
            Self::Video => &["MP4", "MKV", "AVI", "MOV", "WEBM", "FLV", "WMV", "TS"],
            Self::Images => &["PNG", "JPEG", "BMP", "GIF", "WEBP", "TIFF", "AVIF", "ICO"],
            Self::Documents => &["PDF", "DOCX", "XLSX", "PPTX", "ODT", "ODS", "ODP", "EPUB"],
        }
    }

    /// Return only the output formats that make sense given the chosen input.
    /// Zero-allocation: returns a static slice.
    pub fn compatible_outputs(self, input: &str) -> &'static [&'static str] {
        match self {
            Self::Audio => match input {
                "WAV"  => &["MP3", "OGG", "FLAC", "AAC", "M4A", "OPUS"],
                "AIFF" => &["MP3", "OGG", "FLAC", "AAC", "M4A", "OPUS"],
                "FLAC" => &["MP3", "OGG", "AAC", "M4A", "OPUS"],
                "MP3"  => &["OGG", "AAC", "M4A", "OPUS"],
                "OGG"  => &["MP3", "AAC", "M4A", "OPUS"],
                "AAC"  => &["MP3", "OGG", "M4A", "OPUS"],
                "M4A"  => &["MP3", "OGG", "AAC", "OPUS"],
                "OPUS" => &["MP3", "OGG", "AAC", "M4A"],
                "WMA"  => &["MP3", "OGG", "FLAC", "AAC", "M4A", "OPUS"],
                _ => &[],
            },
            Self::Video => match input {
                "AVI"  => &["MP4", "MKV", "WEBM"],
                "MOV"  => &["MP4", "MKV", "WEBM"],
                "FLV"  => &["MP4", "MKV", "WEBM"],
                "WMV"  => &["MP4", "MKV", "WEBM"],
                "TS"   => &["MP4", "MKV"],
                "MP4"  => &["MKV", "WEBM"],
                "MKV"  => &["MP4", "WEBM"],
                "WEBM" => &["MP4", "MKV"],
                _ => &[],
            },
            Self::Images => match input {
                "BMP"  => &["PNG", "JPEG", "WEBP", "AVIF"],
                "TIFF" => &["PNG", "JPEG", "WEBP", "AVIF"],
                "PNG"  => &["JPEG", "WEBP", "AVIF"],
                "JPEG" => &["WEBP", "AVIF"],
                "GIF"  => &["WEBP", "AVIF", "PNG"],
                "WEBP" => &["JPEG", "AVIF", "PNG"],
                "AVIF" => &["JPEG", "WEBP", "PNG"],
                "ICO"  => &["PNG", "WEBP"],
                _ => &[],
            },
            Self::Documents => match input {
                "PDF"  => &["PDF", "PDF (Optimized)"],
                "DOCX" => &["PDF", "ODT"],
                "XLSX" => &["PDF", "ODS"],
                "PPTX" => &["PDF", "ODP"],
                "ODT"  => &["PDF", "DOCX"],
                "ODS"  => &["PDF", "XLSX"],
                "ODP"  => &["PDF", "PPTX"],
                "EPUB" => &["PDF"],
                _ => &[],
            },
        }
    }

    /// Whether a specific format is lossless for this media type.
    pub fn is_lossless(self, fmt: &str) -> bool {
        match self {
            Self::Audio => matches!(fmt, "WAV" | "AIFF" | "FLAC"),
            Self::Video => false, // all our video codecs are lossy
            Self::Images => matches!(fmt, "PNG" | "BMP" | "TIFF" | "ICO"),
            Self::Documents => fmt != "PDF (Optimized)", // 150 PPI downsample is lossy
        }
    }

    /// Human-friendly warning when converting from lossless → lossy.
    /// Returns `None` when no warning is needed.
    pub fn lossy_warning(self, input: &str, output: &str) -> Option<&'static str> {
        if self == Self::Documents {
            if output == "PDF (Optimized)" {
                return Some(
                    "\u{26a0}  PDF (Optimized) downsamples images to 150 PPI.\n   \
                     This is irreversible \u{2014} keep a backup if you need full resolution."
                );
            }
            return None;
        }
        let in_lossless = self.is_lossless(input);
        let out_lossless = self.is_lossless(output);

        if in_lossless && !out_lossless {
            Some(
                "⚠  You are converting from a lossless format to a lossy one.\n   \
                 This is a one-way operation — you cannot recover the original quality.\n   \
                 Lossless → lossless (e.g. WAV→FLAC) is recommended unless you need smaller files."
            )
        } else if !in_lossless && !out_lossless {
            Some(
                "⚠  Both formats are lossy — each re-encode degrades quality slightly.\n   \
                 Consider keeping a lossless master copy of your originals."
            )
        } else {
            None // lossless → lossless or lossy → lossless (rare but fine)
        }
    }

    pub fn display_item(self) -> String {
        format!("{} {}", self.icon(), self.label())
    }
}

/// Sentinel returned by `prompt::select_input_format` when the user picks "All Lossless → FLAC".
pub const LOSSLESS_AUDIO_SENTINEL: &str = "ALL_LOSSLESS_AUDIO";
/// The input formats that map to the lossless-to-FLAC batch conversion.
pub const LOSSLESS_AUDIO_INPUTS: &[&str] = &["WAV", "AIFF"];

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Optimal conversion table for Recommended Crunch mode.
/// Each entry is (MediaType, input_format, output_format).
pub fn recommended_conversions() -> &'static [(MediaType, &'static str, &'static str)] {
    &[
        // Audio: lossless → FLAC
        (MediaType::Audio, "WAV", "FLAC"),
        // Audio: lossy → OPUS
        (MediaType::Audio, "MP3", "OPUS"),
        (MediaType::Audio, "OGG", "OPUS"),
        (MediaType::Audio, "AAC", "OPUS"),
        (MediaType::Audio, "M4A", "OPUS"),
        (MediaType::Audio, "WMA", "OPUS"),
        // Video: legacy containers → MKV
        (MediaType::Video, "AVI", "MKV"),
        (MediaType::Video, "MOV", "MKV"),
        (MediaType::Video, "FLV", "MKV"),
        (MediaType::Video, "WMV", "MKV"),
        (MediaType::Video, "TS", "MKV"),
        // Images: uncompressed/legacy → PNG (lossless)
        (MediaType::Images, "BMP", "PNG"),
        (MediaType::Images, "TIFF", "PNG"),
        (MediaType::Images, "ICO", "PNG"),
        (MediaType::Images, "GIF", "PNG"),
        // Images: lossy → AVIF (most efficient)
        (MediaType::Images, "JPEG", "AVIF"),
        // Documents: PDF optimization
        (MediaType::Documents, "PDF", "PDF (Optimized)"),
    ]
}
