//! Ambient terminal rendering for the Codex companion.
//!
//! Ambient pets reuse the same extracted image frames as the full-screen viewer
//! but are rendered through a different ownership split: ratatui still owns the
//! transcript/composer layout, while the sprite itself is emitted through the
//! terminal image protocol after the frame draw completes.
//!
//! This module therefore owns two separate contracts:
//! choosing which animation frame should be visible for the current semantic
//! pet state, and translating that frame into a precise on-screen image request
//! that does not overlap reserved bottom-pane space. It does not persist pet
//! selection or decide when modal/popover UI should suppress the sprite.

#[cfg(test)]
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use codex_config::types::TuiPetSide;

use anyhow::Context;
use anyhow::Result;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::tui::FrameRequester;

use super::DEFAULT_PET_ID;
use super::ansi_half_block::AVATAR_HEIGHT;
use super::ansi_half_block::AVATAR_WIDTH;
use super::ansi_half_block::AnsiHalfBlockFrame;
use super::frames;
use super::image_protocol::ImageProtocol;
use super::image_protocol::PetImageSupport;
#[cfg(not(test))]
use super::image_protocol::ProtocolSelection;
use super::model::Animation;
#[cfg(test)]
use super::model::AnimationFrame;
use super::model::Pet;
use super::model::PetRenderMode;

const PET_TARGET_HEIGHT_PX: u16 = 75;
const PET_COMPOSER_GAP_PX: u16 = 10;
const TERMINAL_ROW_HEIGHT_PX: u16 = 15;

const RUNNING_LIFETIME: Duration = Duration::from_secs(3 * 60);
const FAILED_LIFETIME: Duration = Duration::from_secs(60 * 60);
const WAITING_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
const REVIEW_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PetNotificationKind {
    Running,
    Waiting,
    Review,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PetContextTier {
    Fresh,
    Focused,
    Tired,
    Exhausted,
}

impl PetContextTier {
    fn from_used_percent(used_percent: i64) -> Self {
        match used_percent.clamp(0, 100) {
            0..=24 => Self::Fresh,
            25..=49 => Self::Focused,
            50..=74 => Self::Tired,
            75..=100 => Self::Exhausted,
            _ => unreachable!(),
        }
    }

    fn animation_name(self, base_animation: &str) -> Option<&'static str> {
        match (self, base_animation) {
            (Self::Fresh, "idle") => Some("fresh-idle"),
            (Self::Fresh, "running") => Some("fresh-running"),
            (Self::Focused, "idle") => Some("focused-idle"),
            (Self::Focused, "running") => Some("focused-running"),
            (Self::Tired, "idle") => Some("tired-idle"),
            (Self::Tired, "running") => Some("tired-running"),
            (Self::Exhausted, "idle") => Some("exhausted-idle"),
            (Self::Exhausted, "running") => Some("exhausted-running"),
            (_, _) => None,
        }
    }
}

impl PetNotificationKind {
    fn animation_name(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Review => "review",
            Self::Failed => "failed",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Waiting => "Needs input",
            Self::Review => "Ready",
            Self::Failed => "Blocked",
        }
    }

    fn fallback_body(self) -> &'static str {
        match self {
            Self::Running => "Thinking",
            Self::Waiting => "Needs input",
            Self::Review => "Ready",
            Self::Failed => "Blocked",
        }
    }

    fn lifetime(self) -> Duration {
        match self {
            Self::Running => RUNNING_LIFETIME,
            Self::Waiting => WAITING_LIFETIME,
            Self::Review => REVIEW_LIFETIME,
            Self::Failed => FAILED_LIFETIME,
        }
    }
}

#[derive(Debug, Clone)]
struct PetNotification {
    kind: PetNotificationKind,
    body: String,
    updated_at: Instant,
}

impl PetNotification {
    fn new(kind: PetNotificationKind, body: Option<String>) -> Self {
        Self {
            kind,
            body: body.unwrap_or_else(|| kind.fallback_body().to_string()),
            updated_at: Instant::now(),
        }
    }

