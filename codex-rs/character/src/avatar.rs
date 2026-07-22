use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::AvatarSelector;

const TERMINAL_FRAME_WIDTH: u32 = 192;
const TERMINAL_FRAME_HEIGHT: u32 = 208;
const TERMINAL_FRAME_COLUMNS: u32 = 8;
const TERMINAL_FRAME_ROWS: u32 = 9;
const TERMINAL_SHEET_WIDTH: u32 = TERMINAL_FRAME_WIDTH * TERMINAL_FRAME_COLUMNS;
const TERMINAL_SHEET_HEIGHT: u32 = TERMINAL_FRAME_HEIGHT * TERMINAL_FRAME_ROWS;
const ANSI_FRAME_WIDTH: u32 = 24;
const ANSI_FRAME_HEIGHT: u32 = 24;
const MAX_AVATAR_FRAMES: usize = 256;
const MAX_ANIMATION_FPS: f64 = 60.0;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AvatarRenderMode {
    #[default]
    TerminalImage,
    AnsiHalfBlock,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct AvatarFrameSpec {
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
}

impl AvatarFrameSpec {
    fn terminal_default() -> Self {
        Self {
            width: TERMINAL_FRAME_WIDTH,
            height: TERMINAL_FRAME_HEIGHT,
            columns: TERMINAL_FRAME_COLUMNS,
            rows: TERMINAL_FRAME_ROWS,
        }
    }

    fn ansi_default() -> Self {
        Self {
            width: ANSI_FRAME_WIDTH,
            height: ANSI_FRAME_HEIGHT,
            columns: 1,
            rows: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvatarAnimationFrame {
    pub sprite_index: usize,
    pub duration: Duration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvatarAnimation {
    pub frames: Vec<AvatarAnimationFrame>,
    pub loop_start: Option<usize>,
    pub fallback: String,
}

#[derive(Clone, Debug, Deserialize)]
struct AvatarAnimationSpec {
    #[serde(default)]
    frames: Vec<usize>,
    fps: Option<f64>,
    #[serde(rename = "loop")]
    loop_animation: Option<bool>,
    #[serde(default)]
    fallback: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedAvatarPack {
    pub manifest_path: PathBuf,
    pub spritesheet_path: PathBuf,
    pub id: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub render_mode: AvatarRenderMode,
    pub frame: AvatarFrameSpec,
    pub frame_count: usize,
    pub animations: HashMap<String, AvatarAnimation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvatarPackErrorKind {
    InvalidSelector,
    MissingManifest,
    InvalidManifest,
    MissingSpritesheet,
    InvalidSpritesheet,
    InvalidGeometry,
    InvalidAnimation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvatarPackValidationError {
    pub kind: AvatarPackErrorKind,
    pub message: String,
}

impl AvatarPackValidationError {
    fn new(kind: AvatarPackErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for AvatarPackValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AvatarPackValidationError {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AvatarManifestFile {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    spritesheet_path: Option<String>,
    #[serde(default)]
    render_mode: AvatarRenderMode,
    frame: Option<AvatarFrameSpec>,
    #[serde(default)]
    animations: HashMap<String, AvatarAnimationSpec>,
}

pub fn validate_avatar_selector(
    package_root: &Path,
    selector: &AvatarSelector,
) -> Result<ValidatedAvatarPack, AvatarPackValidationError> {
    let selector_path = Path::new(selector.as_str());
    if !is_safe_relative_path(selector_path) {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidSelector,
            "avatar selector must stay inside the character package",
        ));
    }
    if selector_path.file_name().and_then(|name| name.to_str()) != Some("avatar.json") {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidSelector,
            "avatar selector must name avatar.json",
        ));
    }

    let package_root = fs::canonicalize(package_root).map_err(|error| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidSelector,
            format!("resolve character package: {error}"),
        )
    })?;
    let requested_manifest = package_root.join(selector_path);
    let manifest_path = fs::canonicalize(&requested_manifest).map_err(|error| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::MissingManifest,
            format!(
                "resolve avatar manifest {}: {error}",
                requested_manifest.display()
            ),
        )
    })?;
    if !manifest_path.starts_with(&package_root) {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidSelector,
            "avatar selector escapes the character package",
        ));
    }
    let avatar_root = manifest_path.parent().ok_or_else(|| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidManifest,
            "avatar manifest has no containing directory",
        )
    })?;
    let raw = fs::read_to_string(&manifest_path).map_err(|error| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidManifest,
            format!("read avatar manifest {}: {error}", manifest_path.display()),
        )
    })?;
    let file = serde_json::from_str::<AvatarManifestFile>(&raw).map_err(|error| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidManifest,
            format!("parse avatar manifest {}: {error}", manifest_path.display()),
        )
    })?;
    let spritesheet_selector = file
        .spritesheet_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .unwrap_or("spritesheet.webp");
    let spritesheet_relative = Path::new(spritesheet_selector);
    if !is_safe_relative_path(spritesheet_relative) {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidSpritesheet,
            "spritesheetPath must stay inside the avatar pack",
        ));
    }
    let requested_spritesheet = avatar_root.join(spritesheet_relative);
    let spritesheet_path = fs::canonicalize(&requested_spritesheet).map_err(|error| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::MissingSpritesheet,
            format!(
                "resolve avatar spritesheet {}: {error}",
                requested_spritesheet.display()
            ),
        )
    })?;
    if !spritesheet_path.starts_with(avatar_root) || !spritesheet_path.starts_with(&package_root) {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidSpritesheet,
            "spritesheetPath escapes the avatar pack",
        ));
    }
    let (sheet_width, sheet_height) =
        image::image_dimensions(&spritesheet_path).map_err(|error| {
            AvatarPackValidationError::new(
                AvatarPackErrorKind::InvalidSpritesheet,
                format!(
                    "decode avatar spritesheet {}: {error}",
                    spritesheet_path.display()
                ),
            )
        })?;
    let frame = file.frame.unwrap_or_else(|| match file.render_mode {
        AvatarRenderMode::TerminalImage => AvatarFrameSpec::terminal_default(),
        AvatarRenderMode::AnsiHalfBlock => AvatarFrameSpec::ansi_default(),
    });
    let frame_count = validate_geometry(file.render_mode, frame, sheet_width, sheet_height)?;
    image::open(&spritesheet_path).map_err(|error| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidSpritesheet,
            format!(
                "decode avatar spritesheet {}: {error}",
                spritesheet_path.display()
            ),
        )
    })?;
    let animations = normalize_animations(file.render_mode, file.animations, frame_count)?;

    Ok(ValidatedAvatarPack {
        manifest_path,
        spritesheet_path,
        id: trimmed(file.id),
        display_name: trimmed(file.display_name),
        description: file.description.map(|value| value.trim().to_string()),
        render_mode: file.render_mode,
        frame,
        frame_count,
        animations,
    })
}

