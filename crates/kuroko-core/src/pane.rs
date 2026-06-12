//! 全ペイン型が実装すべきPaneトレイトの定義。
//! ターミナル、エージェント、ファイルツリーなどのペインはこのトレイトを実装する。

use std::any::Any;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::action::Action;
use crate::event::AppEvent;
use crate::types::{PaneId, PaneType};

/// ペインの共通インターフェース。
/// 各ペイン型（ターミナル、エージェント、ファイルツリー）はこのトレイトを実装する。
pub trait Pane: Send {
    /// このペインの一意なIDを返す
    fn id(&self) -> PaneId;

    /// ステータスバーやタブに表示するタイトルを返す
    fn title(&self) -> &str;

    /// ratatuiのフレームに描画する。
    ///
    /// @param frame - ratatuiの描画フレーム
    /// @param area - このペインに割り当てられた描画領域
    /// @param focused - このペインがフォーカスされているかどうか
    fn render(&mut self, frame: &mut Frame, area: Rect, focused: bool);

    /// イベントを処理し、結果としてActionのリストを返す。
    ///
    /// @param event - 処理すべきアプリケーションイベント
    /// @returns 発行されたActionのリスト
    fn handle_event(&mut self, event: &AppEvent) -> Vec<Action>;

    /// このペインがInsertモードでraw入力を受け取りたいかどうかを返す。
    /// PTYベースのペイン（ターミナル、エージェント）はtrueを返す。
    fn wants_raw_input(&self) -> bool;

    /// このペインの種類を返す
    fn pane_type(&self) -> PaneType;

    /// PTYから受信したデータをパーサーに流し込む。
    /// PTYを持つペイン（ターミナル、エージェント）がオーバーライドする。
    ///
    /// @param _data - PTYからの出力バイト列
    fn process_output(&mut self, _data: &[u8]) {}

    /// PTYにバイト列を書き込む（キー入力の転送）。
    /// PTYを持つペイン（ターミナル、エージェント）がオーバーライドする。
    ///
    /// @param _data - 書き込むバイト列
    fn write_to_pty(&mut self, _data: &[u8]) {}

    /// 安全なダウンキャスト用に&dyn Anyを返す
    fn as_any(&self) -> &dyn Any;

    /// 安全なダウンキャスト用に&mut dyn Anyを返す
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
