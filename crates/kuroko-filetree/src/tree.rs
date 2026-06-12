//! ファイルシステムのツリー構造を表現するデータ型。
//! 遅延読み込みでサブディレクトリをオンデマンドに展開する。

use std::path::{Path, PathBuf};

/// ファイルツリーのエントリ。ファイルまたはディレクトリを表す。
#[derive(Debug, Clone)]
pub struct TreeEntry {
    /// ファイル/ディレクトリのパス
    pub path: PathBuf,
    /// ファイル名（表示用）
    pub name: String,
    /// ディレクトリかどうか
    pub is_dir: bool,
    /// ツリーの深さ（インデント用）
    pub depth: usize,
    /// ディレクトリの場合、展開されているかどうか
    pub expanded: bool,
    /// 子エントリ（遅延読み込み）
    pub children: Vec<TreeEntry>,
    /// 子エントリが読み込み済みかどうか
    pub loaded: bool,
}

impl TreeEntry {
    /// 指定パスからルートエントリを作成する。
    ///
    /// @param path - ルートディレクトリのパス
    /// @param show_hidden - trueなら隠しファイルも表示する
    /// @returns TreeEntryインスタンス
    pub fn new_root(path: &Path, show_hidden: bool) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let mut entry = Self {
            path: path.to_path_buf(),
            name,
            is_dir: true,
            depth: 0,
            expanded: true,
            children: Vec::new(),
            loaded: false,
        };
        entry.load_children(show_hidden);
        entry
    }

    /// 子エントリを読み込む（gitignore対応）。
    ///
    /// @param show_hidden - trueなら隠しファイルも表示する
    pub fn load_children(&mut self, show_hidden: bool) {
        if !self.is_dir || self.loaded {
            return;
        }
        self.loaded = true;

        let walker = ignore::WalkBuilder::new(&self.path)
            .max_depth(Some(1))
            .hidden(!show_hidden)
            .sort_by_file_name(|a, b| {
                // ディレクトリを先に、ファイルを後にソート
                let a_is_dir = a.to_str().map(|s| Path::new(s).is_dir()).unwrap_or(false);
                let b_is_dir = b.to_str().map(|s| Path::new(s).is_dir()).unwrap_or(false);
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.cmp(b),
                }
            })
            .build();

        for entry in walker.flatten() {
            let path = entry.path().to_path_buf();
            if path == self.path {
                continue; // ルート自身をスキップ
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let is_dir = path.is_dir();

            self.children.push(TreeEntry {
                path,
                name,
                is_dir,
                depth: self.depth + 1,
                expanded: false,
                children: Vec::new(),
                loaded: false,
            });
        }
    }

    /// ツリーをフラットなリストに展開する（表示用）。
    ///
    /// @returns 表示順の(深さ, 名前, パス, ディレクトリか, 展開済みか)のリスト
    pub fn flatten(&self) -> Vec<FlatEntry> {
        let mut result = Vec::new();
        self.flatten_inner(&mut result);
        result
    }

    /// flattenの再帰実装
    fn flatten_inner(&self, result: &mut Vec<FlatEntry>) {
        result.push(FlatEntry {
            depth: self.depth,
            name: self.name.clone(),
            path: self.path.clone(),
            is_dir: self.is_dir,
            expanded: self.expanded,
        });
        if self.expanded {
            for child in &self.children {
                child.flatten_inner(result);
            }
        }
    }

    /// インデックスで指定されたエントリのexpand/collapseを切り替える。
    /// フラットリストのインデックスからツリー内の対応するノードを操作する。
    ///
    /// @param flat_index - フラットリスト上のインデックス
    /// @param show_hidden - trueなら隠しファイルも表示する
    pub fn toggle(&mut self, flat_index: usize, show_hidden: bool) {
        let mut counter = 0;
        self.toggle_inner(flat_index, &mut counter, show_hidden);
    }

    /// toggleの再帰実装
    fn toggle_inner(&mut self, target: usize, counter: &mut usize, show_hidden: bool) -> bool {
        if *counter == target {
            if self.is_dir {
                self.expanded = !self.expanded;
                if self.expanded && !self.loaded {
                    self.load_children(show_hidden);
                }
            }
            return true;
        }
        *counter += 1;
        if self.expanded {
            for child in &mut self.children {
                if child.toggle_inner(target, counter, show_hidden) {
                    return true;
                }
            }
        }
        false
    }

    /// 子エントリを強制的に再読み込みする。
    /// 展開状態を維持したまま、ディスク上の最新状態を反映する。
    ///
    /// @param show_hidden - trueなら隠しファイルも表示する
    pub fn reload_children(&mut self, show_hidden: bool) {
        if !self.is_dir {
            return;
        }
        // 展開中だったサブディレクトリのパスを記録
        let expanded_dirs: Vec<PathBuf> = self
            .children
            .iter()
            .filter(|c| c.is_dir && c.expanded)
            .map(|c| c.path.clone())
            .collect();

        self.children.clear();
        self.loaded = false;
        self.load_children(show_hidden);

        // 以前展開されていたサブディレクトリを再展開
        for child in &mut self.children {
            if expanded_dirs.contains(&child.path) {
                child.expanded = true;
                child.load_children(show_hidden);
            }
        }
    }

    /// 指定パスを含むサブツリーをリフレッシュする。
    /// ルートから対象パスの親ディレクトリまで辿り、そのディレクトリの子を再読み込みする。
    ///
    /// @param target - リフレッシュ対象のパス（ファイルまたはディレクトリ）
    /// @param show_hidden - trueなら隠しファイルも表示する
    pub fn refresh_subtree(&mut self, target: &Path, show_hidden: bool) {
        // 対象がこのエントリの直接の子の場合
        if target.parent() == Some(&self.path) {
            self.reload_children(show_hidden);
            return;
        }
        // 対象がこのエントリのサブツリー内にある場合、再帰的に探索
        for child in &mut self.children {
            if child.is_dir && target.starts_with(&child.path) {
                child.refresh_subtree(target, show_hidden);
                return;
            }
        }
        // 見つからなければルートを再読み込み
        self.reload_children(show_hidden);
    }
}

