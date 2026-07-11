use image::Rgba;
use image::RgbaImage;
use insta::assert_snapshot;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::AVATAR_HEIGHT;
use super::AVATAR_WIDTH;
use super::AnsiHalfBlockFrame;

#[test]
#[allow(clippy::disallowed_methods)]
fn renders_24px_avatar_as_twelve_truecolor_half_block_rows() {
    let mut image = RgbaImage::from_pixel(
        u32::from(AVATAR_WIDTH),
        u32::from(AVATAR_HEIGHT) * 2,
        Rgba([0, 0, 0, 0]),
    );
    for index in 0..24 {
        image.put_pixel(index, index, Rgba([255, 0, 0, 255]));
        image.put_pixel(23 - index, index, Rgba([0, 0, 255, 255]));
    }
    let frame = AnsiHalfBlockFrame::from_image(image).unwrap();
    let area = Rect::new(0, 0, AVATAR_WIDTH, AVATAR_HEIGHT);
    let mut buffer = Buffer::empty(area);

    frame.render(area, &mut buffer);

    let rendered = (0..AVATAR_HEIGHT)
        .map(|row| {
            (0..AVATAR_WIDTH)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>()
                .replace(' ', "·")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_snapshot!(rendered);
    assert_eq!(buffer[(0, 0)].fg, Color::Rgb(255, 0, 0));
    assert_eq!(buffer[(23, 0)].fg, Color::Rgb(0, 0, 255));
}

#[test]
fn transparent_pixels_keep_the_default_background() {
    let mut image = RgbaImage::from_pixel(24, 24, Rgba([0, 0, 0, 0]));
    image.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
    image.put_pixel(1, 1, Rgba([40, 50, 60, 255]));
    let frame = AnsiHalfBlockFrame::from_image(image).unwrap();
    let area = Rect::new(0, 0, AVATAR_WIDTH, AVATAR_HEIGHT);
    let mut buffer = Buffer::empty(area);

    frame.render(area, &mut buffer);

    assert_eq!(buffer[(0, 0)].symbol(), "▀");
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
    assert_eq!(buffer[(1, 0)].symbol(), "▄");
    assert_eq!(buffer[(1, 0)].bg, Color::Reset);
}

#[test]
fn alpha_threshold_is_binary_and_deterministic() {
    let mut image = RgbaImage::from_pixel(24, 24, Rgba([0, 0, 0, 0]));
    image.put_pixel(0, 0, Rgba([255, 0, 0, 127]));
    image.put_pixel(1, 0, Rgba([255, 0, 0, 128]));
    let frame = AnsiHalfBlockFrame::from_image(image).unwrap();
    let area = Rect::new(0, 0, AVATAR_WIDTH, AVATAR_HEIGHT);
    let mut buffer = Buffer::empty(area);

    frame.render(area, &mut buffer);

    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(1, 0)].symbol(), "▀");
}

#[test]
fn stale_layout_area_outside_resized_buffer_is_ignored() {
    let image = RgbaImage::from_pixel(24, 24, Rgba([255, 0, 0, 255]));
    let frame = AnsiHalfBlockFrame::from_image(image).unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 37, 20));

    frame.render(Rect::new(141, 2, AVATAR_WIDTH, AVATAR_HEIGHT), &mut buffer);

    assert!(buffer.content().iter().all(|cell| cell.symbol() == " "));
}
