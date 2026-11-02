//! Headset-free stereo and projection detection for media library items.
//!
//! Precedence is applied independently to stereo and projection: recognized Jellyfin
//! `Video3DFormat`, filename tags, container hints, then mono/flat defaults. Jellyfin
//! has no projection field here. Unknown Jellyfin values do not mask other hints.
//! No file access, probing, decoding, or hardware capability checks are performed.

/// Encoded stereo format, including the packing resolution of each eye.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StereoFormat {
    #[default]
    Mono,
    HalfSideBySide,
    FullSideBySide,
    HalfOverUnder,
    FullOverUnder,
    Mvc,
    MvHevc,
    /// A `3D` tag alone cannot establish packing or codec.
    Unspecified3d,
}

/// Image projection, independent of stereo packing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Projection {
    #[default]
    Flat,
    Equirectangular180,
    Equirectangular360,
    Fisheye,
}

/// Container metadata already normalized by the caller's demuxer/probe adapter.
///
/// Use `None` when absent or unrecognized; `Some(Mono)`/`Some(Flat)` are explicit
/// hints. Do not infer MVC/MV-HEVC from an ordinary AVC/HEVC codec identifier, or
/// half/full packing from frame dimensions alone. This module does not parse
/// container files or require an FFmpeg dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContainerHints {
    pub stereo: Option<StereoFormat>,
    pub projection: Option<Projection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionSource {
    Jellyfin,
    Filename,
    Container,
    Default,
}

/// Detected format plus provenance for diagnostics or a future manual override UI.
/// Detection does not imply decoder or renderer support for the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedFormat {
    pub stereo: StereoFormat,
    pub projection: Projection,
    pub stereo_source: DetectionSource,
    pub projection_source: DetectionSource,
}

/// Map all Jellyfin `Video3DFormat` values; tolerate casing and surrounding space.
/// Missing, empty, and future unknown values return `None`.
pub fn jellyfin_stereo(value: &str) -> Option<StereoFormat> {
    match value.trim().to_ascii_lowercase().as_str() {
        "halfsidebyside" => Some(StereoFormat::HalfSideBySide),
        "fullsidebyside" => Some(StereoFormat::FullSideBySide),
        "fulloverunder" | "fulltopandbottom" => Some(StereoFormat::FullOverUnder),
        "halfoverunder" | "halftopandbottom" => Some(StereoFormat::HalfOverUnder),
        "mvc" => Some(StereoFormat::Mvc),
        _ => None,
    }
}

/// Detect a media item's format without opening the file.
///
/// `filename` may be a filename or a local path with either slash style (not a
/// URL). Only its basename, excluding the last extension, is inspected. Tags must
/// occupy complete alphanumeric tokens, so `1080p`, `x360`, and `SBSpecial` do not
/// match. `_LR` and `_TB` require the leading underscore specifically.
///
/// Explicit packing tags win over generic tags regardless of position. Bare
/// SBS/LR and OU/TAB/TB default to half packing; `3D` alone is unspecified stereo.
/// Conflicting tags at the same specificity use the first in filename order.
/// A `fisheye` tag wins over angular tags, since its mapping is not equirectangular.
/// These are naming heuristics, not evidence of decoder support.
pub fn detect_format(
    video_3d_format: Option<&str>,
    filename: Option<&str>,
    container: ContainerHints,
) -> DetectedFormat {
    let (file_stereo, file_projection) = filename.map(filename_hints).unwrap_or_default();
    let (stereo, stereo_source) = select(
        video_3d_format.and_then(jellyfin_stereo),
        file_stereo,
        container.stereo,
    );
    let (projection, projection_source) = select(None, file_projection, container.projection);
    DetectedFormat {
        stereo,
        projection,
        stereo_source,
        projection_source,
    }
}

fn select<T: Default>(
    jellyfin: Option<T>,
    filename: Option<T>,
    container: Option<T>,
) -> (T, DetectionSource) {
    if let Some(value) = jellyfin {
        (value, DetectionSource::Jellyfin)
    } else if let Some(value) = filename {
        (value, DetectionSource::Filename)
    } else if let Some(value) = container {
        (value, DetectionSource::Container)
    } else {
        (T::default(), DetectionSource::Default)
    }
}