/// フラット化されたツリーエントリ（表示用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatEntry {
    /// インデントの深さ
    pub depth: usize,
    /// 表示名
    pub name: String,
    /// ファイルパス
    pub path: PathBuf,
    /// ディレクトリかどうか
    pub is_dir: bool,
    /// ディレクトリの場合、展開済みかどうか
    pub expanded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用にファイルシステムに依存しないTreeEntryを手動構築するヘルパー
    fn make_entry(
        name: &str,
        is_dir: bool,
        depth: usize,
        expanded: bool,
        children: Vec<TreeEntry>,
    ) -> TreeEntry {
        TreeEntry {
            path: PathBuf::from(format!("/test/{}", name)),
            name: name.to_string(),
            is_dir,
            depth,
            expanded,
            children,
            loaded: true,
        }
    }

    fn sample_tree() -> TreeEntry {
        // root/
        //   src/        (expanded)
        //     main.rs
        //     lib.rs
        //   tests/      (collapsed)
        //     test1.rs
        //   README.md
        make_entry(
            "root",
            true,
            0,
            true,
            vec![
                make_entry(
                    "src",
                    true,
                    1,
                    true,
                    vec![
                        make_entry("main.rs", false, 2, false, vec![]),
                        make_entry("lib.rs", false, 2, false, vec![]),
                    ],
                ),
                make_entry(
                    "tests",
                    true,
                    1,
                    false,
                    vec![make_entry("test1.rs", false, 2, false, vec![])],
                ),
                make_entry("README.md", false, 1, false, vec![]),
            ],
        )
    }

    // --- flatten ---

    #[test]
    fn flatten_shows_expanded_children() {
        let tree = sample_tree();
        let flat = tree.flatten();
        let names: Vec<&str> = flat.iter().map(|e| e.name.as_str()).collect();
        // src is expanded so its children are visible
        // tests is collapsed so test1.rs is NOT visible
        assert_eq!(
            names,
            vec!["root", "src", "main.rs", "lib.rs", "tests", "README.md"]
        );
    }

    #[test]
    fn flatten_preserves_depth() {
        let tree = sample_tree();
        let flat = tree.flatten();
        let depths: Vec<usize> = flat.iter().map(|e| e.depth).collect();
        assert_eq!(depths, vec![0, 1, 2, 2, 1, 1]);
    }

    #[test]
    fn flatten_empty_tree() {
        let tree = make_entry("empty", true, 0, true, vec![]);
        let flat = tree.flatten();
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].name, "empty");
    }

    #[test]
    fn flatten_collapsed_root_shows_only_root() {
        let mut tree = sample_tree();
        tree.expanded = false;
        let flat = tree.flatten();
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].name, "root");
    }

    // --- toggle ---

    #[test]
    fn toggle_collapses_expanded_dir() {
        let mut tree = sample_tree();
        // flat index 1 = "src" (expanded)
        tree.toggle(1, false);
        let flat = tree.flatten();
        let names: Vec<&str> = flat.iter().map(|e| e.name.as_str()).collect();
        // src is now collapsed, main.rs and lib.rs are hidden
        assert_eq!(names, vec!["root", "src", "tests", "README.md"]);
    }

    #[test]
    fn toggle_expands_collapsed_dir() {
        let mut tree = sample_tree();
        // flat index 4 = "tests" (collapsed)
        tree.toggle(4, false);
        let flat = tree.flatten();
        let names: Vec<&str> = flat.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "root",
                "src",
                "main.rs",
                "lib.rs",
                "tests",
                "test1.rs",
                "README.md"
            ]
        );
    }

    #[test]
    fn toggle_file_is_noop() {
        let mut tree = sample_tree();
        let before = tree.flatten();
        // flat index 2 = "main.rs" (file)
        tree.toggle(2, false);
        let after = tree.flatten();
        assert_eq!(before.len(), after.len());
    }

    // --- flatten + index access ---

    #[test]
    fn flatten_returns_correct_paths() {
        let tree = sample_tree();
        let flat = tree.flatten();
        assert_eq!(flat[0].path, PathBuf::from("/test/root"));
        assert_eq!(flat[2].path, PathBuf::from("/test/main.rs"));
    }

    #[test]
    fn flatten_out_of_bounds_returns_none() {
        let tree = sample_tree();
        let flat = tree.flatten();
        assert!(flat.get(100).is_none());
    }

    // --- new_root (filesystem-dependent, tests existence) ---

    #[test]
    fn new_root_creates_expanded_entry() {
        // /tmp は常に存在する
        let entry = TreeEntry::new_root(Path::new("/tmp"), false);
        assert!(entry.is_dir);
        assert!(entry.expanded);
        assert!(entry.loaded);
        assert_eq!(entry.depth, 0);
    }
}
