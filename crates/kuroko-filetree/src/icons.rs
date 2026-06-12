//! ファイルタイプに応じたアイコン文字とカラーのマッピング。
//! Nerd Font v3互換のUnicode文字を使用し、ターミナル上でファイル種別を視覚的に識別可能にする。

use ratatui::style::Color;

/// ファイルのアイコン情報（文字とカラー）
pub struct FileIcon {
    /// アイコン文字（Nerd Font）
    pub icon: &'static str,
    /// アイコンの表示色
    pub color: Color,
}

/// ファイル名・種別からアイコン情報を返す。
///
/// @param name - ファイル名（拡張子を含む）
/// @param is_dir - ディレクトリかどうか
/// @param expanded - ディレクトリの場合、展開中かどうか
/// @returns アイコン文字とカラー
pub fn file_icon(name: &str, is_dir: bool, expanded: bool) -> FileIcon {
    if is_dir {
        return if expanded {
            FileIcon {
                icon: "\u{f07c}",
                color: Color::Rgb(229, 165, 68),
            }
        } else {
            FileIcon {
                icon: "\u{f07b}",
                color: Color::Rgb(229, 165, 68),
            }
        };
    }
    icon_for_file(name)
}

/// ファイル名から拡張子・完全一致でアイコンを決定する。
///
/// @param name - ファイル名
/// @returns アイコン文字とカラー
fn icon_for_file(name: &str) -> FileIcon {
    // 特定ファイル名の完全一致（大文字小文字を無視）
    let lower = name.to_lowercase();
    match lower.as_str() {
        "cargo.toml" | "cargo.lock" => {
            return FileIcon {
                icon: "\u{e7a8}",
                color: Color::Rgb(222, 120, 53),
            };
        }
        "dockerfile" | "containerfile" => {
            return FileIcon {
                icon: "\u{e7b0}",
                color: Color::Rgb(56, 143, 205),
            };
        }
        "docker-compose.yml" | "docker-compose.yaml" | "compose.yml" | "compose.yaml" => {
            return FileIcon {
                icon: "\u{e7b0}",
                color: Color::Rgb(56, 143, 205),
            };
        }
        ".gitignore" | ".gitmodules" | ".gitattributes" | ".gitkeep" => {
            return FileIcon {
                icon: "\u{e702}",
                color: Color::Rgb(222, 79, 54),
            };
        }
        "license" | "license.md" | "license.txt" | "licence" | "licence.md" => {
            return FileIcon {
                icon: "\u{f0219}",
                color: Color::Rgb(185, 155, 55),
            };
        }
        "makefile" | "justfile" => {
            return FileIcon {
                icon: "\u{e615}",
                color: Color::Rgb(111, 155, 98),
            };
        }
        "readme.md" | "readme" | "readme.txt" => {
            return FileIcon {
                icon: "\u{e73e}",
                color: Color::Rgb(66, 165, 245),
            };
        }
        ".editorconfig" | ".prettierrc" | ".eslintrc" => {
            return FileIcon {
                icon: "\u{e615}",
                color: Color::Rgb(155, 135, 105),
            };
        }
        _ => {}
    }

    // 拡張子で判定
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        // Rust
        "rs" => FileIcon {
            icon: "\u{e7a8}",
            color: Color::Rgb(222, 120, 53),
        },

        // JavaScript / TypeScript
        "js" | "mjs" | "cjs" => FileIcon {
            icon: "\u{e74e}",
            color: Color::Rgb(229, 214, 80),
        },
        "ts" | "mts" | "cts" => FileIcon {
            icon: "\u{e628}",
            color: Color::Rgb(49, 120, 198),
        },
        "jsx" => FileIcon {
            icon: "\u{e7ba}",
            color: Color::Rgb(97, 218, 251),
        },
        "tsx" => FileIcon {
            icon: "\u{e7ba}",
            color: Color::Rgb(49, 120, 198),
        },

        // Web
        "html" | "htm" => FileIcon {
            icon: "\u{e736}",
            color: Color::Rgb(228, 79, 38),
        },
        "css" => FileIcon {
            icon: "\u{e749}",
            color: Color::Rgb(86, 156, 214),
        },
        "scss" | "sass" => FileIcon {
            icon: "\u{e749}",
            color: Color::Rgb(205, 103, 153),
        },
        "vue" => FileIcon {
            icon: "\u{e6a0}",
            color: Color::Rgb(65, 184, 131),
        },
        "svelte" => FileIcon {
            icon: "\u{e697}",
            color: Color::Rgb(255, 62, 0),
        },

        // スクリプト言語
        "py" => FileIcon {
            icon: "\u{e73c}",
            color: Color::Rgb(55, 118, 171),
        },
        "rb" => FileIcon {
            icon: "\u{e791}",
            color: Color::Rgb(204, 52, 45),
        },
        "lua" => FileIcon {
            icon: "\u{e620}",
            color: Color::Rgb(66, 135, 245),
        },
        "php" => FileIcon {
            icon: "\u{e73d}",
            color: Color::Rgb(119, 123, 180),
        },
        "pl" | "pm" => FileIcon {
            icon: "\u{e769}",
            color: Color::Rgb(57, 69, 124),
        },

        // コンパイル言語
        "go" => FileIcon {
            icon: "\u{e626}",
            color: Color::Rgb(0, 173, 216),
        },
        "java" => FileIcon {
            icon: "\u{e738}",
            color: Color::Rgb(204, 62, 68),
        },
        "kt" | "kts" => FileIcon {
            icon: "\u{e634}",
            color: Color::Rgb(129, 104, 198),
        },
        "swift" => FileIcon {
            icon: "\u{e755}",
            color: Color::Rgb(240, 81, 56),
        },
        "c" => FileIcon {
            icon: "\u{e61e}",
            color: Color::Rgb(85, 141, 205),
        },
        "cpp" | "cc" | "cxx" => FileIcon {
            icon: "\u{e61d}",
            color: Color::Rgb(85, 141, 205),
        },
        "h" | "hpp" | "hxx" => FileIcon {
            icon: "\u{e61e}",
            color: Color::Rgb(121, 94, 163),
        },
        "cs" => FileIcon {
            icon: "\u{f031b}",
            color: Color::Rgb(86, 156, 214),
        },
        "zig" => FileIcon {
            icon: "\u{e6a9}",
            color: Color::Rgb(236, 145, 27),
        },

        // データ・設定
        "json" | "jsonc" => FileIcon {
            icon: "\u{e60b}",
            color: Color::Rgb(229, 214, 80),
        },
        "toml" => FileIcon {
            icon: "\u{e6b2}",
            color: Color::Rgb(155, 135, 105),
        },
        "yaml" | "yml" => FileIcon {
            icon: "\u{e6a8}",
            color: Color::Rgb(155, 89, 182),
        },
        "xml" => FileIcon {
            icon: "\u{e619}",
            color: Color::Rgb(228, 79, 38),
        },
        "csv" => FileIcon {
            icon: "\u{f1c3}",
            color: Color::Rgb(86, 156, 76),
        },
        "sql" => FileIcon {
            icon: "\u{e706}",
            color: Color::Rgb(218, 165, 32),
        },
        "graphql" | "gql" => FileIcon {
            icon: "\u{e662}",
            color: Color::Rgb(229, 53, 171),
        },
        "proto" => FileIcon {
            icon: "\u{e6b1}",
            color: Color::Rgb(130, 130, 130),
        },

        // シェル
        "sh" | "bash" | "zsh" | "fish" => FileIcon {
            icon: "\u{e795}",
            color: Color::Rgb(111, 155, 98),
        },

        // ドキュメント
        "md" | "mdx" => FileIcon {
            icon: "\u{e73e}",
            color: Color::Rgb(66, 165, 245),
        },
        "txt" => FileIcon {
            icon: "\u{f15c}",
            color: Color::Rgb(175, 175, 175),
        },
        "pdf" => FileIcon {
            icon: "\u{f1c1}",
            color: Color::Rgb(210, 70, 50),
        },
        "doc" | "docx" => FileIcon {
            icon: "\u{f1c2}",
            color: Color::Rgb(52, 101, 175),
        },

        // 画像
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "tiff" => FileIcon {
            icon: "\u{f1c5}",
            color: Color::Rgb(165, 121, 214),
        },
        "svg" => FileIcon {
            icon: "\u{f1c5}",
            color: Color::Rgb(255, 181, 62),
        },

        // フォント
        "ttf" | "otf" | "woff" | "woff2" => FileIcon {
            icon: "\u{f031}",
            color: Color::Rgb(175, 175, 175),
        },

        // アーカイブ
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => FileIcon {
            icon: "\u{f1c6}",
            color: Color::Rgb(175, 135, 95),
        },

        // バイナリ・実行可能
        "wasm" => FileIcon {
            icon: "\u{e6a1}",
            color: Color::Rgb(101, 79, 240),
        },
        "exe" | "dll" | "so" | "dylib" => FileIcon {
            icon: "\u{f013}",
            color: Color::Rgb(130, 130, 130),
        },

        // ロック・環境
        "lock" => FileIcon {
            icon: "\u{f023}",
            color: Color::Rgb(130, 130, 130),
        },
        "env" => FileIcon {
            icon: "\u{f462}",
            color: Color::Rgb(250, 200, 50),
        },
        "log" => FileIcon {
            icon: "\u{f18d}",
            color: Color::Rgb(130, 130, 130),
        },

        // Git関連
        "diff" | "patch" => FileIcon {
            icon: "\u{e702}",
            color: Color::Rgb(222, 79, 54),
        },

        // デフォルト
        _ => FileIcon {
            icon: "\u{f15b}",
            color: Color::Rgb(155, 155, 155),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_icons_differ_by_expanded_state() {
        let collapsed = file_icon("src", true, false);
        let expanded = file_icon("src", true, true);
        assert_ne!(collapsed.icon, expanded.icon);
        // 色は同じ（ディレクトリのアイコン色は統一）
        assert_eq!(collapsed.color, expanded.color);
    }

    #[test]
    fn rust_file_icon() {
        let icon = file_icon("main.rs", false, false);
        assert_eq!(icon.icon, "\u{e7a8}");
    }

    #[test]
    fn cargo_toml_exact_match() {
        let icon = file_icon("Cargo.toml", false, false);
        assert_eq!(icon.icon, "\u{e7a8}");
        assert_eq!(icon.color, Color::Rgb(222, 120, 53));
    }

    #[test]
    fn dockerfile_match() {
        let icon = file_icon("Dockerfile", false, false);
        assert_eq!(icon.icon, "\u{e7b0}");
    }

    #[test]
    fn typescript_extension() {
        let icon = file_icon("app.ts", false, false);
        assert_eq!(icon.icon, "\u{e628}");
    }

    #[test]
    fn tsx_extension() {
        let icon = file_icon("component.tsx", false, false);
        assert_eq!(icon.icon, "\u{e7ba}");
    }

    #[test]
    fn python_extension() {
        let icon = file_icon("script.py", false, false);
        assert_eq!(icon.icon, "\u{e73c}");
    }

    #[test]
    fn unknown_extension_gets_default() {
        let icon = file_icon("data.xyz123", false, false);
        assert_eq!(icon.icon, "\u{f15b}");
        assert_eq!(icon.color, Color::Rgb(155, 155, 155));
    }

    #[test]
    fn gitignore_exact_match() {
        let icon = file_icon(".gitignore", false, false);
        assert_eq!(icon.icon, "\u{e702}");
    }

    #[test]
    fn case_insensitive_exact_match() {
        // "CARGO.TOML" should match "cargo.toml"
        let icon = file_icon("CARGO.TOML", false, false);
        assert_eq!(icon.icon, "\u{e7a8}");
    }

    #[test]
    fn shell_script_extensions() {
        for ext in &["script.sh", "setup.bash", "init.zsh", "config.fish"] {
            let icon = file_icon(ext, false, false);
            assert_eq!(icon.icon, "\u{e795}", "failed for {}", ext);
        }
    }

    #[test]
    fn markdown_extension() {
        let icon = file_icon("README.md", false, false);
        // README.md は完全一致ルールが先にヒットする
        assert_eq!(icon.icon, "\u{e73e}");
    }

    #[test]
    fn json_extension() {
        let icon = file_icon("package.json", false, false);
        assert_eq!(icon.icon, "\u{e60b}");
    }
}
