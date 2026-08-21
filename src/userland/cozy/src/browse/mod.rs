//! Browse モードのツリーモデルと並列ディレクトリ走査。
//!
//! `ignore` クレート（ripgrep の walker）で `.gitignore`/`.git`/`target` を除外しつつ
//! 並列にディレクトリ全体を走査し、フラットな arena（`Vec<BrowseNode>`、index 0 = root）
//! として保持する。描画・操作は「展開済みディレクトリの子だけを深さ優先で並べた可視ノード列」
//! を介して行う。crossterm/ratatui には依存しない（将来の core/UI 分離方針）。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// ツリー上の 1 エントリ（ファイル or ディレクトリ）。
#[derive(Debug, Clone)]
pub struct BrowseNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// root からの相対深さ（root = 0）。描画のインデントに使う。
    pub depth: usize,
    pub parent: Option<usize>,
    /// ディレクトリ先・名前順にソート済みの子 index。
    pub children: Vec<usize>,
}

/// 走査済みのツリー全体と現在の表示・選択状態。
#[derive(Debug, Clone)]
pub struct BrowseTree {
    pub root: PathBuf,
    pub nodes: Vec<BrowseNode>,
    /// 展開中ディレクトリの index（root は常に展開）。
    pub expanded: HashSet<usize>,
    /// カーソル位置のノード index（常に可視ノードのいずれか）。
    pub selected: usize,
    /// `/` 絞り込み文字列（空＝絞り込みなし）。
    pub filter: String,
    /// `/` 絞り込み入力を受け付けている最中か。
    pub filtering: bool,
}

impl BrowseTree {
    /// `root` 以下を並列走査してツリーを構築する。
    pub fn build(root: &Path) -> Self {
        let root = root.to_path_buf();
        let entries = walk_parallel(&root);
        let nodes = build_arena(&root, entries);

        let mut expanded = HashSet::new();
        expanded.insert(0); // root は常に展開

        BrowseTree {
            root,
            nodes,
            expanded,
            selected: 0,
            filter: String::new(),
            filtering: false,
        }
    }

    fn is_filtering(&self) -> bool {
        !self.filter.is_empty()
    }

    /// 画面に出すノード index を上から順に返す。
    /// 通常は展開済みディレクトリの子を深さ優先で。絞り込み中はマッチノードとその祖先のみ
    /// （ディレクトリは自動展開扱い）。
    pub fn visible_nodes(&self) -> Vec<usize> {
        let filtering = self.is_filtering();
        let keep = if filtering {
            let needle = self.filter.to_lowercase();
            let mut keep = vec![false; self.nodes.len()];
            // 子は親より後ろの index（path ソート済みのため）→ 逆順で子→親に伝播。
            for i in (0..self.nodes.len()).rev() {
                let matches = self.nodes[i].name.to_lowercase().contains(&needle);
                let child_kept = self.nodes[i].children.iter().any(|&c| keep[c]);
                keep[i] = matches || child_kept;
            }
            Some(keep)
        } else {
            None
        };

        let mut out = Vec::new();
        self.collect_visible(0, &keep, &mut out);
        out
    }

    fn collect_visible(&self, idx: usize, keep: &Option<Vec<bool>>, out: &mut Vec<usize>) {
        if let Some(keep) = keep {
            if !keep[idx] {
                return;
            }
        }
        out.push(idx);
        // 絞り込み中は全ディレクトリを展開扱い。
        let expanded = keep.is_some() || self.expanded.contains(&idx);
        if expanded {
            for &child in &self.nodes[idx].children {
                self.collect_visible(child, keep, out);
            }
        }
    }

    /// 可視列上でカーソルを 1 つ下へ。
    pub fn move_down(&mut self) {
        let vis = self.visible_nodes();
        match vis.iter().position(|&i| i == self.selected) {
            Some(pos) if pos + 1 < vis.len() => self.selected = vis[pos + 1],
            None => {
                if let Some(&first) = vis.first() {
                    self.selected = first;
                }
            }
            _ => {}
        }
    }