    fn is_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.updated_at) >= self.kind.lifetime()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AmbientPetDraw {
    pub(crate) frame: PathBuf,
    pub(crate) protocol: ImageProtocol,
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) clear_top_y: u16,
    pub(crate) columns: u16,
    pub(crate) rows: u16,
    pub(crate) height_px: u16,
    pub(crate) sixel_dir: PathBuf,
}

#[derive(Debug)]
pub(crate) struct AmbientPet {
    pet: Pet,
    support: PetImageSupport,
    frames: Vec<PathBuf>,
    ansi_frames: Option<Vec<AnsiHalfBlockFrame>>,
    sixel_dir: PathBuf,
    frame_requester: FrameRequester,
    notification: Option<PetNotification>,
    planning: bool,
    talking: bool,
    context_tier: Option<PetContextTier>,
    preview_animation: String,
    animation_started_at: Instant,
    animations_enabled: bool,
}

impl AmbientPet {
    /// Load the active ambient pet and prepare its frame cache.
    ///
    /// This resolves the selected pet id, extracts per-frame PNGs into the
    /// CODEX_HOME cache, and records the terminal protocol support snapshot used
    /// for later draw requests. A caller that repeatedly recreates `AmbientPet`
    /// instead of mutating one instance would lose animation timing continuity
    /// and pay the frame-cache preparation cost more often than necessary.
    pub(crate) fn load(
        selected_pet: Option<&str>,
        codex_home: &std::path::Path,
        frame_requester: FrameRequester,
        animations_enabled: bool,
    ) -> Result<Self> {
        let pet = Pet::load_with_codex_home(
            selected_pet.unwrap_or(DEFAULT_PET_ID),
            /*codex_home*/ Some(codex_home),
        )
        .with_context(|| "load ambient pet")?;
        let cache_dir = codex_home
            .join("cache")
            .join("tui-pets")
            .join("frame-cache")
            .join(&pet.id)
            .join(pet.frame_cache_key()?);
        let frame_dir = cache_dir.join("frames");
        let sixel_dir = cache_dir.join("sixel");
        let frames = frames::prepare_png_frames(&pet, &frame_dir)?;
        let support = default_image_support();
        let ansi_frames = match pet.render_mode {
            PetRenderMode::AnsiHalfBlock => Some(
                frames
                    .iter()
                    .map(|path| AnsiHalfBlockFrame::load(path))
                    .collect::<Result<Vec<_>>>()?,
            ),
            PetRenderMode::TerminalImage if support.protocol().is_none() => Some(
                frames
                    .iter()
                    .map(|path| AnsiHalfBlockFrame::load_resized(path))
                    .collect::<Result<Vec<_>>>()?,
            ),
            PetRenderMode::TerminalImage => None,
        };
        Ok(Self {
            pet,
            support,
            frames,
            ansi_frames,
            sixel_dir,
            frame_requester,
            notification: None,
            planning: false,
            talking: false,
            context_tier: None,
            preview_animation: "idle".to_string(),
            animation_started_at: Instant::now(),
            animations_enabled,
        })
    }

    pub(crate) fn set_notification(&mut self, kind: PetNotificationKind, body: Option<String>) {
        self.notification = Some(PetNotification::new(kind, body));
        self.animation_started_at = Instant::now();
    }

    pub(crate) fn set_planning(&mut self, planning: bool) {
        if self.planning != planning {
            self.planning = planning;
            self.animation_started_at = Instant::now();
        }
    }

    pub(crate) fn set_talking(&mut self, talking: bool) {
        if self.talking != talking {
            self.talking = talking;
            self.animation_started_at = Instant::now();
        }
    }

    pub(crate) fn set_context_used_percent(&mut self, used_percent: Option<i64>) {
        let context_tier = used_percent.map(PetContextTier::from_used_percent);
        if self.context_tier != context_tier {
            self.context_tier = context_tier;
            self.animation_started_at = Instant::now();
        }
    }

    pub(crate) fn set_preview_animation(&mut self, animation_name: &str) {
        if self.preview_animation != animation_name {
            self.preview_animation = animation_name.to_string();
            self.animation_started_at = Instant::now();
        }
    }

