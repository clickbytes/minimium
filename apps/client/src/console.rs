use std::collections::VecDeque;

const CELL: usize = 8;
const COLUMNS: usize = 16;
const ATLAS_WIDTH: usize = CELL * COLUMNS;
const ATLAS_HEIGHT: usize = CELL * 8;
const MAX_LINES: usize = 128;
const MAX_INPUT: usize = 128;
const MAX_VERTICES: usize = 8192;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    Help,
    Connect(String),
    Disconnect,
    FpsShow,
    NetStats,
    Ping,
    Say(String),
    State,
    Clear,
    Quit,
}

pub fn parse_command(input: &str) -> Result<Option<ClientCommand>, String> {
    let words: Vec<_> = input.split_whitespace().collect();
    let command = match words.as_slice() {
        [] => return Ok(None),
        ["help"] => ClientCommand::Help,
        ["connect", address] => ClientCommand::Connect((*address).to_owned()),
        ["disconnect"] => ClientCommand::Disconnect,
        ["fps", "show"] => ClientCommand::FpsShow,
        ["net", "stats"] => ClientCommand::NetStats,
        ["ping"] => ClientCommand::Ping,
        ["say", name] => ClientCommand::Say((*name).to_owned()),
        ["state"] => ClientCommand::State,
        ["clear"] => ClientCommand::Clear,
        ["quit"] | ["exit"] => ClientCommand::Quit,
        _ => return Err("unknown command; press Tab for suggestions or type help".into()),
    };
    Ok(Some(command))
}

struct ConsoleLine {
    text: String,
    color: [f32; 3],
}

pub struct Console {
    pub open: bool,
    input: String,
    history: Vec<String>,
    history_cursor: Option<usize>,
    lines: VecDeque<ConsoleLine>,
    vertices: Vec<UiVertex>,
}

impl Console {
    pub fn new() -> Self {
        Self {
            open: false,
            input: String::with_capacity(MAX_INPUT),
            history: Vec::with_capacity(32),
            history_cursor: None,
            lines: VecDeque::with_capacity(MAX_LINES),
            vertices: Vec::with_capacity(MAX_VERTICES),
        }
    }

    pub fn push_line(&mut self, text: impl AsRef<str>) {
        self.push_colored(text.as_ref(), [0.78, 0.86, 0.92]);
    }

    pub fn push_error(&mut self, text: impl AsRef<str>) {
        self.push_colored(text.as_ref(), [1.0, 0.40, 0.34]);
    }

    fn push_colored(&mut self, text: &str, color: [f32; 3]) {
        let mut clean = String::with_capacity(text.len().min(192));
        for character in text.chars().take(192) {
            clean.push(if character.is_control() {
                ' '
            } else {
                character
            });
        }
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(ConsoleLine { text: clean, color });
    }

