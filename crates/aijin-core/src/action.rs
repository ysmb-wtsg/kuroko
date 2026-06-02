//! アプリケーション内の全操作を統一的に表現するAction列挙型。
//! イベントハンドラからメインループへの通信手段として機能する。

use std::path::PathBuf;

use crate::types::{Direction, Mode, PaneId, SideContent};

/// ファイル操作プロンプトの種別。
/// App側でプロンプト表示→ユーザー入力→FS操作の流れを制御する。
#[derive(Debug, Clone)]
pub enum FilePromptKind {
    /// ファイルまたはディレクトリの作成（末尾 `/` でディレクトリ判別）
    Create { parent_dir: PathBuf },
    /// ファイル/ディレクトリのリネーム
    Rename { path: PathBuf, current_name: String },
    /// ファイル/ディレクトリの削除確認（ゴミ箱移動）
    Delete { paths: Vec<PathBuf> },
}

/// アプリケーション全体で発行される操作の統一表現。
/// メインイベントループがこのenumをmatch式でディスパッチする。
#[derive(Debug, Clone)]
pub enum Action {
    // --- レイアウト操作 ---
    /// ペインをリサイズする（方向と変化量）
    ResizePane { direction: Direction, amount: i16 },

    // --- フォーカス操作 ---
    /// 次のペインにフォーカスを移動する
    FocusNext,
    /// 前のペインにフォーカスを移動する
    FocusPrev,
    /// 指定方向にフォーカスを移動する
    FocusDirection(Direction),
    /// 指定ペインにフォーカスを移動する（マウスクリック等）
    FocusPane(PaneId),

    // --- モード切替 ---
    /// アプリケーションのモードを切り替える
    SetMode(Mode),

    // --- サイドパネル ---
    /// サイドパネルの表示内容を切り替える（同じ内容なら閉じる）
    ToggleSide(SideContent),
    /// 後方互換: ファイルツリーパネルの表示/非表示を切り替える
    ToggleFileTree,
    /// 後方互換: ターミナルパネルの表示/非表示を切り替える
    ToggleTerminal,
    /// 後方互換: Gitパネルの表示/非表示を切り替える
    ToggleGitPanel,

    // --- PTY操作 ---
    /// フォーカス中のPTYペインにバイト列を書き込む
    PtyWrite { pane_id: PaneId, data: Vec<u8> },

    // --- タブ操作 ---
    /// 新しいエージェントタブを追加する
    NewTab,
    /// アクティブなタブを閉じる
    CloseTab,
    /// 次のタブに切り替える
    NextTab,
    /// 前のタブに切り替える
    PrevTab,
    /// インデックス指定でタブを選択する（0始まり）
    SelectTab(usize),
    /// アクティブタブの名前を変更する
    RenameTab(String),

    // --- ターミナルタブ操作 ---
    /// 新しいターミナルタブを追加する
    NewTerminalTab,
    /// アクティブなターミナルタブを閉じる
    CloseTerminalTab,
    /// 次のターミナルタブに切り替える
    NextTerminalTab,
    /// 前のターミナルタブに切り替える
    PrevTerminalTab,
    /// インデックス指定でターミナルタブを選択する（0始まり）
    SelectTerminalTab(usize),
    /// アクティブなターミナルタブの名前を変更する
    RenameTerminalTab(String),

    // --- ファイルプレビュー ---
    /// ファイルプレビューの表示/非表示をトグルする
    ToggleFilePreview(PathBuf),

    // --- ファイル操作 ---
    /// ファイル操作プロンプトを表示する
    OpenFilePrompt(FilePromptKind),
    /// ファイル詳細情報をフローティング表示する
    ShowFileInfo(PathBuf),
    /// テキストをクリップボードにコピーする
    CopyToClipboard(String),
    /// ファイルパスをアクティブなエージェントタブのPTYに送る
    SendFileToAgent(PathBuf),

    // --- コピーモード ---
    /// ターミナルのコピーモードに入る
    EnterCopyMode,
    /// ターミナルのコピーモードを終了する
    ExitCopyMode,

    // --- 通知 ---
    /// ステータスバーにメッセージを表示する
    Notify(String),

    // --- ヘルプ ---
    /// キーバインドのヘルプオーバーレイを表示する
    ShowHelp,

    // --- アプリ制御 ---
    /// アプリケーションを終了する
    Quit,
    /// 画面を再描画する
    Redraw,
}