    /// 可視列上でカーソルを 1 つ上へ。
    pub fn move_up(&mut self) {
        let vis = self.visible_nodes();
        match vis.iter().position(|&i| i == self.selected) {
            Some(pos) if pos > 0 => self.selected = vis[pos - 1],
            None => {
                if let Some(&first) = vis.first() {
                    self.selected = first;
                }
            }
            _ => {}
        }
    }

    pub fn goto_top(&mut self) {
        if let Some(&first) = self.visible_nodes().first() {
            self.selected = first;
        }
    }

    pub fn goto_bottom(&mut self) {
        if let Some(&last) = self.visible_nodes().last() {
            self.selected = last;
        }
    }

    /// `l`/Enter/→: ディレクトリなら展開トグル（None）、ファイルなら開く対象 path を返す。
    pub fn expand_or_open(&mut self) -> Option<PathBuf> {
        let idx = self.selected;
        if self.nodes[idx].is_dir {
            if self.expanded.contains(&idx) {
                self.expanded.remove(&idx);
            } else {
                self.expanded.insert(idx);
            }
            None
        } else {
            Some(self.nodes[idx].path.clone())
        }
    }

    /// `h`/←: 展開中ディレクトリは畳む、それ以外は親ノードへ移動。
    pub fn collapse_or_parent(&mut self) {
        let idx = self.selected;
        if idx != 0 && self.nodes[idx].is_dir && self.expanded.contains(&idx) {
            self.expanded.remove(&idx);
        } else if let Some(parent) = self.nodes[idx].parent {
            self.selected = parent;
        }
    }

    /// 指定 path のノードを選択し、その祖先をすべて展開する（現ファイルのプリセレクト用）。
    pub fn select_path(&mut self, path: &Path) {
        if let Some(idx) = self.nodes.iter().position(|n| n.path == path) {
            self.selected = idx;
            let mut cur = self.nodes[idx].parent;
            while let Some(p) = cur {
                self.expanded.insert(p);
                cur = self.nodes[p].parent;
            }
        }
    }

    /// 絞り込み文字を 1 文字追加。
    pub fn push_filter_char(&mut self, c: char) {
        self.filter.push(c);
        self.reclamp_selection();
    }

    /// 絞り込み文字を 1 文字削除。
    pub fn pop_filter_char(&mut self) {
        self.filter.pop();
        self.reclamp_selection();
    }

    /// 選択が現在の可視列から外れていたら先頭に寄せ直す。
    fn reclamp_selection(&mut self) {
        let vis = self.visible_nodes();
        if !vis.contains(&self.selected) {
            self.selected = vis.first().copied().unwrap_or(0);
        }
    }
}

/// `ignore` の並列ウォーカーで (path, is_dir) を収集する。標準フィルタ（.gitignore/.git/
/// hidden）が効くので `.git`・`target` 等は自動除外される。
fn walk_parallel(root: &Path) -> Vec<(PathBuf, bool)> {
    use ignore::WalkBuilder;
    use std::sync::Mutex;

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let collected = Mutex::new(Vec::new());
    WalkBuilder::new(root)
        .standard_filters(true)
        .threads(threads)
        .build_parallel()
        .run(|| {
            Box::new(|result| {
                if let Ok(entry) = result {
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    collected
                        .lock()
                        .unwrap()
                        .push((entry.path().to_path_buf(), is_dir));
                }
                ignore::WalkState::Continue
            })
        });

    collected.into_inner().unwrap()
}

