use super::history;
use super::hud::{self, HudState};
use super::text::{self, DrawContext, TextDraw, UiVertex};
use super::theme::*;
use std::collections::VecDeque;
use std::time::Instant;

pub(super) struct ConsoleLine {
    pub(super) text: String,
    pub(super) color: [f32; 3],
}

pub(super) struct ChatLine {
    pub(super) text: String,
    pub(super) received_at: Instant,
}

pub struct Console {
    pub open: bool,
    pub(super) input: String,
    pub(super) history: Vec<String>,
    pub(super) history_cursor: Option<usize>,
    pub(super) lines: VecDeque<ConsoleLine>,
    scroll_offset: usize,
    pub(super) chat_lines: VecDeque<ChatLine>,
    vertices: Vec<UiVertex>,
    render_input: String,
}

impl Console {
    pub fn new() -> Self {
        Self {
            open: false,
            input: String::with_capacity(MAX_INPUT),
            history: Vec::with_capacity(32),
            history_cursor: None,
            lines: VecDeque::with_capacity(MAX_LINES),
            scroll_offset: 0,
            chat_lines: VecDeque::with_capacity(MAX_CHAT_LINES),
            vertices: Vec::with_capacity(MAX_VERTICES),
            render_input: String::with_capacity(MAX_INPUT + 2),
        }
    }

    pub fn push_line(&mut self, text: impl AsRef<str>) {
        self.push_colored(text.as_ref(), CONSOLE_TEXT);
    }

    pub fn push_error(&mut self, text: impl AsRef<str>) {
        self.push_colored(text.as_ref(), ERROR_TEXT);
    }

    pub(super) fn push_colored(&mut self, text: &str, color: [f32; 3]) {
        self.push_cleaned(clean_line(text), color);
    }

    fn push_cleaned(&mut self, text: String, color: [f32; 3]) {
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(ConsoleLine { text, color });
        if self.scroll_offset > 0 {
            self.scroll_offset = (self.scroll_offset + 1).min(self.lines.len());
        }
    }

    pub fn push_chat_line(&mut self, text: &str) {
        let clean = clean_line(text);
        self.push_cleaned(clean.clone(), CHAT_TEXT);
        if self.chat_lines.len() == MAX_CHAT_LINES {
            self.chat_lines.pop_front();
        }
        self.chat_lines.push_back(ChatLine {
            text: clean,
            received_at: Instant::now(),
        });
    }

    pub fn append_text(&mut self, text: &str) {
        for character in text.chars() {
            let encoded_length = character.len_utf8();
            if self.input.len() + encoded_length > MAX_INPUT {
                break;
            }
            if !character.is_control() {
                self.input.push(character);
            }
        }
        self.history_cursor = None;
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.history_cursor = None;
    }

    pub fn take_input(&mut self) -> String {
        history::take_input(self)
    }

    pub fn history_up(&mut self) {
        history::history_up(self);
    }

    pub fn history_down(&mut self) {
        history::history_down(self);
    }

    pub fn complete_input(&mut self) {
        history::complete_input(self);
    }

    pub fn clear_lines(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
    }

    pub fn scroll_lines(&mut self, delta: isize) {
        self.scroll_offset = self.scroll_offset.saturating_add_signed(delta);
        self.scroll_offset = self.scroll_offset.min(self.lines.len());
    }

    pub fn scroll_to_latest(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn build_vertices(
        &mut self,
        width: u32,
        height: u32,
        health: u16,
        show_crosshair: bool,
        hit_feedback: bool,
        status: Option<&str>,
    ) -> &[UiVertex] {
        self.vertices.clear();
        if width < 120 || height < 100 {
            return &self.vertices;
        }
        let context = DrawContext::new(width as f32, height as f32);
        hud::append_hud(
            &mut self.vertices,
            context,
            HudState {
                health,
                show_crosshair,
                hit_feedback,
            },
        );
        if let Some(status) = status {
            hud::append_status(&mut self.vertices, context, status);
        }
        if !self.open {
            self.append_chat(context);
            return &self.vertices;
        }
        self.append_console(context);
        &self.vertices
    }

    fn append_chat(&mut self, context: DrawContext) {
        let now = Instant::now();
        while self
            .chat_lines
            .front()
            .is_some_and(|line| now.duration_since(line.received_at) >= CHAT_VISIBLE_DURATION)
        {
            self.chat_lines.pop_front();
        }
        let visible_lines = self.chat_lines.len().min(4);
        let first_line = self.chat_lines.len().saturating_sub(visible_lines);
        let max_chars = ((context.width - 32.0) / GLYPH_ADVANCE) as usize;
        for (row, line) in self.chat_lines.iter().skip(first_line).enumerate() {
            text::append_text(
                &mut self.vertices,
                context,
                TextDraw {
                    x: 16.0,
                    y: context.height - 82.0 - (visible_lines - row - 1) as f32 * 20.0,
                    text: &line.text,
                    color: CHAT_TEXT,
                    max_chars,
                    scale: 1.0,
                },
            );
        }
    }

    fn append_console(&mut self, context: DrawContext) {
        let panel_height = 500.0f32.min(context.height - 20.0);
        let panel_top = context.height - panel_height - 10.0;
        text::append_quad(
            &mut self.vertices,
            context,
            text::QuadDraw::solid(
                10.0,
                panel_top,
                context.width - 20.0,
                panel_height,
                [0.025, 0.035, 0.050],
            ),
        );
        text::append_quad(
            &mut self.vertices,
            context,
            text::QuadDraw::solid(
                16.0,
                context.height - 48.0,
                context.width - 32.0,
                30.0,
                [0.07, 0.10, 0.13],
            ),
        );
        let scale = 1.0;
        let line_height = 22.0;
        let max_chars = ((context.width - 44.0) / GLYPH_ADVANCE) as usize;
        let visible_lines = ((panel_height - 70.0) / line_height).max(0.0) as usize;
        let first_line = self
            .lines
            .len()
            .saturating_sub(visible_lines + self.scroll_offset);
        for (row, line) in self
            .lines
            .iter()
            .skip(first_line)
            .take(visible_lines)
            .enumerate()
        {
            text::append_text(
                &mut self.vertices,
                context,
                TextDraw {
                    x: 22.0,
                    y: panel_top + 13.0 + row as f32 * line_height,
                    text: &line.text,
                    color: line.color,
                    max_chars,
                    scale,
                },
            );
        }
        self.render_input.clear();
        self.render_input.push_str("> ");
        self.render_input.push_str(&self.input);
        text::append_text(
            &mut self.vertices,
            context,
            TextDraw {
                x: 22.0,
                y: context.height - 42.0,
                text: &self.render_input,
                color: INPUT_TEXT,
                max_chars,
                scale,
            },
        );
    }
}

fn clean_line(text: &str) -> String {
    let mut clean = String::with_capacity(text.len().min(192));
    for character in text.chars().take(192) {
        clean.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    clean
}
