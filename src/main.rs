//! aijin: AIエージェントランチャーTUIのエントリポイント。
//! ターミナルの初期化とAppの起動を行う。

use aijin_tui::App;

fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