fn validate_geometry(
    render_mode: AvatarRenderMode,
    frame: AvatarFrameSpec,
    sheet_width: u32,
    sheet_height: u32,
) -> Result<usize, AvatarPackValidationError> {
    match render_mode {
        AvatarRenderMode::TerminalImage
            if sheet_width != TERMINAL_SHEET_WIDTH || sheet_height != TERMINAL_SHEET_HEIGHT =>
        {
            return Err(AvatarPackValidationError::new(
                AvatarPackErrorKind::InvalidGeometry,
                format!(
                    "terminal-image spritesheet must be {TERMINAL_SHEET_WIDTH}x{TERMINAL_SHEET_HEIGHT} pixels"
                ),
            ));
        }
        AvatarRenderMode::AnsiHalfBlock
            if frame.width != ANSI_FRAME_WIDTH || frame.height != ANSI_FRAME_HEIGHT =>
        {
            return Err(AvatarPackValidationError::new(
                AvatarPackErrorKind::InvalidGeometry,
                "ANSI half-block avatar frames must be 24x24 pixels",
            ));
        }
        AvatarRenderMode::TerminalImage | AvatarRenderMode::AnsiHalfBlock => {}
    }
    if frame.width == 0 || frame.height == 0 || frame.columns == 0 || frame.rows == 0 {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidGeometry,
            "avatar frame dimensions and grid counts must be non-zero",
        ));
    }
    let grid_width = frame.width.checked_mul(frame.columns).ok_or_else(|| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidGeometry,
            "avatar frame grid width overflow",
        )
    })?;
    let grid_height = frame.height.checked_mul(frame.rows).ok_or_else(|| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidGeometry,
            "avatar frame grid height overflow",
        )
    })?;
    if grid_width != sheet_width || grid_height != sheet_height {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidGeometry,
            format!(
                "avatar frame grid must cover spritesheet exactly: expected {sheet_width}x{sheet_height}, got {grid_width}x{grid_height}"
            ),
        ));
    }
    let frame_count = frame.columns.checked_mul(frame.rows).ok_or_else(|| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidGeometry,
            "avatar frame count overflow",
        )
    })?;
    let frame_count = usize::try_from(frame_count).map_err(|_| {
        AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidGeometry,
            "avatar frame count does not fit usize",
        )
    })?;
    if frame_count > MAX_AVATAR_FRAMES {
        return Err(AvatarPackValidationError::new(
            AvatarPackErrorKind::InvalidGeometry,
            format!("avatar frame count {frame_count} exceeds maximum {MAX_AVATAR_FRAMES}"),
        ));
    }
    Ok(frame_count)
}