    pub(crate) fn image_enabled(&self) -> bool {
        self.pet.render_mode == PetRenderMode::TerminalImage && self.support.protocol().is_some()
    }

    pub(crate) fn ansi_enabled(&self) -> bool {
        self.ansi_frames.is_some()
    }

    pub(crate) fn visual_enabled(&self) -> bool {
        self.image_enabled() || self.ansi_enabled()
    }

    pub(crate) fn unavailable_message(&self) -> Option<&'static str> {
        (!self.ansi_enabled())
            .then(|| self.support.unsupported_message())
            .flatten()
    }

    pub(crate) fn visual_columns(&self) -> u16 {
        if self.ansi_enabled() {
            AVATAR_WIDTH
        } else {
            self.image_size().columns
        }
    }

    pub(crate) fn ansi_min_height(&self) -> u16 {
        if self.ansi_enabled() {
            AVATAR_HEIGHT.saturating_add(composer_gap_rows())
        } else {
            0
        }
    }

    #[cfg(test)]
    pub(crate) fn set_image_support_for_tests(&mut self, support: PetImageSupport) {
        self.support = support;
    }

    pub(crate) fn schedule_next_frame(&self) {
        if let Some(delay) = self.next_frame_delay() {
            self.frame_requester.schedule_frame_in(delay);
        }
    }

    pub(crate) fn schedule_preview_next_frame(&self) {
        if !self.visual_enabled() || !self.animations_enabled {
            return;
        }
        if let Some(delay) = self
            .preview_animation()
            .and_then(|animation| {
                current_animation_frame(animation, self.animation_started_at.elapsed())
            })
            .and_then(|tick| tick.delay)
        {
            self.frame_requester.schedule_frame_in(delay);
        }
    }

    fn next_frame_delay(&self) -> Option<Duration> {
        if !self.visual_enabled() || !self.animations_enabled {
            return None;
        }

        current_animation_frame(
            self.current_animation()?,
            self.animation_started_at.elapsed(),
        )?
        .delay
    }

    /// Build an image draw request for the ambient pet anchored above the composer.
    ///
    /// Returning `None` means "do not render the sprite this frame", typically
    /// because the terminal protocol is unavailable or the current layout cannot
    /// fit the image without overlapping reserved UI. Callers should not try to
    /// partially clip the image themselves; that would desynchronize the image
    /// protocol output from the TUI's notion of cleared rows.
    pub(crate) fn draw_request(
        &self,
        area: Rect,
        composer_bottom_y: u16,
    ) -> Option<AmbientPetDraw> {
        if self.pet.render_mode != PetRenderMode::TerminalImage {
            return None;
        }
        let protocol = self.support.protocol()?;
        let size = self.image_size();
        let notification = self.visible_notification(Instant::now());
        let notification_height = notification.map_or(0, notification_height);
        let required_height = size.rows.saturating_add(notification_height);
        let sprite_bottom_y = composer_bottom_y.saturating_sub(composer_gap_rows());
        if sprite_bottom_y < area.y.saturating_add(required_height) || area.width < size.columns {
            return None;
        }

        let x = area.x + area.width.saturating_sub(size.columns);
        let y = sprite_bottom_y.saturating_sub(size.rows);
        Some(AmbientPetDraw {
            frame: self.current_frame_path()?,
            protocol,
            x,
            y,
            clear_top_y: area.y,
            columns: size.columns,
            rows: size.rows,
            height_px: size.height_px,
            sixel_dir: self.sixel_dir.clone(),
        })
    }

    /// Build a centered preview draw request for the `/pets` picker side pane.
    ///
    /// The picker preview intentionally uses the first idle frame rather than
    /// the live animation state so selection browsing stays stable and does not
    /// require the full ambient animation lifecycle.
    pub(crate) fn preview_draw_request(&self, area: Rect) -> Option<AmbientPetDraw> {
        if self.pet.render_mode != PetRenderMode::TerminalImage {
            return None;
        }
        let protocol = self.support.protocol()?;
        let size = self.image_size();
        if area.width < size.columns || area.height < size.rows {
            return None;
        }

        let y = area.y + area.height.saturating_sub(size.rows) / 2;
        Some(AmbientPetDraw {
            frame: self.preview_frame_path()?,
            protocol,
            x: area.x + area.width.saturating_sub(size.columns) / 2,
            y,
            clear_top_y: y,
            columns: size.columns,
            rows: size.rows,
            height_px: size.height_px,
            sixel_dir: self.sixel_dir.clone(),
        })
    }

    fn visible_notification(&self, now: Instant) -> Option<&PetNotification> {
        self.notification
            .as_ref()
            .filter(|notification| !notification.is_expired(now))
    }

    fn current_animation(&self) -> Option<&Animation> {
        let base_animation = if self.planning {
            "planning"
        } else if self.talking {
            "talking"
        } else {
            self.visible_notification(Instant::now())
                .map_or("idle", |notification| notification.kind.animation_name())
        };
        let tier_animation = (!self.planning)
            .then(|| self.context_tier?.animation_name(base_animation))
            .flatten();
        let animation = self
            .pet
            .animations
            .get(tier_animation.unwrap_or(base_animation))
            .or_else(|| self.pet.animations.get(base_animation))
            .or_else(|| {
                (base_animation == "talking")
                    .then(|| self.pet.animations.get("running"))
                    .flatten()
            })
            .or_else(|| self.pet.animations.get("idle"))?;
        if animation.loop_start.is_none() {
            let elapsed = self.animation_started_at.elapsed();
            if elapsed >= animation.total_duration()
                && let Some(fallback) = self.pet.animations.get(&animation.fallback)
            {
                return Some(fallback);
            }
        }
        Some(animation)
    }

    fn preview_animation(&self) -> Option<&Animation> {
        let base_animation = match self.preview_animation.as_str() {
            "fresh-idle" | "focused-idle" | "tired-idle" | "exhausted-idle" => "idle",
            "fresh-running" | "focused-running" | "tired-running" | "exhausted-running" => {
                "running"
            }
            animation_name => animation_name,
        };
        self.pet
            .animations
            .get(&self.preview_animation)
            .or_else(|| self.pet.animations.get(base_animation))
            .or_else(|| {
                (base_animation == "talking")
                    .then(|| self.pet.animations.get("running"))
                    .flatten()
            })
            .or_else(|| self.pet.animations.get("idle"))
    }

    fn current_frame_path(&self) -> Option<PathBuf> {
        let sprite_index = self.current_sprite_index();
        self.frame_path_for_sprite_index(sprite_index)
    }

    fn current_sprite_index(&self) -> usize {
        self.current_animation()
            .and_then(|animation| {
                if self.animations_enabled {
                    current_animation_frame(animation, self.animation_started_at.elapsed())
                        .map(|frame| frame.sprite_index)
                } else {
                    animation.frames.first().map(|frame| frame.sprite_index)
                }
            })
            .unwrap_or(0)
    }

    fn preview_sprite_index(&self) -> usize {
        self.preview_animation()
            .and_then(|animation| {
                if self.animations_enabled {
                    current_animation_frame(animation, self.animation_started_at.elapsed())
                        .map(|frame| frame.sprite_index)
                } else {
                    animation.frames.first().map(|frame| frame.sprite_index)
                }
            })
            .unwrap_or(0)
    }

    fn preview_frame_path(&self) -> Option<PathBuf> {
        self.frame_path_for_sprite_index(self.preview_sprite_index())
    }

    fn frame_path_for_sprite_index(&self, sprite_index: usize) -> Option<PathBuf> {
        self.frames
            .get(sprite_index.min(self.frames.len().saturating_sub(1)))
            .cloned()
    }

    pub(crate) fn render_ansi(
        &self,
        area: Rect,
        anchor_bottom_y: u16,
        side: TuiPetSide,
        buf: &mut Buffer,
    ) {
        let Some(frames) = self.ansi_frames.as_ref() else {
            return;
        };
        let sprite_bottom_y = anchor_bottom_y.saturating_sub(composer_gap_rows());
        if area.width < AVATAR_WIDTH || sprite_bottom_y < area.y.saturating_add(AVATAR_HEIGHT) {
            return;
        }
        let frame_index = self
            .current_sprite_index()
            .min(frames.len().saturating_sub(1));
        let Some(frame) = frames.get(frame_index) else {
            return;
        };
        frame.render(
            Rect::new(
                match side {
                    TuiPetSide::FarLeft | TuiPetSide::BelowLeft | TuiPetSide::AboveLeft => area.x,
                    TuiPetSide::BelowCenter | TuiPetSide::AboveCenter => area
                        .x
                        .saturating_add(area.width.saturating_sub(AVATAR_WIDTH) / 2),
                    TuiPetSide::FarRight | TuiPetSide::BelowRight | TuiPetSide::AboveRight => {
                        area.right().saturating_sub(AVATAR_WIDTH)
                    }
                },
                sprite_bottom_y.saturating_sub(AVATAR_HEIGHT),
                AVATAR_WIDTH,
                AVATAR_HEIGHT,
            ),
            buf,
        );
    }

    pub(crate) fn render_ansi_preview(&self, area: Rect, buf: &mut Buffer) {
        let Some(frame) = self.ansi_frames.as_ref().and_then(|frames| {
            let index = self.preview_sprite_index();
            frames.get(index.min(frames.len().saturating_sub(1)))
        }) else {
            return;
        };
        if area.width < AVATAR_WIDTH || area.height < AVATAR_HEIGHT {
            return;
        }
        frame.render(
            Rect::new(
                area.x + area.width.saturating_sub(AVATAR_WIDTH) / 2,
                area.y + area.height.saturating_sub(AVATAR_HEIGHT) / 2,
                AVATAR_WIDTH,
                AVATAR_HEIGHT,
            ),
            buf,
        );
    }

    fn image_size(&self) -> ImageSize {
        let rows = (f64::from(PET_TARGET_HEIGHT_PX) / f64::from(TERMINAL_ROW_HEIGHT_PX))
            .round()
            .max(/*other*/ 1.0) as u16;
        let aspect = f64::from(self.pet.frame_height) / f64::from(self.pet.frame_width) * 0.52;
        let columns = (f64::from(rows) / aspect).round() as u16;
        ImageSize {
            columns: columns.max(1),
            rows,
            height_px: PET_TARGET_HEIGHT_PX,
        }
    }
}

