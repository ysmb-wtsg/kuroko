//! kuroko: AIエージェントランチャーTUIのエントリポイント。
//! ターミナルの初期化とAppの起動を行う。

use kuroko_tui::App;

fn main() -> std::io::Result<()> {
    // TUI起動前にバージョン照会へ応答する（Homebrew等のパッケージ検証で使用）
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("krk {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
