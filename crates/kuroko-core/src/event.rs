//! アプリケーション内で発生するイベントの定義。
//! キーボード入力、PTY出力、リサイズなどをAppEventとして統一表現する。

use ratatui::crossterm::event::KeyEvent;

use crate::types::PaneId;

/// アプリケーション内で発生する全イベントの統一表現。
/// イベントバス経由でメインループに配信される。
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// キーボード入力イベント
    Key(KeyEvent),
    /// ターミナルリサイズイベント（幅, 高さ）
    Resize(u16, u16),
    /// PTYからの出力データ
    PtyOutput { pane_id: PaneId, data: Vec<u8> },
    /// PTYプロセスの終了
    PtyExit { pane_id: PaneId },
    /// 定期描画用のティックイベント
    Tick,
}