fn composer_gap_rows() -> u16 {
    ((f64::from(PET_COMPOSER_GAP_PX) / f64::from(TERMINAL_ROW_HEIGHT_PX)).round() as u16)
        .max(/*other*/ 1)
}

#[cfg(not(test))]
fn default_image_support() -> PetImageSupport {
    ProtocolSelection::Auto.resolve()
}

#[cfg(test)]
fn default_image_support() -> PetImageSupport {
    PetImageSupport::Unsupported(super::image_protocol::PetImageUnsupportedReason::Terminal)
}

#[derive(Debug, Clone, Copy)]
struct ImageSize {
    columns: u16,
    rows: u16,
    height_px: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AnimationFrameTick {
    sprite_index: usize,
    delay: Option<Duration>,
}

fn current_animation_frame(animation: &Animation, elapsed: Duration) -> Option<AnimationFrameTick> {
    if animation.frames.len() <= 1 {
        return Some(AnimationFrameTick {
            sprite_index: animation.frames.first()?.sprite_index,
            delay: None,
        });
    }

    let elapsed_nanos = elapsed.as_nanos();
    if let Some(loop_start) = animation
        .loop_start
        .filter(|idx| *idx < animation.frames.len())
    {
        let total_nanos = animation.total_duration().as_nanos();
        let prefix_nanos = animation.frames[..loop_start]
            .iter()
            .map(|frame| frame.duration.as_nanos())
            .sum::<u128>();
        let loop_nanos = animation.frames[loop_start..]
            .iter()
            .map(|frame| frame.duration.as_nanos())
            .sum::<u128>();
        let effective_elapsed = if elapsed_nanos >= total_nanos && loop_nanos > 0 {
            prefix_nanos + elapsed_nanos.saturating_sub(prefix_nanos) % loop_nanos
        } else {
            elapsed_nanos
        };
        frame_at_elapsed(animation, effective_elapsed)
    } else if elapsed_nanos >= animation.total_duration().as_nanos() {
        Some(AnimationFrameTick {
            sprite_index: animation.frames.last()?.sprite_index,
            delay: None,
        })
    } else {
        frame_at_elapsed(animation, elapsed_nanos)
    }
}

fn frame_at_elapsed(animation: &Animation, elapsed_nanos: u128) -> Option<AnimationFrameTick> {
    let mut remaining_elapsed = elapsed_nanos;
    for frame in &animation.frames {
        let frame_nanos = frame.duration.as_nanos().max(/*other*/ 1);
        if remaining_elapsed < frame_nanos {
            return Some(AnimationFrameTick {
                sprite_index: frame.sprite_index,
                delay: Some(nanos_to_duration(frame_nanos - remaining_elapsed)),
            });
        }
        remaining_elapsed = remaining_elapsed.saturating_sub(frame_nanos);
    }

    Some(AnimationFrameTick {
        sprite_index: animation.frames.last()?.sprite_index,
        delay: None,
    })
}

fn nanos_to_duration(nanos: u128) -> Duration {
    Duration::from_nanos(nanos.min(u128::from(u64::MAX)) as u64)
}

fn notification_height(notification: &PetNotification) -> u16 {
    if notification.body == notification.kind.label() {
        1
    } else {
        2
    }
}

#[cfg(test)]
pub(crate) fn test_ambient_pet(
    frame_requester: FrameRequester,
    animations_enabled: bool,
) -> AmbientPet {
    AmbientPet {
        pet: Pet {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            description: String::new(),
            spritesheet_path: PathBuf::from("spritesheet.webp"),
            frame_width: 192,
            frame_height: 208,
            columns: 8,
            rows: 9,
            frame_count: 72,
            animations: HashMap::from([("idle".to_string(), test_animation())]),
            render_mode: PetRenderMode::TerminalImage,
        },
        support: PetImageSupport::Supported(ImageProtocol::Kitty),
        frames: vec![PathBuf::from("frame-0.png"), PathBuf::from("frame-1.png")],
        ansi_frames: None,
        sixel_dir: PathBuf::new(),
        frame_requester,
        notification: None,
        planning: false,
        talking: false,
        context_tier: None,
        preview_animation: "idle".to_string(),
        animation_started_at: Instant::now()
            .checked_sub(Duration::from_millis(/*millis*/ 15))
            .unwrap(),
        animations_enabled,
    }
}

#[cfg(test)]
pub(crate) fn test_ansi_ambient_pet(
    frame_requester: FrameRequester,
    animations_enabled: bool,
) -> AmbientPet {
    let image = image::RgbaImage::from_pixel(24, 24, image::Rgba([255, 0, 0, 255]));
    AmbientPet {
        pet: Pet {
            id: "ansi-test".to_string(),
            display_name: "ANSI Test".to_string(),
            description: String::new(),
            spritesheet_path: PathBuf::from("avatar.png"),
            frame_width: 24,
            frame_height: 24,
            columns: 1,
            rows: 1,
            frame_count: 1,
            animations: HashMap::from([("idle".to_string(), test_animation())]),
            render_mode: PetRenderMode::AnsiHalfBlock,
        },
        support: PetImageSupport::Unsupported(
            super::image_protocol::PetImageUnsupportedReason::Terminal,
        ),
        frames: vec![PathBuf::from("frame-0.png")],
        ansi_frames: Some(vec![AnsiHalfBlockFrame::from_image(image).unwrap()]),
        sixel_dir: PathBuf::new(),
        frame_requester,
        notification: None,
        planning: false,
        talking: false,
        context_tier: None,
        preview_animation: "idle".to_string(),
        animation_started_at: Instant::now(),
        animations_enabled,
    }
}

#[cfg(test)]
fn test_animation() -> Animation {
    Animation {
        frames: vec![
            AnimationFrame {
                sprite_index: 0,
                duration: Duration::from_millis(/*millis*/ 10),
            },
            AnimationFrame {
                sprite_index: 1,
                duration: Duration::from_millis(/*millis*/ 10),
            },
        ],
        loop_start: Some(/*loop_start*/ 0),
        fallback: "idle".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_image_pet_uses_ansi_frames_without_image_protocol_support() {
        let codex_home = tempfile::tempdir().unwrap();
        let pet_dir = codex_home.path().join("pets").join("fallback");
        std::fs::create_dir_all(&pet_dir).unwrap();
        std::fs::write(
            pet_dir.join("pet.json"),
            r#"{
                "spritesheetPath": "sprite.png"
            }"#,
        )
        .unwrap();
        image::RgbaImage::from_pixel(
            super::super::catalog::SPRITESHEET_WIDTH,
            super::super::catalog::SPRITESHEET_HEIGHT,
            image::Rgba([255, 0, 0, 255]),
        )
        .save(pet_dir.join("sprite.png"))
        .unwrap();

        let pet = AmbientPet::load(
            Some("custom:fallback"),
            codex_home.path(),
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        )
        .unwrap();

        assert!(pet.ansi_enabled());
        assert!(!pet.image_enabled());
        assert!(pet.visual_enabled());
        assert_eq!(pet.unavailable_message(), None);
    }

    #[test]
    fn notification_labels_match_codex_app_vocabulary() {
        assert_eq!(PetNotificationKind::Running.label(), "Running");
        assert_eq!(PetNotificationKind::Waiting.label(), "Needs input");
        assert_eq!(PetNotificationKind::Review.label(), "Ready");
        assert_eq!(PetNotificationKind::Failed.label(), "Blocked");
    }

    #[test]
    fn animation_frame_uses_per_frame_duration() {
        let animation = test_animation();

        assert_eq!(
            current_animation_frame(&animation, Duration::from_millis(/*millis*/ 15)),
            Some(AnimationFrameTick {
                sprite_index: 1,
                delay: Some(Duration::from_millis(/*millis*/ 5)),
            })
        );
    }

    #[test]
    fn reduced_motion_uses_stable_first_frame_and_schedules_no_follow_up() {
        let pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );

        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-0.png")));
        assert_eq!(pet.next_frame_delay(), None);
    }