    pub fn append_text(&mut self, text: &str) {
        for character in text.chars() {
            if self.input.len() >= MAX_INPUT {
                break;
            }
            if character.is_ascii_graphic() || character == ' ' {
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
        let input = std::mem::take(&mut self.input);
        self.input = String::with_capacity(MAX_INPUT);
        self.history_cursor = None;
        if !input.trim().is_empty() {
            if self.history.last() != Some(&input) {
                if self.history.len() == MAX_LINES {
                    self.history.remove(0);
                }
                self.history.push(input.clone());
            }
            self.push_colored(&format!("> {input}"), [0.35, 0.88, 0.88]);
        }
        input
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = self
            .history_cursor
            .unwrap_or(self.history.len())
            .saturating_sub(1);
        self.history_cursor = Some(index);
        self.input.clear();
        self.input.push_str(&self.history[index]);
    }

    pub fn history_down(&mut self) {
        let Some(index) = self.history_cursor else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_cursor = None;
            self.input.clear();
        } else {
            self.history_cursor = Some(index + 1);
            self.input.clear();
            self.input.push_str(&self.history[index + 1]);
        }
    }

    pub fn complete_input(&mut self) {
        const COMMANDS: [&str; 10] = [
            "clear",
            "connect",
            "disconnect",
            "fps show",
            "help",
            "net stats",
            "ping",
            "quit",
            "say",
            "state",
        ];
        let input = self.input.trim();
        let matches: Vec<_> = COMMANDS
            .iter()
            .filter(|command| command.starts_with(input))
            .collect();
        if matches.len() == 1 {
            self.input.clear();
            self.input.push_str(matches[0]);
            self.input.push(' ');
        } else if matches.len() > 1 {
            let mut suggestions = String::new();
            for command in &matches {
                if !suggestions.is_empty() {
                    suggestions.push_str("  ");
                }
                suggestions.push_str(command);
            }
            self.push_line(suggestions);
        }
    }

    pub fn clear_lines(&mut self) {
        self.lines.clear();
    }

    pub fn build_vertices(&mut self, width: u32, height: u32) -> &[UiVertex] {
        self.vertices.clear();
        if !self.open || width < 120 || height < 100 {
            return &self.vertices;
        }
        let width = width as f32;
        let height = height as f32;
        let panel_height = 260.0f32.min(height - 20.0);
        let panel_top = height - panel_height - 10.0;
        append_quad(
            &mut self.vertices,
            width,
            height,
            10.0,
            panel_top,
            width - 20.0,
            panel_height,
            [0.025, 0.035, 0.050],
            white_uv(),
            white_uv(),
        );
        append_quad(
            &mut self.vertices,
            width,
            height,
            16.0,
            height - 48.0,
            width - 32.0,
            30.0,
            [0.07, 0.10, 0.13],
            white_uv(),
            white_uv(),
        );
        let scale = 2.0;
        let line_height = 20.0;
        let max_chars = ((width - 44.0) / (CELL as f32 * scale)) as usize;
        let visible_lines = ((panel_height - 70.0) / line_height).max(0.0) as usize;
        let first_line = self.lines.len().saturating_sub(visible_lines);
        for (row, line) in self.lines.iter().skip(first_line).enumerate() {
            append_text(
                &mut self.vertices,
                width,
                height,
                22.0,
                panel_top + 13.0 + row as f32 * line_height,
                &line.text,
                line.color,
                max_chars,
                scale,
            );
        }
        append_text(
            &mut self.vertices,
            width,
            height,
            22.0,
            height - 42.0,
            &format!("> {}", self.input),
            [0.88, 0.96, 0.98],
            max_chars,
            scale,
        );
        &self.vertices
    }
}

pub fn font_atlas_pixels() -> Vec<u8> {
    let mut pixels = vec![0; ATLAS_WIDTH * ATLAS_HEIGHT];
    for code in 0..127usize {
        let rows = glyph_rows(char::from_u32(code as u32).unwrap_or('?'));
        let cell_x = (code % COLUMNS) * CELL;
        let cell_y = (code / COLUMNS) * CELL;
        for (y, row) in rows.iter().enumerate() {
            for x in 0..5 {
                if row & (1 << (4 - x)) != 0 {
                    pixels[(cell_y + y) * ATLAS_WIDTH + cell_x + x + 1] = 255;
                }
            }
        }
    }
    let white_x = 127 % COLUMNS * CELL;
    let white_y = 127 / COLUMNS * CELL;
    for y in 0..CELL {
        for x in 0..CELL {
            pixels[(white_y + y) * ATLAS_WIDTH + white_x + x] = 255;
        }
    }
    pixels
}

fn append_text(
    vertices: &mut Vec<UiVertex>,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    text: &str,
    color: [f32; 3],
    max_chars: usize,
    scale: f32,
) {
    for (column, character) in text.chars().take(max_chars).enumerate() {
        if vertices.len() + 6 > MAX_VERTICES {
            return;
        }
        let character = if character.is_ascii() {
            character.to_ascii_uppercase()
        } else {
            '?'
        };
        let code = (character as usize).min(126);
        let cell_x = (code % COLUMNS) * CELL;
        let cell_y = (code / COLUMNS) * CELL;
        let uv_min = [
            (cell_x as f32 + 0.5) / ATLAS_WIDTH as f32,
            (cell_y as f32 + 0.5) / ATLAS_HEIGHT as f32,
        ];
        let uv_max = [
            (cell_x as f32 + CELL as f32 - 0.5) / ATLAS_WIDTH as f32,
            (cell_y as f32 + CELL as f32 - 0.5) / ATLAS_HEIGHT as f32,
        ];
        let left = x + column as f32 * CELL as f32 * scale;
        append_quad(
            vertices,
            width,
            height,
            left,
            y,
            CELL as f32 * scale,
            CELL as f32 * scale,
            color,
            uv_min,
            uv_max,
        );
    }
}

fn append_quad(
    vertices: &mut Vec<UiVertex>,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    quad_width: f32,
    quad_height: f32,
    color: [f32; 3],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
) {
    if vertices.len() + 6 > MAX_VERTICES {
        return;
    }
    let top_left = screen_position(x, y, width, height);
    let top_right = screen_position(x + quad_width, y, width, height);
    let bottom_left = screen_position(x, y + quad_height, width, height);
    let bottom_right = screen_position(x + quad_width, y + quad_height, width, height);
    let make = |position, uv| UiVertex {
        position,
        uv,
        color,
    };
    vertices.extend_from_slice(&[
        make(top_left, [uv_min[0], uv_min[1]]),
        make(bottom_left, [uv_min[0], uv_max[1]]),
        make(bottom_right, [uv_max[0], uv_max[1]]),
        make(top_left, [uv_min[0], uv_min[1]]),
        make(bottom_right, [uv_max[0], uv_max[1]]),
        make(top_right, [uv_max[0], uv_min[1]]),
    ]);
}

fn screen_position(x: f32, y: f32, width: f32, height: f32) -> [f32; 2] {
    [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0]
}

fn white_uv() -> [f32; 2] {
    [
        (127 % COLUMNS * CELL + 3) as f32 / ATLAS_WIDTH as f32,
        (127 / COLUMNS * CELL + 3) as f32 / ATLAS_HEIGHT as f32,
    ]
}

fn glyph_rows(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 2, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 25, 21, 19, 19, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ',' => [0, 0, 0, 0, 0, 12, 8],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        ';' => [0, 4, 4, 0, 4, 4, 8],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '\\' => [16, 8, 8, 4, 2, 2, 1],
        '>' => [16, 8, 4, 2, 4, 8, 16],
        '<' => [1, 2, 4, 8, 4, 2, 1],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '\'' => [4, 4, 8, 0, 0, 0, 0],
        '"' => [10, 10, 10, 0, 0, 0, 0],
        '#' => [10, 31, 10, 10, 31, 10, 0],
        '%' => [17, 2, 4, 8, 16, 17, 0],
        '*' => [0, 17, 10, 31, 10, 17, 0],
        _ => [0; 7],
    }
}