fn filename_hints(filename: &str) -> (Option<StereoFormat>, Option<Projection>) {
    let basename = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let stem = basename.rsplit_once('.').map_or(basename, |(stem, _)| stem);
    let lower = stem.to_ascii_lowercase();
    let mut explicit = None;
    let mut generic = None;
    let mut is_3d = false;
    let mut projection = None;
    let mut fisheye = false;
    // Keep offsets to distinguish the suffix conventions `_LR`/`_TB` from words.
    let mut offset = 0;
    for token in lower.split(|c: char| !c.is_alphanumeric()) {
        let underscore = offset > 0 && lower.as_bytes().get(offset - 1) == Some(&b'_');
        let packing = match token {
            "hsbs" => Some(StereoFormat::HalfSideBySide),
            "fsbs" => Some(StereoFormat::FullSideBySide),
            "hou" | "htab" => Some(StereoFormat::HalfOverUnder),
            "fou" | "ftab" => Some(StereoFormat::FullOverUnder),
            "mvc" => Some(StereoFormat::Mvc),
            "mvhevc" => Some(StereoFormat::MvHevc),
            _ => None,
        };
        explicit = explicit.or(packing);
        let packing = match token {
            "sbs" => Some(StereoFormat::HalfSideBySide),
            "ou" | "tab" => Some(StereoFormat::HalfOverUnder),
            "lr" if underscore => Some(StereoFormat::HalfSideBySide),
            "tb" if underscore => Some(StereoFormat::HalfOverUnder),
            _ => None,
        };
        generic = generic.or(packing);
        is_3d |= token == "3d";
        fisheye |= token == "fisheye";
        projection = projection.or(match token {
            "180" => Some(Projection::Equirectangular180),
            "360" => Some(Projection::Equirectangular360),
            _ => None,
        });
        // The separator may be multibyte. Advance past it by its UTF-8 length.
        offset += token.len();
        if let Some(separator) = lower.get(offset..).and_then(|rest| rest.chars().next()) {
            offset += separator.len_utf8();
        }
    }
    (
        explicit
            .or(generic)
            .or(is_3d.then_some(StereoFormat::Unspecified3d)),
        if fisheye {
            Some(Projection::Fisheye)
        } else {
            projection
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use Projection::*;
    use StereoFormat::*;

    #[test]
    fn every_jellyfin_value() {
        for (value, expected) in [
            ("HalfSideBySide", HalfSideBySide),
            ("FullSideBySide", FullSideBySide),
            ("FullTopAndBottom", FullOverUnder),
            ("HalfTopAndBottom", HalfOverUnder),
            ("FullOverUnder", FullOverUnder),
            ("HalfOverUnder", HalfOverUnder),
            ("MVC", Mvc),
            (" halfSIDEbySIDE ", HalfSideBySide),
        ] {
            assert_eq!(jellyfin_stereo(value), Some(expected), "{value}");
        }
        for value in ["", "None", "FutureFormat", "HalfSideBySideExtra"] {
            assert_eq!(jellyfin_stereo(value), None, "{value}");
        }
        let detected = detect_format(None, None, ContainerHints::default());
        assert_eq!(detected.stereo, Mono);
        assert_eq!(detected.stereo_source, DetectionSource::Default);
    }

    #[test]
    fn real_world_filename_patterns() {
        for (name, stereo, projection) in [
            (
                "Avatar.2009.3D.HSBS.1080p.BluRay.x264.mkv",
                HalfSideBySide,
                Flat,
            ),
            ("Gravity (2013) [3d-fSbS].mkv", FullSideBySide, Flat),
            ("Dredd.2012.3D.SBS.1080p.mkv", HalfSideBySide, Flat),
            ("Life of Pi (2012) 3D OU.mkv", HalfOverUnder, Flat),
            ("Hugo.2011.3D.TAB.mkv", HalfOverUnder, Flat),
            ("Movie.3D.FOU.mkv", FullOverUnder, Flat),
            ("Movie.HTAB.mkv", HalfOverUnder, Flat),
            ("Movie.FTAB.mkv", FullOverUnder, Flat),
            ("Movie.HOU.mkv", HalfOverUnder, Flat),
            ("Vacation_180_LR.mp4", HalfSideBySide, Equirectangular180),
            ("Space_360_tB.mp4", HalfOverUnder, Equirectangular360),
            ("Dive.180.FishEye.3D.mp4", Unspecified3d, Fisheye),
            ("Tour.360.mp4", Mono, Equirectangular360),
            ("Movie.MVC.mkv", Mvc, Flat),
            ("Spatial.MVHEVC.mov", MvHevc, Flat),
            ("Movie.3d.mkv", Unspecified3d, Flat),
            ("Movie.1080p.x360.x264.mkv", Mono, Flat),
            ("SBSpecial.OUtlander.TABle.3Days.1800.360p.mkv", Mono, Flat),
            ("Movie.LR.TB.mkv", Mono, Flat),
            ("/media/3D/360/Normal.1080p.mkv", Mono, Flat),
            (r"C:\3D\HSBS\Normal.mkv", Mono, Flat),
            ("Movie.hsbs", Mono, Flat),
            ("旅行—_lR_180.mp4", HalfSideBySide, Equirectangular180),
            ("HSBS", HalfSideBySide, Flat),
            ("", Mono, Flat),
        ] {
            let result = detect_format(None, Some(name), ContainerHints::default());
            assert_eq!(
                (result.stereo, result.projection),
                (stereo, projection),
                "{name}"
            );
        }
    }

    #[test]
    fn priority_is_independent_for_each_dimension() {
        let hints = ContainerHints {
            stereo: Some(MvHevc),
            projection: Some(Fisheye),
        };
        let result = detect_format(Some("FullOverUnder"), Some("Movie.HSBS.180.mkv"), hints);
        assert_eq!(
            result,
            DetectedFormat {
                stereo: FullOverUnder,
                projection: Equirectangular180,
                stereo_source: DetectionSource::Jellyfin,
                projection_source: DetectionSource::Filename,
            }
        );
        let result = detect_format(Some("MVC"), Some("Movie.mkv"), hints);
        assert_eq!((result.stereo, result.projection), (Mvc, Fisheye));
        assert_eq!(result.projection_source, DetectionSource::Container);
        for jellyfin in [None, Some(""), Some("Unknown")] {
            let result = detect_format(jellyfin, Some("Movie.HSBS.360.mkv"), hints);
            assert_eq!(
                (result.stereo, result.projection),
                (HalfSideBySide, Equirectangular360)
            );
            assert_eq!(result.stereo_source, DetectionSource::Filename);
            let result = detect_format(jellyfin, Some("Movie.1080p.mkv"), hints);
            assert_eq!((result.stereo, result.projection), (MvHevc, Fisheye));
            assert_eq!(result.stereo_source, DetectionSource::Container);
        }
        let result = detect_format(None, Some("Movie.3D.mkv"), hints);
        assert_eq!(result.stereo, Unspecified3d);
        assert_eq!(result.stereo_source, DetectionSource::Filename);
    }

    #[test]
    fn absent_and_explicit_mono_hints_have_distinct_provenance() {
        let result = detect_format(None, None, ContainerHints::default());
        assert_eq!((result.stereo, result.projection), (Mono, Flat));
        assert_eq!(result.stereo_source, DetectionSource::Default);
        assert_eq!(result.projection_source, DetectionSource::Default);
        let result = detect_format(
            None,
            None,
            ContainerHints {
                stereo: Some(Mono),
                projection: Some(Flat),
            },
        );
        assert_eq!((result.stereo, result.projection), (Mono, Flat));
        assert_eq!(result.stereo_source, DetectionSource::Container);
        assert_eq!(result.projection_source, DetectionSource::Container);
    }

    #[test]
    fn specificity_and_conflicts_are_deterministic() {
        for name in ["Movie.3D.SBS.FSBS.mkv", "Movie.FSBS.SBS.3D.mkv"] {
            assert_eq!(
                detect_format(None, Some(name), ContainerHints::default()).stereo,
                FullSideBySide
            );
        }
        for (name, stereo) in [
            ("Movie.FOU.HSBS.mkv", FullOverUnder),
            ("Movie.HSBS.FOU.mkv", HalfSideBySide),
        ] {
            assert_eq!(
                detect_format(None, Some(name), ContainerHints::default()).stereo,
                stereo
            );
        }
        for name in ["Movie.180.fisheye.mp4", "Movie.fisheye.180.mp4"] {
            assert_eq!(
                detect_format(None, Some(name), ContainerHints::default()).projection,
                Fisheye
            );
        }
    }
}
