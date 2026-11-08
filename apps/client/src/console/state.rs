use super::editor::Editor;
use super::history;
use super::hud::{self, HudState};
use super::input_draw;
use super::suggestions::Suggestions;
use super::text::{self, DrawContext, TextDraw, UiVertex};
use super::theme::*;
use super::zoom;
use std::collections::VecDeque;
use std::time::Instant;

pub(super) struct ConsoleLine {
    pub(super) text: String,
    pub(super) color: [f32; 3],
    localized: Option<LocalizedLine>,
}

enum LocalizedLine {
    Ui(minimium_localization::UiMessage),
    Client(crate::messages::ClientText),
}

pub(super) struct ChatLine {
    pub(super) text: String,
    pub(super) received_at: Instant,
}

pub struct Overlay<'a> {
    pub status: Option<&'a str>,
    pub overview_zoom: Option<f32>,
    pub celestial_labels: &'a [CelestialLabel],
}

pub struct CelestialLabel {
    pub x: f32,
    pub y: f32,
    pub text: String,
}

pub struct Console {
    pub open: bool,
    pub(crate) editor: Editor,
    pub(super) suggestions: Suggestions,
    pub(super) history: Vec<String>,
    pub(super) history_cursor: Option<usize>,
    pub(super) history_draft: String,
    pub(super) lines: VecDeque<ConsoleLine>,
    scroll_offset: usize,
    pub(super) chat_lines: VecDeque<ChatLine>,
    vertices: Vec<UiVertex>,
    dpi_scale: f32,
}

impl Console {
    pub fn new() -> Self {
        Self {
            open: false,
            editor: Editor::new(),
            suggestions: Suggestions::new(),
            history: Vec::with_capacity(32),
            history_cursor: None,
            history_draft: String::new(),
            lines: VecDeque::with_capacity(MAX_LINES),
            scroll_offset: 0,
            chat_lines: VecDeque::with_capacity(MAX_CHAT_LINES),
            vertices: Vec::with_capacity(MAX_VERTICES),
            dpi_scale: 1.0,
        }
    }

    pub fn push_line(&mut self, text: impl AsRef<str>) {
        for line in text.as_ref().lines() {
            self.push_colored(line, CONSOLE_TEXT);
        }
    }

    pub fn push_ui_message(
        &mut self,
        message: minimium_localization::UiMessage,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        let text = localizer.format(locale, &message);
        self.push_localized(text, CONSOLE_TEXT, LocalizedLine::Ui(message));
    }

    pub fn push_ui_error_message(
        &mut self,
        message: minimium_localization::UiMessage,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        let text = localizer.format(locale, &message);
        self.push_localized(text, ERROR_TEXT, LocalizedLine::Ui(message));
    }

    pub fn push_client_text(
        &mut self,
        message: crate::messages::ClientText,
        error: bool,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        let text = message.localized(localizer, locale);
        self.push_localized(
            text,
            if error { ERROR_TEXT } else { CONSOLE_TEXT },
            LocalizedLine::Client(message),
        );
    }

    pub fn relocalize(
        &mut self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        for line in &mut self.lines {
            let Some(message) = &line.localized else {
                continue;
            };
            let text = match message {
                LocalizedLine::Ui(message) => localizer.format(locale, message),
                LocalizedLine::Client(message) => message.localized(localizer, locale),
            };
            line.text = clean_line(&text);
        }
    }

    pub(super) fn push_colored(&mut self, text: &str, color: [f32; 3]) {
        self.push_cleaned(clean_line(text), color, None);
    }

    fn push_localized(&mut self, text: String, color: [f32; 3], localized: LocalizedLine) {
        self.push_cleaned(clean_line(&text), color, Some(localized));
    }

    fn push_cleaned(&mut self, text: String, color: [f32; 3], localized: Option<LocalizedLine>) {
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(ConsoleLine {
            text,
            color,
            localized,
        });
        if self.scroll_offset > 0 {
            self.scroll_offset = (self.scroll_offset + 1).min(self.lines.len());
        }
    }

    pub fn push_chat_line(&mut self, text: &str) {
        let clean = clean_line(text);
        self.push_cleaned(clean.clone(), CHAT_TEXT, None);
        if self.chat_lines.len() == MAX_CHAT_LINES {
            self.chat_lines.pop_front();
        }
        self.chat_lines.push_back(ChatLine {
            text: clean,
            received_at: Instant::now(),
        });
    }

    pub fn append_text(
        &mut self,
        text: &str,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        self.editor.replace_selection(text);
        self.history_cursor = None;
        self.suggestions.edited(&self.editor, localizer, locale);
    }

    pub fn set_preedit(&mut self, text: &str) {
        self.editor.set_preedit(text);
    }

    pub fn clear_preedit(&mut self) {
        self.editor.clear_preedit();
    }

