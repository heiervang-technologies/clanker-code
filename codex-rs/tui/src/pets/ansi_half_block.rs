//! Pane-safe 24x24 avatar rendering using ANSI half-block cells.

use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use image::Rgba;
use image::RgbaImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

pub(super) const AVATAR_WIDTH: u16 = 24;
pub(super) const AVATAR_HEIGHT: u16 = 12;

#[derive(Debug, Clone)]
pub(super) struct AnsiHalfBlockFrame {
    text: Text<'static>,
}

impl AnsiHalfBlockFrame {
    pub(super) fn load(path: &Path) -> Result<Self> {
        let image = image::open(path)
            .with_context(|| format!("read {}", path.display()))?
            .into_rgba8();
        Self::from_image(image)
    }

    pub(super) fn from_image(image: RgbaImage) -> Result<Self> {
        if image.width() != u32::from(AVATAR_WIDTH)
            || image.height() != u32::from(AVATAR_HEIGHT) * 2
        {
            bail!(
                "ANSI half-block avatar frame must be {}x{} pixels",
                AVATAR_WIDTH,
                AVATAR_HEIGHT * 2
            );
        }

        let lines = (0..u32::from(AVATAR_HEIGHT))
            .map(|row| {
                let spans = (0..u32::from(AVATAR_WIDTH))
                    .map(|column| {
                        half_block_span(
                            *image.get_pixel(column, row * 2),
                            *image.get_pixel(column, row * 2 + 1),
                        )
                    })
                    .collect::<Vec<_>>();
                Line::from(spans)
            })
            .collect::<Vec<_>>();
        Ok(Self {
            text: Text::from(lines),
        })
    }

    pub(super) fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width < AVATAR_WIDTH || area.height < AVATAR_HEIGHT {
            return;
        }
        Paragraph::new(self.text.clone())
            .render(Rect::new(area.x, area.y, AVATAR_WIDTH, AVATAR_HEIGHT), buf);
    }
}

fn half_block_span(top: Rgba<u8>, bottom: Rgba<u8>) -> Span<'static> {
    let top = opaque_rgb(top);
    let bottom = opaque_rgb(bottom);
    match (top, bottom) {
        (Some(top), Some(bottom)) => Span::styled(
            "▀",
            Style::default().fg(rgb_color(top)).bg(rgb_color(bottom)),
        ),
        (Some(top), None) => Span::styled("▀", Style::default().fg(rgb_color(top))),
        (None, Some(bottom)) => Span::styled("▄", Style::default().fg(rgb_color(bottom))),
        (None, None) => " ".into(),
    }
}

/// Pixel-art alpha is binary at the terminal boundary: alpha 128+ is opaque.
fn opaque_rgb(pixel: Rgba<u8>) -> Option<[u8; 3]> {
    (pixel.0[3] >= 128).then_some([pixel.0[0], pixel.0[1], pixel.0[2]])
}

// Pixel art must preserve its authored palette instead of adapting to the terminal theme.
#[allow(clippy::disallowed_methods)]
fn rgb_color(rgb: [u8; 3]) -> Color {
    Color::Rgb(rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
#[path = "ansi_half_block_tests.rs"]
mod tests;
