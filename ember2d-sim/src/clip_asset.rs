// clip_asset.rs — an animation clip as a PROJECT asset: an ordered list of
// named regions of one tileset, a frame rate, and whether it loops.
//
// ── WHY (Step 8-3, docs/ember2d-master-plan.md §5.7) ─────────────────────────
//
// `AnimationClip` (components/animator.rs) has existed since Phase 3, but
// only ever as RUNTIME data: a script built one with `register_clip` every
// time a level started, glyph frames only, gone when the process exited
// (saves carry it, nothing else does). Nothing could author a sprite-sheet
// clip at all — `ClipFrames::Rects` was defined and never constructed.
//
// A `ClipData` is the authored, saved form, written by the editor's clip
// editor as `<project>/assets/clips/<name>.ron` (one file per clip — the
// user's own 8-3 scoping choice, mirroring `assets/tilesets/`). Its frames
// are REGION NAMES of one tileset, never pixel rects (the 8-2 decision,
// carried over): re-slicing the tileset updates every clip drawn from it.
// `to_animation_clip` turns it into the existing runtime `AnimationClip`
// (`ClipFrames::Rects`) once the tileset is known — at level load, in
// `simulation/tilesets.rs`, the same place tile sprites resolve.
//
// Pure data, like `tileset.rs`: no filesystem access in this crate.

use serde::{Deserialize, Serialize};

use crate::components::{AnimationClip, ClipFrames};
use crate::tileset::{valid_name, TilesetData};

/// Where a project keeps its clips, relative to the project root.
pub const CLIP_DIR: &str = "assets/clips";

/// The fastest a clip may play. Well past anything a tile animation needs;
/// the bound exists so a hand-edited `fps: 1e9` can't make `Animator`'s
/// catch-up loop the hot path (its own R5 runaway guard notwithstanding).
pub const MAX_FPS: f32 = 60.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipData {
    /// Also the file stem: `assets/clips/<name>.ron`.
    pub name: String,
    /// The tileset every frame is a region of.
    pub tileset: String,
    /// Region names, in playback order. A region may appear more than once
    /// (a hold on one frame is just that frame listed twice).
    pub frames: Vec<String>,
    pub fps: f32,
    #[serde(default = "yes")]
    pub looping: bool,
}

fn yes() -> bool {
    true
}

impl ClipData {
    /// What makes this clip unplayable: a bad name, no frames, a frame
    /// rate outside (0, `MAX_FPS`], or a malformed region/tileset name.
    /// Whether the regions EXIST is only knowable with the tileset in hand
    /// — `to_animation_clip` checks that.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_name(&self.name) {
            return Err(format!("clip name '{}' must be letters, digits, '_' or '-'", self.name));
        }
        if !valid_name(&self.tileset) {
            return Err(format!("clip '{}' names no valid tileset", self.name));
        }
        if self.frames.is_empty() {
            return Err(format!("clip '{}' has no frames", self.name));
        }
        if !(self.fps.is_finite() && self.fps > 0.0 && self.fps <= MAX_FPS) {
            return Err(format!("clip '{}': fps must be above 0 and at most {MAX_FPS}", self.name));
        }
        Ok(())
    }

    /// The runtime clip: each frame's region looked up in `tileset` (whose
    /// sheet image is at `image`, already resolved by the caller).
    pub fn to_animation_clip(
        &self,
        tileset: &TilesetData,
        image: &str,
    ) -> Result<AnimationClip, String> {
        self.validate()?;
        let frames = self
            .frames
            .iter()
            .map(|region| {
                tileset.region_rect(region).ok_or_else(|| {
                    format!(
                        "clip '{}': tileset '{}' has no region named '{}'",
                        self.name, self.tileset, region
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(AnimationClip {
            frames: ClipFrames::Rects { texture: image.to_string(), frames },
            fps: self.fps,
            looping: self.looping,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Rect;
    use crate::tileset::TilesetRegion;

    fn sheet() -> TilesetData {
        TilesetData {
            name: "dungeon".to_string(),
            image: "dungeon.png".to_string(),
            cell_w: 16,
            cell_h: 16,
            margin: 0,
            spacing: 0,
            columns: 4,
            rows: 1,
            regions: ["torch_1", "torch_2", "torch_3"]
                .iter()
                .enumerate()
                .map(|(i, n)| TilesetRegion {
                    name: n.to_string(),
                    col: i as u32,
                    row: 0,
                    w: 1,
                    h: 1,
                })
                .collect(),
        }
    }

    fn torch() -> ClipData {
        ClipData {
            name: "torch".to_string(),
            tileset: "dungeon".to_string(),
            frames: vec!["torch_1".into(), "torch_2".into(), "torch_3".into(), "torch_2".into()],
            fps: 8.0,
            looping: true,
        }
    }

    #[test]
    fn a_clip_becomes_a_rects_animation_in_frame_order() {
        let clip = torch().to_animation_clip(&sheet(), "p/dungeon.png").unwrap();
        assert_eq!((clip.fps, clip.looping, clip.frame_count()), (8.0, true, 4));
        match clip.frames {
            ClipFrames::Rects { texture, frames } => {
                assert_eq!(texture, "p/dungeon.png");
                assert_eq!(frames[1], Rect::new(16.0, 0.0, 16.0, 16.0));
                assert_eq!(frames[3], frames[1], "a repeated region is a repeated frame");
            }
            other => panic!("expected Rects, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_region_is_named_in_the_error() {
        let mut c = torch();
        c.frames.push("torch_9".into());
        assert!(c.to_animation_clip(&sheet(), "x.png").unwrap_err().contains("torch_9"));
    }

    #[test]
    fn validate_rejects_empty_frames_bad_fps_and_bad_names() {
        assert!(torch().validate().is_ok());
        let mut c = torch();
        c.frames.clear();
        assert!(c.validate().is_err());
        for fps in [0.0, -1.0, f32::NAN, f32::INFINITY, 61.0] {
            let mut c = torch();
            c.fps = fps;
            assert!(c.validate().is_err(), "fps {fps} must be rejected");
        }
        let mut c = torch();
        c.name = "../x".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn a_clip_round_trips_through_ron_and_defaults_to_looping() {
        let text = ron::ser::to_string(&torch()).unwrap();
        assert_eq!(ron::de::from_str::<ClipData>(&text).unwrap(), torch());
        let c: ClipData =
            ron::de::from_str(r#"(name: "a", tileset: "t", frames: ["x"], fps: 4.0)"#).unwrap();
        assert!(c.looping);
    }
}
