use std::collections::VecDeque;
use vte::{Params, Perform};

const MAX_SCROLLBACK: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermColor {
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl TermColor {
    pub fn to_egui(self, default: egui::Color32) -> egui::Color32 {
        match self {
            TermColor::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
            TermColor::Indexed(idx) => ansi_256_to_rgb(idx).unwrap_or(default),
        }
    }
}

fn ansi_256_to_rgb(idx: u8) -> Option<egui::Color32> {
    const BASIC_16: [(u8, u8, u8); 16] = [
        (24, 26, 36),    // 0 Black
        (239, 83, 80),   // 1 Red
        (82, 209, 138),  // 2 Green
        (250, 198, 72),  // 3 Yellow
        (96, 165, 250),  // 4 Blue
        (192, 132, 252), // 5 Magenta
        (56, 211, 211),  // 6 Cyan
        (214, 220, 232), // 7 White
        (108, 117, 138), // 8 Bright Black
        (252, 129, 129), // 9 Bright Red
        (110, 231, 183), // 10 Bright Green
        (253, 224, 71),  // 11 Bright Yellow
        (147, 197, 253), // 12 Bright Blue
        (216, 180, 254), // 13 Bright Magenta
        (103, 232, 249), // 14 Bright Cyan
        (248, 250, 252), // 15 Bright White
    ];
    if (idx as usize) < 16 {
        let (r, g, b) = BASIC_16[idx as usize];
        return Some(egui::Color32::from_rgb(r, g, b));
    }
    if idx < 232 {
        let c = idx - 16;
        let r = c / 36;
        let g = (c % 36) / 6;
        let b = c % 6;
        let map = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        return Some(egui::Color32::from_rgb(map(r), map(g), map(b)));
    }
    let gray = 8 + (idx - 232) * 10;
    Some(egui::Color32::from_rgb(gray, gray, gray))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    pub ch: char,
    pub fg: Option<TermColor>,
    pub bg: Option<TermColor>,
    pub bold: bool,
    pub dim: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            bold: false,
            dim: false,
            underline: false,
            inverse: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct CellStyle {
    fg: Option<TermColor>,
    bg: Option<TermColor>,
    bold: bool,
    dim: bool,
    underline: bool,
    inverse: bool,
}

pub struct TerminalState {
    pub cols: usize,
    pub rows: usize,
    pub grid: Vec<Vec<TerminalCell>>,
    alt_grid: Vec<Vec<TerminalCell>>,
    pub using_alt: bool,
    pub scrollback: VecDeque<Vec<TerminalCell>>,
    pub scroll_offset: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
    saved_cursor: (usize, usize),
    scroll_top: usize,
    scroll_bottom: usize,
    style: CellStyle,
    wrap_pending: bool,
}

impl TerminalState {
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(10);
        let rows = rows.max(4);
        let blank = vec![vec![TerminalCell::default(); cols]; rows];
        Self {
            cols,
            rows,
            grid: blank.clone(),
            alt_grid: blank,
            using_alt: false,
            scrollback: VecDeque::with_capacity(MAX_SCROLLBACK),
            scroll_offset: 0,
            cursor_row: 0,
            cursor_col: 0,
            saved_cursor: (0, 0),
            scroll_top: 0,
            scroll_bottom: rows - 1,
            style: CellStyle::default(),
            wrap_pending: false,
        }
    }

    pub fn resize(&mut self, new_cols: usize, new_rows: usize) {
        let new_cols = new_cols.max(10);
        let new_rows = new_rows.max(4);
        if new_cols == self.cols && new_rows == self.rows {
            return;
        }
        Self::resize_grid(&mut self.grid, new_cols, new_rows);
        Self::resize_grid(&mut self.alt_grid, new_cols, new_rows);
        self.cols = new_cols;
        self.rows = new_rows;
        self.cursor_row = self.cursor_row.min(new_rows - 1);
        self.cursor_col = self.cursor_col.min(new_cols - 1);
        self.scroll_top = 0;
        self.scroll_bottom = new_rows - 1;
        self.wrap_pending = false;
    }

    fn resize_grid(grid: &mut Vec<Vec<TerminalCell>>, cols: usize, rows: usize) {
        for row in grid.iter_mut() {
            row.resize(cols, TerminalCell::default());
        }
        while grid.len() < rows {
            grid.push(vec![TerminalCell::default(); cols]);
        }
        if grid.len() > rows {
            let excess = grid.len() - rows;
            grid.drain(0..excess);
        }
    }

    fn active_grid_mut(&mut self) -> &mut Vec<Vec<TerminalCell>> {
        if self.using_alt {
            &mut self.alt_grid
        } else {
            &mut self.grid
        }
    }

    pub fn active_grid(&self) -> &Vec<Vec<TerminalCell>> {
        if self.using_alt {
            &self.alt_grid
        } else {
            &self.grid
        }
    }

    pub fn visible_rows(&self) -> Vec<&[TerminalCell]> {
        if self.using_alt || self.scroll_offset == 0 {
            return self.active_grid().iter().map(|r| r.as_slice()).collect();
        }
        let sb_len = self.scrollback.len();
        let offset = self.scroll_offset.min(sb_len);
        let mut out = Vec::with_capacity(self.rows);
        let start_sb = sb_len - offset;
        for i in start_sb..sb_len {
            if out.len() >= self.rows {
                break;
            }
            out.push(self.scrollback[i].as_slice());
        }
        for row in &self.grid {
            if out.len() >= self.rows {
                break;
            }
            out.push(row.as_slice());
        }
        out
    }

    pub fn scroll_up(&mut self, lines: usize) {
        if !self.using_alt {
            self.scroll_offset = (self.scroll_offset + lines).min(self.scrollback.len());
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn clear(&mut self) {
        let blank = vec![vec![TerminalCell::default(); self.cols]; self.rows];
        self.grid = blank;
        self.scrollback.clear();
        self.scroll_offset = 0;
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.wrap_pending = false;
    }

    fn scroll_region_up(&mut self, count: usize) {
        let top = self.scroll_top.min(self.rows - 1);
        let bottom = self.scroll_bottom.min(self.rows - 1);
        if top > bottom {
            return;
        }
        let cols = self.cols;
        for _ in 0..count {
            let removed = if !self.using_alt && top == 0 {
                let row = self.grid.remove(0);
                self.grid.insert(bottom, vec![TerminalCell::default(); cols]);
                Some(row)
            } else {
                let g = self.active_grid_mut();
                g.remove(top);
                g.insert(bottom, vec![TerminalCell::default(); cols]);
                None
            };
            if let Some(row) = removed {
                if self.scrollback.len() >= MAX_SCROLLBACK {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(row);
            }
        }
    }

    fn scroll_region_down(&mut self, count: usize) {
        let top = self.scroll_top.min(self.rows - 1);
        let bottom = self.scroll_bottom.min(self.rows - 1);
        if top > bottom {
            return;
        }
        let cols = self.cols;
        let g = self.active_grid_mut();
        for _ in 0..count {
            g.remove(bottom);
            g.insert(top, vec![TerminalCell::default(); cols]);
        }
    }

    fn line_feed(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_bottom {
            self.scroll_region_up(1);
        } else if self.cursor_row + 1 < self.rows {
            self.cursor_row += 1;
        }
    }

    fn put_char(&mut self, ch: char) {
        if self.wrap_pending {
            self.cursor_col = 0;
            self.line_feed();
        }
        let r = self.cursor_row.min(self.rows - 1);
        let c = self.cursor_col.min(self.cols - 1);
        let style = self.style;
        let g = self.active_grid_mut();
        g[r][c] = TerminalCell {
            ch,
            fg: style.fg,
            bg: style.bg,
            bold: style.bold,
            dim: style.dim,
            underline: style.underline,
            inverse: style.inverse,
        };
        if self.cursor_col + 1 >= self.cols {
            self.wrap_pending = true;
        } else {
            self.cursor_col += 1;
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        let mut iter = params.iter();
        let mut any = false;
        while let Some(param) = iter.next() {
            any = true;
            let code = param[0];
            match code {
                0 => self.style = CellStyle::default(),
                1 => self.style.bold = true,
                2 => self.style.dim = true,
                4 => self.style.underline = true,
                7 => self.style.inverse = true,
                22 => {
                    self.style.bold = false;
                    self.style.dim = false;
                }
                24 => self.style.underline = false,
                27 => self.style.inverse = false,
                30..=37 => self.style.fg = Some(TermColor::Indexed((code - 30) as u8)),
                38 => {
                    if let Some(mode) = iter.next() {
                        match mode[0] {
                            5 => {
                                if let Some(idx) = iter.next() {
                                    self.style.fg = Some(TermColor::Indexed(idx[0] as u8));
                                }
                            }
                            2 => {
                                if let (Some(r), Some(g), Some(b)) =
                                    (iter.next(), iter.next(), iter.next())
                                {
                                    self.style.fg = Some(TermColor::Rgb(
                                        r[0] as u8, g[0] as u8, b[0] as u8,
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.style.fg = None,
                40..=47 => self.style.bg = Some(TermColor::Indexed((code - 40) as u8)),
                48 => {
                    if let Some(mode) = iter.next() {
                        match mode[0] {
                            5 => {
                                if let Some(idx) = iter.next() {
                                    self.style.bg = Some(TermColor::Indexed(idx[0] as u8));
                                }
                            }
                            2 => {
                                if let (Some(r), Some(g), Some(b)) =
                                    (iter.next(), iter.next(), iter.next())
                                {
                                    self.style.bg = Some(TermColor::Rgb(
                                        r[0] as u8, g[0] as u8, b[0] as u8,
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.style.bg = None,
                90..=97 => self.style.fg = Some(TermColor::Indexed((code - 90 + 8) as u8)),
                100..=107 => self.style.bg = Some(TermColor::Indexed((code - 100 + 8) as u8)),
                _ => {}
            }
        }
        if !any {
            self.style = CellStyle::default();
        }
    }

    pub fn text_snapshot(&self) -> String {
        let mut out = String::new();
        for row in self.active_grid() {
            let line: String = row.iter().map(|c| c.ch).collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}

impl Perform for TerminalState {
    fn print(&mut self, c: char) {
        self.scroll_to_bottom();
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        self.scroll_to_bottom();
        match byte {
            0x08 => {
                self.wrap_pending = false;
                self.cursor_col = self.cursor_col.saturating_sub(1);
            }
            0x09 => {
                self.wrap_pending = false;
                let next_tab = ((self.cursor_col / 8) + 1) * 8;
                self.cursor_col = next_tab.min(self.cols - 1);
            }
            0x0a..=0x0c => self.line_feed(),
            0x0d => {
                self.wrap_pending = false;
                self.cursor_col = 0;
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let p = |idx: usize, default: usize| -> usize {
            params
                .iter()
                .nth(idx)
                .and_then(|s| s.first().copied())
                .map(|v| if v == 0 { default } else { v as usize })
                .unwrap_or(default)
        };
        let p_raw = |idx: usize, default: usize| -> usize {
            params
                .iter()
                .nth(idx)
                .and_then(|s| s.first().copied())
                .map(|v| v as usize)
                .unwrap_or(default)
        };

        if intermediates == b"?" {
            if let Some(first) = params.iter().next().and_then(|s| s.first().copied()) {
                if first == 1049 || first == 47 || first == 1047 {
                    match action {
                        'h' => {
                            self.saved_cursor = (self.cursor_row, self.cursor_col);
                            self.using_alt = true;
                            let blank = vec![vec![TerminalCell::default(); self.cols]; self.rows];
                            self.alt_grid = blank;
                            self.cursor_row = 0;
                            self.cursor_col = 0;
                            self.scroll_offset = 0;
                        }
                        'l' => {
                            self.using_alt = false;
                            self.cursor_row = self.saved_cursor.0.min(self.rows - 1);
                            self.cursor_col = self.saved_cursor.1.min(self.cols - 1);
                        }
                        _ => {}
                    }
                }
            }
            return;
        }

        self.wrap_pending = false;
        match action {
            'A' => self.cursor_row = self.cursor_row.saturating_sub(p(0, 1)),
            'B' => self.cursor_row = (self.cursor_row + p(0, 1)).min(self.rows - 1),
            'C' => self.cursor_col = (self.cursor_col + p(0, 1)).min(self.cols - 1),
            'D' => self.cursor_col = self.cursor_col.saturating_sub(p(0, 1)),
            'E' => {
                self.cursor_row = (self.cursor_row + p(0, 1)).min(self.rows - 1);
                self.cursor_col = 0;
            }
            'F' => {
                self.cursor_row = self.cursor_row.saturating_sub(p(0, 1));
                self.cursor_col = 0;
            }
            'G' => self.cursor_col = (p(0, 1) - 1).min(self.cols - 1),
            'H' | 'f' => {
                let r = p(0, 1).saturating_sub(1).min(self.rows - 1);
                let c = p(1, 1).saturating_sub(1).min(self.cols - 1);
                self.cursor_row = r;
                self.cursor_col = c;
            }
            'J' => {
                let mode = p_raw(0, 0);
                let r = self.cursor_row;
                let c = self.cursor_col;
                let cols = self.cols;
                let rows = self.rows;
                let g = self.active_grid_mut();
                match mode {
                    0 => {
                        for cell in &mut g[r][c..] {
                            *cell = TerminalCell::default();
                        }
                        for row in &mut g[(r + 1)..rows] {
                            row.fill(TerminalCell::default());
                        }
                    }
                    1 => {
                        for row in &mut g[0..r] {
                            row.fill(TerminalCell::default());
                        }
                        for cell in &mut g[r][..=c.min(cols - 1)] {
                            *cell = TerminalCell::default();
                        }
                    }
                    2 | 3 => {
                        for row in g.iter_mut() {
                            row.fill(TerminalCell::default());
                        }
                    }
                    _ => {}
                }
            }
            'K' => {
                let mode = p_raw(0, 0);
                let r = self.cursor_row;
                let c = self.cursor_col;
                let cols = self.cols;
                let g = self.active_grid_mut();
                match mode {
                    0 => g[r][c..].fill(TerminalCell::default()),
                    1 => g[r][..=c.min(cols - 1)].fill(TerminalCell::default()),
                    2 => g[r].fill(TerminalCell::default()),
                    _ => {}
                }
            }
            'L' => {
                let count = p(0, 1);
                let r = self.cursor_row;
                let cols = self.cols;
                let bottom = self.scroll_bottom;
                let g = self.active_grid_mut();
                for _ in 0..count {
                    if r <= bottom {
                        g.remove(bottom);
                        g.insert(r, vec![TerminalCell::default(); cols]);
                    }
                }
            }
            'M' => {
                let count = p(0, 1);
                let r = self.cursor_row;
                let cols = self.cols;
                let bottom = self.scroll_bottom;
                let g = self.active_grid_mut();
                for _ in 0..count {
                    if r <= bottom {
                        g.remove(r);
                        g.insert(bottom, vec![TerminalCell::default(); cols]);
                    }
                }
            }
            'P' => {
                let count = p(0, 1);
                let r = self.cursor_row;
                let c = self.cursor_col;
                let cols = self.cols;
                let g = self.active_grid_mut();
                for _ in 0..count {
                    if c < cols {
                        g[r].remove(c);
                        g[r].push(TerminalCell::default());
                    }
                }
            }
            'S' => self.scroll_region_up(p(0, 1)),
            'T' => self.scroll_region_down(p(0, 1)),
            'd' => self.cursor_row = p(0, 1).saturating_sub(1).min(self.rows - 1),
            'm' => self.apply_sgr(params),
            'r' => {
                let top = p(0, 1).saturating_sub(1).min(self.rows - 1);
                let bottom = p(1, self.rows).saturating_sub(1).min(self.rows - 1);
                if top < bottom {
                    self.scroll_top = top;
                    self.scroll_bottom = bottom;
                    self.cursor_row = 0;
                    self.cursor_col = 0;
                }
            }
            's' => self.saved_cursor = (self.cursor_row, self.cursor_col),
            'u' => {
                self.cursor_row = self.saved_cursor.0.min(self.rows - 1);
                self.cursor_col = self.saved_cursor.1.min(self.cols - 1);
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'M' => {
                if self.cursor_row == self.scroll_top {
                    self.scroll_region_down(1);
                } else {
                    self.cursor_row = self.cursor_row.saturating_sub(1);
                }
            }
            b'7' => self.saved_cursor = (self.cursor_row, self.cursor_col),
            b'8' => {
                self.cursor_row = self.saved_cursor.0.min(self.rows - 1);
                self.cursor_col = self.saved_cursor.1.min(self.cols - 1);
            }
            _ => {}
        }
    }
}

pub struct TerminalEmulator {
    pub state: TerminalState,
    parser: vte::Parser,
}

impl TerminalEmulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            state: TerminalState::new(cols, rows),
            parser: vte::Parser::new(),
        }
    }

    pub fn feed_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.parser.advance(&mut self.state, b);
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.state.resize(cols, rows);
    }
}

pub fn encode_egui_key(key: egui::Key, modifiers: egui::Modifiers) -> Option<Vec<u8>> {
    if modifiers.ctrl && !modifiers.alt {
        let ctrl_byte = match key {
            egui::Key::A => Some(0x01),
            egui::Key::B => Some(0x02),
            egui::Key::C => Some(0x03),
            egui::Key::D => Some(0x04),
            egui::Key::E => Some(0x05),
            egui::Key::F => Some(0x06),
            egui::Key::G => Some(0x07),
            egui::Key::H => Some(0x08),
            egui::Key::K => Some(0x0b),
            egui::Key::L => Some(0x0c),
            egui::Key::N => Some(0x0e),
            egui::Key::P => Some(0x10),
            egui::Key::R => Some(0x12),
            egui::Key::U => Some(0x15),
            egui::Key::W => Some(0x17),
            egui::Key::Z => Some(0x1a),
            _ => None,
        };
        if let Some(b) = ctrl_byte {
            return Some(vec![b]);
        }
    }

    let bytes: &[u8] = match key {
        egui::Key::Enter => b"\r",
        egui::Key::Backspace => b"\x7f",
        egui::Key::Tab => b"\t",
        egui::Key::Escape => b"\x1b",
        egui::Key::ArrowUp => b"\x1b[A",
        egui::Key::ArrowDown => b"\x1b[B",
        egui::Key::ArrowRight => b"\x1b[C",
        egui::Key::ArrowLeft => b"\x1b[D",
        egui::Key::Home => b"\x1b[H",
        egui::Key::End => b"\x1b[F",
        egui::Key::PageUp => b"\x1b[5~",
        egui::Key::PageDown => b"\x1b[6~",
        egui::Key::Insert => b"\x1b[2~",
        egui::Key::Delete => b"\x1b[3~",
        egui::Key::F1 => b"\x1bOP",
        egui::Key::F2 => b"\x1bOQ",
        egui::Key::F3 => b"\x1bOR",
        egui::Key::F4 => b"\x1bOS",
        egui::Key::F5 => b"\x1b[15~",
        egui::Key::F6 => b"\x1b[17~",
        egui::Key::F7 => b"\x1b[18~",
        egui::Key::F8 => b"\x1b[19~",
        egui::Key::F9 => b"\x1b[20~",
        egui::Key::F10 => b"\x1b[21~",
        egui::Key::F11 => b"\x1b[23~",
        egui::Key::F12 => b"\x1b[24~",
        _ => return None,
    };
    Some(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_text_ansi_colors_and_alt_screen() {
        let mut term = TerminalEmulator::new(40, 10);
        term.feed_bytes(b"\x1b[32mhello\x1b[0m world\r\nline2");

        let snap = term.state.text_snapshot();
        assert!(snap.starts_with("hello world\nline2"));
        assert_eq!(
            term.state.grid[0][0].fg,
            Some(TermColor::Indexed(2))
        );
        assert_eq!(term.state.grid[0][6].fg, None);

        // Enter alt screen, write something, exit alt screen
        term.feed_bytes(b"\x1b[?1049hALT_SCREEN");
        assert!(term.state.using_alt);
        assert!(term.state.text_snapshot().starts_with("ALT_SCREEN"));

        term.feed_bytes(b"\x1b[?1049l");
        assert!(!term.state.using_alt);
        assert!(term.state.text_snapshot().starts_with("hello world\nline2"));
    }

    #[test]
    fn test_terminal_scrollback_and_key_encoding() {
        let mut term = TerminalEmulator::new(20, 4);
        for i in 0..10 {
            term.feed_bytes(format!("line {}\r\n", i).as_bytes());
        }
        assert!(!term.state.scrollback.is_empty());
        term.state.scroll_up(3);
        assert!(term.state.scroll_offset > 0);
        term.state.scroll_down(3);
        assert_eq!(term.state.scroll_offset, 0);

        let ctrl_c = encode_egui_key(
            egui::Key::C,
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        assert_eq!(ctrl_c, Some(vec![0x03]));
        assert_eq!(
            encode_egui_key(egui::Key::ArrowUp, egui::Modifiers::NONE),
            Some(b"\x1b[A".to_vec())
        );
    }
}
