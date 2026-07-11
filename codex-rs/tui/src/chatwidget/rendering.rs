//! Render composition for the main chat widget surface.

use super::*;

impl ChatWidget {
    pub(super) fn as_renderable(&self) -> RenderableItem<'_> {
        let (active_cell_left_reserve, active_cell_right_reserve) =
            self.ambient_pet_horizontal_reserves();
        let active_cell_renderable = match &self.transcript.active_cell {
            Some(cell) => RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                child: cell.as_ref(),
                top: 1,
                left: active_cell_left_reserve,
                right: active_cell_right_reserve,
            })),
            None => RenderableItem::Owned(Box::new(())),
        };
        let active_hook_cell_renderable = match &self.active_hook_cell {
            Some(cell) if cell.should_render() => {
                RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                    child: cell,
                    top: 1,
                    left: active_cell_left_reserve,
                    right: active_cell_right_reserve,
                }))
            }
            _ => RenderableItem::Owned(Box::new(())),
        };
        let mut flex = FlexRenderable::new();
        flex.push(/*flex*/ 1, active_cell_renderable);
        flex.push(/*flex*/ 0, active_hook_cell_renderable);
        if let Some(cell) = self.pending_token_activity_output() {
            flex.push(
                /*flex*/ 1,
                RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                    child: cell,
                    top: 1,
                    left: active_cell_left_reserve,
                    right: active_cell_right_reserve,
                })),
            );
        }
        if let Some(cell) = self.pending_rate_limit_reset_hint() {
            flex.push(
                /*flex*/ 1,
                RenderableItem::Owned(Box::new(TranscriptAreaRenderable {
                    child: cell,
                    top: 1,
                    left: active_cell_left_reserve,
                    right: active_cell_right_reserve,
                })),
            );
        }
        flex.push(
            /*flex*/ 0,
            RenderableItem::Owned(Box::new(BottomPaneComposerReserveRenderable {
                bottom_pane: &self.bottom_pane,
                left_reserve: active_cell_left_reserve,
                right_reserve: active_cell_right_reserve,
            }))
            .inset(Insets::tlbr(
                /*top*/ 1, /*left*/ 0, /*bottom*/ 0, /*right*/ 0,
            )),
        );
        RenderableItem::Owned(Box::new(flex))
    }
}

struct BottomPaneComposerReserveRenderable<'a> {
    bottom_pane: &'a BottomPane,
    left_reserve: u16,
    right_reserve: u16,
}

impl Renderable for BottomPaneComposerReserveRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.bottom_pane.render_with_composer_right_reserve(
            self.content_area(area),
            buf,
            self.right_reserve,
        );
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.bottom_pane.desired_height_with_composer_right_reserve(
            width.saturating_sub(self.left_reserve),
            self.right_reserve,
        )
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.bottom_pane
            .cursor_pos_with_composer_right_reserve(self.content_area(area), self.right_reserve)
    }

    fn cursor_style(&self, area: Rect) -> crossterm::cursor::SetCursorStyle {
        self.bottom_pane
            .cursor_style_with_composer_right_reserve(self.content_area(area), self.right_reserve)
    }
}

impl BottomPaneComposerReserveRenderable<'_> {
    fn content_area(&self, area: Rect) -> Rect {
        Rect::new(
            area.x.saturating_add(self.left_reserve),
            area.y,
            area.width.saturating_sub(self.left_reserve),
            area.height,
        )
    }
}

struct TranscriptAreaRenderable<'a> {
    child: &'a dyn HistoryCell,
    top: u16,
    left: u16,
    right: u16,
}

impl Renderable for TranscriptAreaRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let area = self.child_area(area);
        let lines = self.child.display_lines(area.width);
        let paragraph = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
        let y = if area.height == 0 {
            0
        } else {
            let overflow = paragraph
                .line_count(area.width)
                .saturating_sub(usize::from(area.height));
            u16::try_from(overflow).unwrap_or(u16::MAX)
        };
        Clear.render(area, buf);
        paragraph.scroll((y, 0)).render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        let child_width = width
            .saturating_sub(self.left)
            .saturating_sub(self.right)
            .max(1);
        HistoryCell::desired_height(self.child, child_width) + self.top
    }
}

impl TranscriptAreaRenderable<'_> {
    fn child_area(&self, area: Rect) -> Rect {
        let y = area.y.saturating_add(self.top);
        let height = area.height.saturating_sub(self.top);
        Rect::new(
            area.x.saturating_add(self.left),
            y,
            area.width
                .saturating_sub(self.left)
                .saturating_sub(self.right)
                .max(1),
            height,
        )
    }
}

impl Renderable for ChatWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.as_renderable().render(area, buf);
        self.render_ambient_pet_ansi(area, buf);
        self.render_pet_picker_preview_ansi(buf);
        self.last_rendered_width.set(Some(area.width as usize));
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.as_renderable()
            .desired_height(width)
            .max(self.ambient_pet_min_height())
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.as_renderable().cursor_pos(area)
    }

    fn cursor_style(&self, area: Rect) -> crossterm::cursor::SetCursorStyle {
        self.as_renderable().cursor_style(area)
    }
}
