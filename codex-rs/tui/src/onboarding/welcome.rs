use crossterm::event::KeyEvent;
use lazy_static::lazy_static;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;
use ratatui::widgets::Wrap;
use resvg::tiny_skia::Pixmap;
use resvg::tiny_skia::Transform;
use resvg::usvg::Options;

use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::StepStateProvider;

use super::onboarding_screen::StepState;

const LOGO_RENDER_WIDTH: u32 = 256;
const LOGO_RENDER_HEIGHT: u32 = 160;
const LOGO_COLS: usize = 64;
const LOGO_ROWS: usize = 24;

lazy_static! {
    static ref GALA_LOGO: Vec<String> = render_gala_logo_ascii();
}

fn render_gala_logo_ascii() -> Vec<String> {
    const SVG: &str = include_str!("../../../docs/gala_logo.svg");
    let opts = Options::default();
    let tree = match resvg::usvg::Tree::from_str(SVG, &opts) {
        Ok(tree) => tree,
        Err(_) => return Vec::new(), // Return empty logo on parse error
    };

    let mut pixmap = match Pixmap::new(LOGO_RENDER_WIDTH, LOGO_RENDER_HEIGHT) {
        Some(pixmap) => pixmap,
        None => return Vec::new(), // Return empty logo on allocation failure
    };
    pixmap.fill(resvg::tiny_skia::Color::from_rgba8(0, 0, 0, 0));

    let svg_size = tree.size();
    let svg_width = svg_size.width();
    let svg_height = svg_size.height();
    let scale = (LOGO_RENDER_WIDTH as f32 / svg_width).min(LOGO_RENDER_HEIGHT as f32 / svg_height);
    let translate_x = (LOGO_RENDER_WIDTH as f32 - svg_width * scale) / 2.0;
    let translate_y = (LOGO_RENDER_HEIGHT as f32 - svg_height * scale) / 2.0;
    let transform = Transform::from_scale(scale, scale).post_translate(translate_x, translate_y);

    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(&tree, transform, &mut pixmap_mut);

    let alpha_grid = pixmap.data();
    let stride = (LOGO_RENDER_WIDTH as usize) * 4;

    let cell_width = LOGO_RENDER_WIDTH as f32 / LOGO_COLS as f32;
    let cell_height = LOGO_RENDER_HEIGHT as f32 / LOGO_ROWS as f32;

    let mut rows: Vec<String> = Vec::with_capacity(LOGO_ROWS);

    for row_idx in 0..LOGO_ROWS {
        let y_start = (row_idx as f32 * cell_height).floor() as usize;
        let y_end = ((row_idx + 1) as f32 * cell_height).ceil() as usize;
        let mut line = String::with_capacity(LOGO_COLS);

        for col_idx in 0..LOGO_COLS {
            let x_start = (col_idx as f32 * cell_width).floor() as usize;
            let x_end = ((col_idx + 1) as f32 * cell_width).ceil() as usize;

            let mut total_alpha = 0.0;
            let mut samples = 0;

            for y in y_start..y_end.min(LOGO_RENDER_HEIGHT as usize) {
                let row_offset = y * stride;
                for x in x_start..x_end.min(LOGO_RENDER_WIDTH as usize) {
                    let idx = row_offset + x * 4 + 3; // alpha channel
                    total_alpha += alpha_grid[idx] as f32 / 255.0;
                    samples += 1;
                }
            }

            let average = if samples == 0 {
                0.0
            } else {
                total_alpha / samples as f32
            };
            line.push(alpha_to_char(average));
        }

        rows.push(line);
    }

    trim_blank_rows(rows)
}

fn alpha_to_char(alpha: f32) -> char {
    match alpha {
        a if a >= 0.75 => '█',
        a if a >= 0.5 => '▓',
        a if a >= 0.25 => '▒',
        a if a >= 0.1 => '░',
        _ => ' ',
    }
}

fn trim_blank_rows(mut rows: Vec<String>) -> Vec<String> {
    while matches!(rows.first(), Some(line) if line.trim().is_empty()) {
        rows.remove(0);
    }
    while matches!(rows.last(), Some(line) if line.trim().is_empty()) {
        rows.pop();
    }
    rows
}

pub(crate) struct WelcomeWidget {
    pub is_logged_in: bool,
}

impl KeyboardHandler for WelcomeWidget {
    fn handle_key_event(&mut self, _key_event: KeyEvent) {}
}

impl WelcomeWidget {
    pub(crate) fn new(is_logged_in: bool) -> Self {
        Self { is_logged_in }
    }
}

impl WidgetRef for &WelcomeWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let mut lines: Vec<Line> = Vec::new();
        lines.extend(GALA_LOGO.iter().cloned().map(Line::from));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            "  ".into(),
            "Welcome to ".into(),
            "Gala Codex".bold(),
            " powered by ".into(),
            "Osmi".bold(),
        ]));

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(area, buf);
    }
}

impl StepStateProvider for WelcomeWidget {
    fn get_step_state(&self) -> StepState {
        match self.is_logged_in {
            true => StepState::Hidden,
            false => StepState::Complete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn welcome_renders_logo_and_message() {
        let widget = WelcomeWidget::new(false);
        let area = Rect::new(
            0,
            0,
            (LOGO_COLS as u16).saturating_add(4),
            (LOGO_ROWS as u16).saturating_add(6),
        );
        let mut buf = Buffer::empty(area);
        (&widget).render(area, &mut buf);

        let mut rendered = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(
            rendered.contains("Welcome to Gala Codex powered by Osmi"),
            "expected welcome message in buffer, got {rendered:?}"
        );
        assert!(
            rendered.contains('█'),
            "expected rendered logo to include filled pixels, got {rendered:?}"
        );
    }
}