    pub fn commit_preedit(
        &mut self,
        text: &str,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        self.editor.clear_preedit();
        self.append_text(text, localizer, locale);
    }

    pub fn backspace(
        &mut self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        self.editor.delete(true, false);
        self.edit_changed(localizer, locale);
    }

    pub fn take_input(
        &mut self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) -> String {
        history::take_input(self, localizer, locale)
    }

    pub fn history_up(
        &mut self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        history::history_up(self, localizer, locale);
    }

    pub fn history_down(
        &mut self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        history::history_down(self, localizer, locale);
    }

    pub fn complete_input(
        &mut self,
        reverse: bool,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        self.suggestions
            .cycle(&mut self.editor, reverse, localizer, locale);
    }

    pub fn edit_changed(
        &mut self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        self.history_cursor = None;
        self.suggestions.edited(&self.editor, localizer, locale);
    }

    pub fn refresh_suggestions(
        &mut self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) {
        self.suggestions.edited(&self.editor, localizer, locale);
    }

    pub fn input_hit(
        &mut self,
        width: u32,
        height: u32,
        x: f64,
        y: f64,
        select: bool,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) -> bool {
        let scale = f64::from(self.dpi_scale);
        let hit = input_draw::hit(
            &mut self.editor,
            (f64::from(width) / scale).round() as u32,
            (f64::from(height) / scale).round() as u32,
            x / scale,
            y / scale,
            select,
        );
        if hit {
            self.refresh_suggestions(localizer, locale);
        }
        hit
    }

    pub fn set_dpi_scale(&mut self, scale: f64) {
        self.dpi_scale = if scale.is_finite() { scale as f32 } else { 1.0 }.clamp(0.5, 4.0);
    }

    pub fn clear_lines(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
    }

    pub(crate) fn diagnostic_lines_since(&self, index: usize) -> Vec<String> {
        self.lines
            .iter()
            .skip(index)
            .map(|line| line.text.clone())
            .collect()
    }

    pub(crate) fn diagnostic_line_count(&self) -> usize {
        self.lines.len()
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
        overlay: Overlay<'_>,
    ) -> &[UiVertex] {
        self.vertices.clear();
        let scale = self.dpi_scale.max(0.5);
        let width = width as f32 / scale;
        let height = height as f32 / scale;
        if width < 120.0 || height < 100.0 {
            return &self.vertices;
        }
        let context = DrawContext::new(width, height);
        if !self.open {
            hud::append_hud(
                &mut self.vertices,
                context,
                HudState {
                    health,
                    show_crosshair,
                    hit_feedback,
                },
            );
        }
        if let Some(status) = overlay.status {
            hud::append_status(&mut self.vertices, context, status);
        }
        if let Some(zoom) = overlay.overview_zoom {
            zoom::draw(&mut self.vertices, context, zoom);
        }
        if !self.open {
            for label in overlay.celestial_labels {
                text::append_text(
                    &mut self.vertices,
                    context,
                    TextDraw {
                        x: label.x / scale,
                        y: label.y / scale,
                        text: &label.text,
                        color: [0.92, 0.93, 0.82],
                        max_chars: 30,
                        scale: 0.75,
                    },
                );
            }
        }
        if !self.open {
            self.append_chat(context);
            return &self.vertices;
        }
        self.append_console(context);
        &self.vertices
    }

    pub fn take_vertices(&mut self) -> Vec<UiVertex> {
        std::mem::take(&mut self.vertices)
    }

    pub fn restore_vertices(&mut self, vertices: Vec<UiVertex>) {
        self.vertices = vertices;
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
        let max_chars = ((context.width - 32.0) / text::glyph_advance()) as usize;
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
                [0.12, 0.14, 0.17],
            ),
        );
        let border = [0.43, 0.50, 0.58];
        for (x, y, width, height) in [
            (10.0, panel_top, context.width - 20.0, 2.0),
            (10.0, context.height - 12.0, context.width - 20.0, 2.0),
            (10.0, panel_top, 2.0, panel_height),
            (context.width - 12.0, panel_top, 2.0, panel_height),
        ] {
            text::append_quad(
                &mut self.vertices,
                context,
                text::QuadDraw::solid(x, y, width, height, border),
            );
        }
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
        let max_chars = ((context.width - 44.0) / text::glyph_advance()) as usize;
        let hint_space = self.suggestions.lines().len() as f32 * 20.0;
        let visible_lines = ((panel_height - 70.0 - hint_space) / line_height).max(0.0) as usize;
        self.scroll_offset = self
            .scroll_offset
            .min(self.lines.len().saturating_sub(visible_lines));
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
        input_draw::draw(
            &mut self.vertices,
            context,
            &mut self.editor,
            &self.suggestions,
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
