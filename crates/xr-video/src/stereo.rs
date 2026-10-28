//! Per-eye texture regions for mono, side-by-side and over-under video.
use crate::error::{MediaError, MediaResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StereoLayout {
    Mono,
    SideBySide,
    OverUnder,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eye {
    Left,
    Right,
}
/// Pixel coordinates in the decoded frame (origin at the top-left).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EyeRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl StereoLayout {
    /// The left eye uses the left/top half. Swap `eye` in the caller for reversed sources.
    /// Packed stereo dimensions must be even so neither eye loses a column or row.
    pub fn eye_region(self, eye: Eye, width: u32, height: u32) -> MediaResult<EyeRegion> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(MediaError::Decoder(
                "Invalid video texture dimensions".into(),
            ));
        }
        let mut region = EyeRegion {
            x: 0,
            y: 0,
            width,
            height,
        };
        match self {
            Self::Mono => {}
            Self::SideBySide => {
                if !width.is_multiple_of(2) {
                    return Err(MediaError::Decoder("SBS video width must be even".into()));
                }
                region.width /= 2;
                if eye == Eye::Right {
                    region.x = region.width;
                }
            }
            Self::OverUnder => {
                if !height.is_multiple_of(2) {
                    return Err(MediaError::Decoder("OU video height must be even".into()));
                }
                region.height /= 2;
                if eye == Eye::Right {
                    region.y = region.height;
                }
            }
        }
        Ok(region)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_eyes_partition_the_entire_frame() {
        let left = StereoLayout::SideBySide
            .eye_region(Eye::Left, 3840, 1080)
            .unwrap();
        let right = StereoLayout::SideBySide
            .eye_region(Eye::Right, 3840, 1080)
            .unwrap();
        assert_eq!(
            left,
            EyeRegion {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(right.x, left.width);
        assert_eq!(right.x + right.width, 3840);
        let top = StereoLayout::OverUnder
            .eye_region(Eye::Left, 1920, 2160)
            .unwrap();
        let bottom = StereoLayout::OverUnder
            .eye_region(Eye::Right, 1920, 2160)
            .unwrap();
        assert_eq!(top, left);
        assert_eq!(bottom.y, top.height);
        assert_eq!(bottom.y + bottom.height, 2160);
    }
    #[test]
    fn mono_eyes_share_the_full_frame() {
        assert_eq!(
            StereoLayout::Mono
                .eye_region(Eye::Left, 1920, 1080)
                .unwrap(),
            StereoLayout::Mono
                .eye_region(Eye::Right, 1920, 1080)
                .unwrap()
        );
    }
    #[test]
    fn rejects_empty_overflowing_or_incomplete_eye_regions() {
        for (layout, width, height) in [
            (StereoLayout::Mono, 0, 1080),
            (StereoLayout::Mono, u32::MAX, 1080),
            (StereoLayout::SideBySide, 1919, 1080),
            (StereoLayout::OverUnder, 1920, 1079),
        ] {
            assert!(layout.eye_region(Eye::Right, width, height).is_err());
        }
    }
}
