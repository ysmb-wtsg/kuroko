//! vt100のScreen状態をratatuiのBufferに変換する描画ウィジェット。
//! セルごとに文字・色・スタイルをマッピングする。

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use kuroko_core::theme;

/// vt100のScreenへの参照を保持し、ratatuiウィジェットとして描画する。
/// コピーモード時はカーソル位置と選択範囲のハイライトも描画する。
pub struct TerminalWidget<'a> {
    /// vt100のスクリーン状態
    screen: &'a vt100::Screen,
    /// コピーモードのカーソル位置（None = コピーモードでない）
    cursor: Option<(u16, u16)>,
    /// セルが選択範囲内かを判定するクロージャ的コールバック
    /// (row, col) -> bool
    selection_check: Option<Box<dyn Fn(u16, u16) -> bool + 'a>>,
}

impl<'a> TerminalWidget<'a> {
    /// 新しいTerminalWidgetを作成する。
    ///
    /// @param screen - 描画元のvt100 Screen
    pub fn new(screen: &'a vt100::Screen) -> Self {
        Self {
            screen,
            cursor: None,
            selection_check: None,
        }
    }

    /// コピーモードのカーソルと選択判定を設定する。
    ///
    /// @param cursor - カーソル位置 (row, col)
    /// @param selection_check - セルが選択範囲内かを判定する関数
    pub fn with_copy_mode(
        mut self,
        cursor: (u16, u16),
        selection_check: impl Fn(u16, u16) -> bool + 'a,
    ) -> Self {
        self.cursor = Some(cursor);
        self.selection_check = Some(Box::new(selection_check));
        self
    }
}

impl Widget for TerminalWidget<'_> {
    /// vt100のScreen状態をratatuiのBufferに描画する。
    /// 各セルの文字・前景色・背景色・スタイル属性を変換する。
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = area.height.min(self.screen.size().0);
        let cols = area.width.min(self.screen.size().1);

        for row in 0..rows {
            for col in 0..cols {
                let cell = self.screen.cell(row, col);
                if let Some(cell) = cell {
                    let ch = cell.contents();
                    let fg = convert_color(cell.fgcolor());
                    let bg = convert_color(cell.bgcolor());

                    let mut modifier = Modifier::empty();
                    if cell.bold() {
                        modifier |= Modifier::BOLD;
                    }
                    if cell.italic() {
                        modifier |= Modifier::ITALIC;
                    }
                    if cell.underline() {
                        modifier |= Modifier::UNDERLINED;
                    }
                    if cell.inverse() {
                        modifier |= Modifier::REVERSED;
                    }

                    let style = Style::default().fg(fg).bg(bg).add_modifier(modifier);

                    // 選択範囲のセルは固定の選択色で背景をハイライト
                    let is_selected = self
                        .selection_check
                        .as_ref()
                        .map(|check| check(row, col))
                        .unwrap_or(false);
                    let style = if is_selected {
                        style.bg(theme::get().surface_selection)
                    } else {
                        style
                    };

                    // カーソル位置のセルはREVERSEDで表示
                    let is_cursor = self.cursor == Some((row, col));
                    let style = if is_cursor {
                        style.add_modifier(Modifier::REVERSED)
                    } else {
                        style
                    };

                    let x = area.x + col;
                    let y = area.y + row;
                    if x < area.right() && y < area.bottom() {
                        let display_char = if ch.is_empty() { " " } else { &ch };
                        buf[(x, y)].set_symbol(display_char).set_style(style);
                    }
                }
            }
        }
    }
}

/// vt100の色をratatuiのColorに変換する。
///
/// @param color - vt100の色値
/// @returns 対応するratatuiのColor
fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