fn normalize_animations(
    render_mode: AvatarRenderMode,
    specs: HashMap<String, AvatarAnimationSpec>,
    frame_count: usize,
) -> Result<HashMap<String, AvatarAnimation>, AvatarPackValidationError> {
    let mut animations = match render_mode {
        AvatarRenderMode::TerminalImage => default_animations(),
        AvatarRenderMode::AnsiHalfBlock => HashMap::from([("idle".to_string(), idle_frame_zero())]),
    };
    let mut specs = specs.into_iter().collect::<Vec<_>>();
    specs.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (name, spec) in specs {
        if spec.frames.is_empty() {
            return Err(animation_error(format!(
                "animation {name} must include at least one frame"
            )));
        }
        if let Some(index) = spec.frames.iter().find(|index| **index >= frame_count) {
            return Err(animation_error(format!(
                "animation {name} references frame {index}, but the avatar has {frame_count} frames"
            )));
        }
        let fps = match spec.fps {
            Some(fps) if fps.is_finite() && fps > 0.0 && fps <= MAX_ANIMATION_FPS => fps,
            Some(fps) => {
                return Err(animation_error(format!(
                    "animation {name} fps must be finite and between 0 and {MAX_ANIMATION_FPS}, got {fps}"
                )));
            }
            None => 8.0,
        };
        let duration = Duration::from_secs_f64(1.0 / fps);
        let fallback = if spec.fallback.is_empty() {
            "idle".to_string()
        } else {
            spec.fallback
        };
        let loop_start = spec.loop_animation.unwrap_or(true).then_some(0);
        animations.insert(
            name,
            AvatarAnimation {
                frames: spec
                    .frames
                    .into_iter()
                    .map(|sprite_index| AvatarAnimationFrame {
                        sprite_index,
                        duration,
                    })
                    .collect(),
                loop_start,
                fallback,
            },
        );
    }
    animations
        .entry("idle".to_string())
        .or_insert_with(idle_animation);
    validate_normalized_animations(&animations, frame_count)?;
    Ok(animations)
}