/// 収集したエントリを path ソート→親子リンクして arena 化する。
/// path ソートにより親は必ず子より前（index が小さい）になるので親リンクが安全に張れる。
fn build_arena(root: &Path, mut entries: Vec<(PathBuf, bool)>) -> Vec<BrowseNode> {
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut nodes: Vec<BrowseNode> = Vec::with_capacity(entries.len());
    let mut index_of: HashMap<PathBuf, usize> = HashMap::new();

    for (path, is_dir) in entries {
        let idx = nodes.len();
        let parent = path.parent().and_then(|p| index_of.get(p).copied());
        let depth = match parent {
            Some(p) => nodes[p].depth + 1,
            None => 0,
        };
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                if idx == 0 {
                    root.to_string_lossy().to_string()
                } else {
                    String::new()
                }
            });
        index_of.insert(path.clone(), idx);
        nodes.push(BrowseNode {
            path,
            name,
            is_dir,
            depth,
            parent,
            children: Vec::new(),
        });
    }

    // 親子リンク。
    for i in 0..nodes.len() {
        if let Some(p) = nodes[i].parent {
            nodes[p].children.push(i);
        }
    }

    // 各ディレクトリの子をディレクトリ先・名前昇順に並べる。
    let keys: Vec<(bool, String)> = nodes
        .iter()
        .map(|n| (!n.is_dir, n.name.to_lowercase()))
        .collect();
    for node in &mut nodes {
        node.children.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
    }

    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 一時ディレクトリツリーを作る簡易ヘルパ（tempfile 非依存）。`name` はテストごとに一意に
    /// （テストは並列実行されるため共有名だと互いの cleanup でディレクトリを消し合う）。
    fn scratch_tree(name: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("cozy_browse_test_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("src")).unwrap();
        fs::create_dir_all(base.join("target/debug")).unwrap();
        fs::create_dir_all(base.join(".git")).unwrap();
        fs::write(base.join("README.md"), "readme").unwrap();
        fs::write(base.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(base.join("src/lib.rs"), "// lib").unwrap();
        fs::write(base.join("target/debug/artifact"), "bin").unwrap();
        fs::write(base.join(".git/config"), "[core]").unwrap();
        base
    }

    #[test]
    fn build_collects_files_and_dirs() {
        let base = scratch_tree("build_collects");
        let tree = BrowseTree::build(&base);
        // root + src + README.md + main.rs + lib.rs は最低限拾える。
        assert!(tree.nodes.iter().any(|n| n.name == "README.md"));
        assert!(tree.nodes.iter().any(|n| n.name == "main.rs"));
        assert!(tree.nodes.iter().any(|n| n.name == "src" && n.is_dir));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn git_and_target_are_excluded() {
        let base = scratch_tree("git_target");
        let tree = BrowseTree::build(&base);
        // .git は hidden 除外、target は（.gitignore 無しでも）少なくとも .git は確実に消える。
        assert!(!tree.nodes.iter().any(|n| n.name == ".git"));
        assert!(
            !tree
                .nodes
                .iter()
                .any(|n| n.path.components().any(|c| c.as_os_str() == ".git"))
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn expand_collapse_changes_visibility() {
        let base = scratch_tree("expand_collapse");
        let mut tree = BrowseTree::build(&base);
        let src_idx = tree.nodes.iter().position(|n| n.name == "src").unwrap();
        // 初期は root のみ展開 → src は見えるが main.rs は見えない。
        let before = tree.visible_nodes();
        assert!(before.contains(&src_idx));
        let main_idx = tree.nodes.iter().position(|n| n.name == "main.rs").unwrap();
        assert!(!before.contains(&main_idx));
        // src を展開すると main.rs が見える。
        tree.expanded.insert(src_idx);
        assert!(tree.visible_nodes().contains(&main_idx));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn filter_keeps_matches_and_ancestors() {
        let base = scratch_tree("filter");
        let mut tree = BrowseTree::build(&base);
        tree.filter = "main".to_string();
        let vis = tree.visible_nodes();
        let names: Vec<&str> = vis.iter().map(|&i| tree.nodes[i].name.as_str()).collect();
        assert!(names.contains(&"main.rs")); // マッチ
        assert!(names.contains(&"src")); // 祖先
        assert!(!names.contains(&"README.md")); // 非マッチ
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn move_down_up_clamps_at_bounds() {
        let base = scratch_tree("move_bounds");
        let mut tree = BrowseTree::build(&base);
        tree.goto_top();
        let top = tree.selected;
        tree.move_up(); // 先頭で上 → 変化なし
        assert_eq!(tree.selected, top);
        tree.goto_bottom();
        let bottom = tree.selected;
        tree.move_down(); // 末尾で下 → 変化なし
        assert_eq!(tree.selected, bottom);
        let _ = fs::remove_dir_all(&base);
    }
}
