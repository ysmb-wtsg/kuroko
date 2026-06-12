//! デザイントークンの一元管理。
//! `Theme`構造体として保持し、`set()`で実行時に差し替え可能（Luaテーマカスタマイズの布石）。

use std::sync::RwLock;

use ratatui::style::Color;

/// アプリ全体のデザイントークン。
/// 全色はoklchベースで設計し、sRGB変換値を `Color::Rgb` で保持する。
/// アクセントカラーは役割ベースの4色（primary/positive/warning/error）に限定する。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    // --- Surface（背景色） ---
    /// メインのダーク背景。タブバー、ステータスバーに使用。
    /// oklch(0.14 0.012 275)
    pub surface: Color,
    /// アクティブ要素の背景。選択中のタブ、カーソル行等。
    /// oklch(0.27 0.012 275)
    pub surface_active: Color,
    /// フローティングオーバーレイの背景。プレビュー、プロンプト、ファイル情報。
    /// oklch(0.10 0.010 275)
    pub surface_overlay: Color,
    /// リスト項目のハイライト背景（非フォーカス時）。
    /// oklch(0.16 0.000 0)
    pub surface_highlight: Color,
    /// コピーモードのテキスト選択背景。
    /// oklch(0.30 0.060 250)
    pub surface_selection: Color,

    // --- Text（テキスト色） ---
    /// プライマリテキスト。タブ名、タイトル等の強調テキスト。
    /// oklch(0.90 0.015 275)
    pub text_primary: Color,
    /// ボディテキスト。ファイル内容等の本文。
    /// oklch(0.84 0.008 275)
    pub text_body: Color,
    /// ミュートテキスト。非アクティブタブ名等のセカンダリ情報。
    /// oklch(0.55 0.012 275)
    pub text_muted: Color,
    /// 薄いテキスト。行番号等のターシャリ情報。
    /// oklch(0.40 0.015 275)
    pub text_dim: Color,
    /// アクセントカラー上のテキスト。モードインジケータ等。
    /// oklch(0.17 0.010 270)
    pub text_on_accent: Color,
    /// プレースホルダーテキスト。リネーム入力のヒント等。
    /// oklch(0.63 0.025 85)
    pub text_placeholder: Color,

    // --- Border（区切り線） ---
    /// ペイン分割面・タブ間のセパレータ。
    /// oklch(0.33 0.012 275)
    pub border_subtle: Color,
    /// フォーカス中ペインに隣接するセパレータ。
    /// oklch(0.68 0.100 250)
    pub border_focus: Color,

    // --- Accent（役割ベースのアクセントカラー） ---
    /// プライマリアクセント。オーバーレイ枠、Normalモード、フォーカス表示。
    /// oklch(0.68 0.100 250)
    pub accent_primary: Color,
    /// ポジティブアクセント。Insertモード、選択マーカー。
    /// oklch(0.78 0.150 150)
    pub accent_positive: Color,
    /// 警告アクセント。Selectモード、リネーム、削除確認、Working表示。
    /// oklch(0.76 0.100 75)
    pub accent_warning: Color,
    /// エラーアクセント。エラーメッセージ表示。
    /// oklch(0.65 0.130 25)
    pub accent_error: Color,
}

impl Theme {
    /// デフォルトテーマ（ダーク）。
    pub const DEFAULT: Theme = Theme {
        surface: Color::Rgb(25, 25, 35),
        surface_active: Color::Rgb(50, 50, 70),
        surface_overlay: Color::Rgb(15, 15, 25),
        surface_highlight: Color::Rgb(30, 30, 30),
        surface_selection: Color::Rgb(40, 60, 100),

        text_primary: Color::Rgb(220, 220, 240),
        text_body: Color::Rgb(200, 200, 210),
        text_muted: Color::Rgb(120, 120, 140),
        text_dim: Color::Rgb(80, 80, 100),
        text_on_accent: Color::Rgb(30, 30, 40),
        text_placeholder: Color::Rgb(160, 150, 100),

        border_subtle: Color::Rgb(60, 60, 80),
        border_focus: Color::Rgb(100, 160, 230),

        accent_primary: Color::Rgb(100, 160, 230),
        accent_positive: Color::Rgb(120, 210, 120),
        accent_warning: Color::Rgb(220, 170, 90),
        accent_error: Color::Rgb(225, 110, 110),
    };
}

impl Default for Theme {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// 現在適用中のテーマ。`set()`による差し替えを想定してRwLockで保持する。
static THEME: RwLock<Theme> = RwLock::new(Theme::DEFAULT);

/// 現在のテーマを返す。
/// 描画コードはフレーム毎にこの関数経由でトークンを参照する。
///
/// @returns 現在適用中のテーマのコピー
pub fn get() -> Theme {
    *THEME
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// テーマを差し替える。
/// Lua APIからのカラースキーム変更のエントリポイントとして使用する。
///
/// @param theme - 適用するテーマ
pub fn set(theme: Theme) {
    *THEME
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = theme;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_matches_const() {
        assert_eq!(Theme::default(), Theme::DEFAULT);
    }

    #[test]
    fn get_returns_current_theme() {
        let t = get();
        assert_eq!(t.surface, Color::Rgb(25, 25, 35));
    }

    #[test]
    fn set_replaces_theme() {
        // グローバル状態を変更するため、テスト終了時に必ずデフォルトへ戻す
        let mut custom = Theme::DEFAULT;
        custom.accent_primary = Color::Rgb(1, 2, 3);
        set(custom);
        assert_eq!(get().accent_primary, Color::Rgb(1, 2, 3));
        set(Theme::DEFAULT);
        assert_eq!(get(), Theme::DEFAULT);
    }
}