fn validate_normalized_animations(
    animations: &HashMap<String, AvatarAnimation>,
    frame_count: usize,
) -> Result<(), AvatarPackValidationError> {
    let mut names = animations.keys().collect::<Vec<_>>();
    names.sort_unstable();
    for name in names {
        let animation = &animations[name];
        if animation.frames.is_empty() {
            return Err(animation_error(format!(
                "animation {name} must include at least one frame"
            )));
        }
        if let Some(frame) = animation
            .frames
            .iter()
            .find(|frame| frame.sprite_index >= frame_count)
        {
            return Err(animation_error(format!(
                "animation {name} references frame {}, but the avatar has {frame_count} frames",
                frame.sprite_index
            )));
        }
        if !animations.contains_key(&animation.fallback) {
            return Err(animation_error(format!(
                "animation {name} fallback {} does not exist",
                animation.fallback
            )));
        }
    }
    Ok(())
}

fn default_animations() -> HashMap<String, AvatarAnimation> {
    [
        ("idle", idle_animation()),
        ("running-right", app_state_animation(1, 8, 120, 220)),
        ("running-left", app_state_animation(2, 8, 120, 220)),
        ("waving", app_state_animation(3, 4, 140, 280)),
        ("jumping", app_state_animation(4, 5, 140, 280)),
        ("failed", app_state_animation(5, 8, 140, 240)),
        ("waiting", app_state_animation(6, 6, 150, 260)),
        ("running", app_state_animation(7, 6, 120, 220)),
        ("review", app_state_animation(8, 6, 150, 280)),
        ("move_right", app_state_animation(1, 8, 120, 220)),
        ("move_left", app_state_animation(2, 8, 120, 220)),
        ("wave", app_state_animation(3, 4, 140, 280)),
        ("bounce", app_state_animation(4, 5, 140, 280)),
        ("sad", app_state_animation(5, 8, 140, 240)),
    ]
    .into_iter()
    .map(|(name, animation)| (name.to_string(), animation))
    .collect()
}

fn idle_animation() -> AvatarAnimation {
    AvatarAnimation {
        frames: [(0, 1680), (1, 660), (2, 660), (3, 840), (4, 840), (5, 1920)]
            .into_iter()
            .map(|(sprite_index, duration_ms)| AvatarAnimationFrame {
                sprite_index,
                duration: Duration::from_millis(duration_ms),
            })
            .collect(),
        loop_start: Some(0),
        fallback: "idle".to_string(),
    }
}

fn idle_frame_zero() -> AvatarAnimation {
    AvatarAnimation {
        frames: vec![AvatarAnimationFrame {
            sprite_index: 0,
            duration: Duration::from_secs(1),
        }],
        loop_start: Some(0),
        fallback: "idle".to_string(),
    }
}

fn app_state_animation(
    row_index: usize,
    frame_count: usize,
    frame_duration_ms: u64,
    final_frame_duration_ms: u64,
) -> AvatarAnimation {
    let primary_frames = (0..frame_count)
        .map(|column_index| AvatarAnimationFrame {
            sprite_index: row_index * TERMINAL_FRAME_COLUMNS as usize + column_index,
            duration: Duration::from_millis(if column_index == frame_count - 1 {
                final_frame_duration_ms
            } else {
                frame_duration_ms
            }),
        })
        .collect::<Vec<_>>();
    let loop_start = primary_frames.len() * 3;
    let frames = primary_frames
        .iter()
        .chain(primary_frames.iter())
        .chain(primary_frames.iter())
        .cloned()
        .chain(idle_animation().frames)
        .collect();
    AvatarAnimation {
        frames,
        loop_start: Some(loop_start),
        fallback: "idle".to_string(),
    }
}

fn animation_error(message: String) -> AvatarPackValidationError {
    AvatarPackValidationError::new(AvatarPackErrorKind::InvalidAnimation, message)
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "avatar_tests.rs"]
mod tests;