    #[test]
    fn notification_states_select_their_configured_avatar_frames() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        pet.frames = (0..5)
            .map(|index| PathBuf::from(format!("frame-{index}.png")))
            .collect();
        for (name, sprite_index) in [("running", 1), ("waiting", 2), ("review", 3), ("failed", 4)] {
            pet.pet.animations.insert(
                name.to_string(),
                Animation {
                    frames: vec![AnimationFrame {
                        sprite_index,
                        duration: Duration::from_secs(1),
                    }],
                    loop_start: Some(/*loop_start*/ 0),
                    fallback: "idle".to_string(),
                },
            );
        }

        for (kind, expected_frame) in [
            (PetNotificationKind::Running, "frame-1.png"),
            (PetNotificationKind::Waiting, "frame-2.png"),
            (PetNotificationKind::Review, "frame-3.png"),
            (PetNotificationKind::Failed, "frame-4.png"),
        ] {
            pet.set_notification(kind, /*body*/ None);
            assert_eq!(
                pet.current_frame_path(),
                Some(PathBuf::from(expected_frame))
            );
        }
    }

    #[test]
    fn planning_uses_planning_animation_and_falls_back_to_idle() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        pet.frames.push(PathBuf::from("frame-1.png"));
        pet.pet.animations.insert(
            "planning".to_string(),
            Animation {
                frames: vec![AnimationFrame {
                    sprite_index: 1,
                    duration: Duration::from_secs(1),
                }],
                loop_start: Some(/*loop_start*/ 0),
                fallback: "idle".to_string(),
            },
        );

        pet.set_planning(true);
        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-1.png")));

        pet.pet.animations.remove("planning");
        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-0.png")));
    }

    #[test]
    fn context_quartiles_select_semantic_idle_and_running_variants() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        pet.frames = (0..9)
            .map(|index| PathBuf::from(format!("frame-{index}.png")))
            .collect();
        for (sprite_index, name) in [
            (1, "fresh-idle"),
            (2, "focused-idle"),
            (3, "tired-idle"),
            (4, "exhausted-idle"),
            (5, "fresh-running"),
            (6, "focused-running"),
            (7, "tired-running"),
            (8, "exhausted-running"),
        ] {
            pet.pet.animations.insert(
                name.to_string(),
                Animation {
                    frames: vec![AnimationFrame {
                        sprite_index,
                        duration: Duration::from_secs(1),
                    }],
                    loop_start: Some(/*loop_start*/ 0),
                    fallback: "idle".to_string(),
                },
            );
        }

        for (used_percent, idle_frame, running_frame) in [
            (0, "frame-1.png", "frame-5.png"),
            (25, "frame-2.png", "frame-6.png"),
            (50, "frame-3.png", "frame-7.png"),
            (75, "frame-4.png", "frame-8.png"),
        ] {
            pet.notification = None;
            pet.set_context_used_percent(Some(used_percent));
            assert_eq!(pet.current_frame_path(), Some(PathBuf::from(idle_frame)));
            pet.set_notification(PetNotificationKind::Running, /*body*/ None);
            assert_eq!(pet.current_frame_path(), Some(PathBuf::from(running_frame)));
        }
    }

    #[test]
    fn context_variant_missing_from_manifest_falls_back_to_base_state() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        pet.frames.push(PathBuf::from("frame-1.png"));
        pet.pet.animations.insert(
            "running".to_string(),
            Animation {
                frames: vec![AnimationFrame {
                    sprite_index: 1,
                    duration: Duration::from_secs(1),
                }],
                loop_start: Some(/*loop_start*/ 0),
                fallback: "idle".to_string(),
            },
        );

        pet.set_context_used_percent(Some(50));
        pet.set_notification(PetNotificationKind::Running, /*body*/ None);

        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-1.png")));
    }

    #[test]
    fn talking_falls_back_to_running_and_planning_has_priority() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        pet.frames = (0..4)
            .map(|index| PathBuf::from(format!("frame-{index}.png")))
            .collect();
        for (name, sprite_index) in [("running", 1), ("talking", 2), ("planning", 3)] {
            pet.pet.animations.insert(
                name.to_string(),
                Animation {
                    frames: vec![AnimationFrame {
                        sprite_index,
                        duration: Duration::from_secs(1),
                    }],
                    loop_start: Some(/*loop_start*/ 0),
                    fallback: "idle".to_string(),
                },
            );
        }

        pet.set_talking(true);
        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-2.png")));
        pet.pet.animations.remove("talking");
        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-1.png")));
        pet.set_planning(true);
        assert_eq!(pet.current_frame_path(), Some(PathBuf::from("frame-3.png")));
    }

    #[test]
    fn wheel_preview_uses_selected_state_with_base_and_idle_fallbacks() {
        let mut pet = test_ambient_pet(
            FrameRequester::test_dummy(),
            /*animations_enabled*/ false,
        );
        pet.frames.push(PathBuf::from("frame-2.png"));
        pet.pet.animations.insert(
            "running".to_string(),
            Animation {
                frames: vec![AnimationFrame {
                    sprite_index: 2,
                    duration: Duration::from_secs(1),
                }],
                loop_start: Some(/*loop_start*/ 0),
                fallback: "idle".to_string(),
            },
        );

        pet.set_preview_animation("tired-running");
        assert_eq!(pet.preview_frame_path(), Some(PathBuf::from("frame-2.png")));
        pet.set_preview_animation("planning");
        assert_eq!(pet.preview_frame_path(), Some(PathBuf::from("frame-0.png")));
    }
}
