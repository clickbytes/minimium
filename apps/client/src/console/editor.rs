use super::theme::MAX_INPUT;
use std::ops::Range;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: usize,
}

pub(crate) struct Editor {
    pub text: String,
    pub preedit: String,
    composing: bool,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll: usize,
    pub dragging: bool,
    pub changed_at: Instant,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            text: String::with_capacity(MAX_INPUT),
            preedit: String::new(),
            composing: false,
            cursor: 0,
            anchor: 0,
            scroll: 0,
            dragging: false,
            changed_at: Instant::now(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    fn record(&mut self) {
        if self.undo.len() == 128 {
            self.undo.remove(0);
        }
        self.undo.push(self.snapshot());
        self.redo.clear();
    }

    pub fn set_text(&mut self, text: &str) {
        self.clear_preedit();
        self.undo.clear();
        self.redo.clear();
        self.text.clear();
        self.text.push_str(text);
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.scroll = 0;
        self.changed_at = Instant::now();
    }

    pub fn clear(&mut self) -> String {
        self.clear_preedit();
        self.undo.clear();
        self.redo.clear();
        self.scroll = 0;
        self.cursor = 0;
        self.anchor = 0;
        std::mem::take(&mut self.text)
    }

    pub fn replace_selection(&mut self, input: &str) {
        self.clear_preedit();
        let range = self.selection();
        let capacity = MAX_INPUT - (self.text.len() - range.len());
        let filtered: String = input.chars().filter(|ch| !ch.is_control()).collect();
        let mut clean = String::new();
        for grapheme in filtered.graphemes(true) {
            if clean.len() + grapheme.len() > capacity {
                break;
            }
            clean.push_str(grapheme);
        }
        if range.is_empty() && clean.is_empty() {
            return;
        }
        self.record();
        self.text.replace_range(range.clone(), &clean);
        self.cursor = self.next_grapheme_boundary(range.start + clean.len());
        self.anchor = self.cursor;
        self.changed_at = Instant::now();
    }

    pub fn replace_range(&mut self, range: Range<usize>, replacement: &str) {
        if range.start > range.end
            || !self.is_grapheme_boundary(range.start)
            || !self.is_grapheme_boundary(range.end)
        {
            return;
        }
        if self.text.len() - range.len() + replacement.len() > MAX_INPUT {
            return;
        }
        self.record();
        self.text.replace_range(range.clone(), replacement);
        self.cursor = self.next_grapheme_boundary(range.start + replacement.len());
        self.anchor = self.cursor;
        self.changed_at = Instant::now();
    }

    fn is_grapheme_boundary(&self, at: usize) -> bool {
        at == self.text.len()
            || self
                .text
                .grapheme_indices(true)
                .any(|(index, _)| index == at)
    }

    fn next_grapheme_boundary(&self, at: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .find(|index| *index >= at)
            .unwrap_or(self.text.len())
    }

    pub fn move_to(&mut self, target: usize, select: bool) {
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .take_while(|index| *index <= target)
            .last()
            .unwrap_or(0);
        if target >= self.text.len() {
            self.cursor = self.text.len();
        }
        if !select {
            self.anchor = self.cursor;
        }
        self.changed_at = Instant::now();
    }

    pub fn left(&mut self, select: bool, word: bool) {
        if !select && !self.selection().is_empty() {
            self.move_to(self.selection().start, false);
        } else {
            self.move_to(
                if word {
                    self.word_left()
                } else {
                    self.previous()
                },
                select,
            );
        }
    }

    pub fn right(&mut self, select: bool, word: bool) {
        if !select && !self.selection().is_empty() {
            self.move_to(self.selection().end, false);
        } else {
            self.move_to(if word { self.word_right() } else { self.next() }, select);
        }
    }

    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .last()
            .map_or(0, |(at, _)| at)
    }

    fn next(&self) -> usize {
        self.text[self.cursor..]
            .graphemes(true)
            .next()
            .map_or(self.cursor, |grapheme| self.cursor + grapheme.len())
    }

    fn word_left(&self) -> usize {
        let mut pos = self.cursor;
        while pos > 0 {
            let grapheme = self.text[..pos].graphemes(true).next_back().unwrap();
            if !grapheme.chars().all(char::is_whitespace) {
                break;
            }
            pos -= grapheme.len();
        }
        while pos > 0 {
            let grapheme = self.text[..pos].graphemes(true).next_back().unwrap();
            if grapheme.chars().any(char::is_whitespace) {
                break;
            }
            pos -= grapheme.len();
        }
        pos
    }

    fn word_right(&self) -> usize {
        let mut pos = self.cursor;
        while pos < self.text.len() {
            let grapheme = self.text[pos..].graphemes(true).next().unwrap();
            if grapheme.chars().any(char::is_whitespace) {
                break;
            }
            pos += grapheme.len();
        }
        while pos < self.text.len() {
            let grapheme = self.text[pos..].graphemes(true).next().unwrap();
            if !grapheme.chars().all(char::is_whitespace) {
                break;
            }
            pos += grapheme.len();
        }
        pos
    }

    pub fn delete(&mut self, backward: bool, word: bool) {
        let range = self.selection();
        if !range.is_empty() {
            self.replace_selection("");
            return;
        }
        let target = if backward {
            if word {
                self.word_left()
            } else {
                self.previous()
            }
        } else if word {
            self.word_right()
        } else {
            self.next()
        };
        if target != self.cursor {
            self.anchor = target;
            self.replace_selection("");
        }
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
        self.changed_at = Instant::now();
    }

    pub fn set_preedit(&mut self, text: &str) {
        self.preedit.clear();
        self.composing = !text.is_empty();
        let range = self.selection();
        let capacity = MAX_INPUT.saturating_sub(self.text.len().saturating_sub(range.len()));
        for grapheme in text.graphemes(true) {
            if self.preedit.len() + grapheme.len() > capacity {
                break;
            }
            self.preedit.push_str(grapheme);
        }
        self.changed_at = Instant::now();
    }

    pub fn is_composing(&self) -> bool {
        self.composing
    }

    pub fn clear_preedit(&mut self) {
        self.preedit.clear();
        self.composing = false;
    }

    pub fn undo(&mut self) {
        if let Some(snapshot) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(snapshot);
        }
    }

    pub fn redo(&mut self) {
        if let Some(snapshot) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(snapshot);
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.text = snapshot.text;
        self.cursor = snapshot.cursor;
        self.anchor = snapshot.anchor;
        self.changed_at = Instant::now();
    }

    pub fn ensure_visible(&mut self, columns: usize) {
        let cursor = self.text[..self.cursor].graphemes(true).count();
        if cursor < self.scroll {
            self.scroll = cursor;
        }
        if cursor >= self.scroll + columns {
            self.scroll = cursor + 1 - columns;
        }
        self.scroll = self.scroll.min(
            self.text
                .graphemes(true)
                .count()
                .saturating_add(1)
                .saturating_sub(columns),
        );
    }
}
