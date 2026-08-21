use crate::browse::BrowseTree;
use crate::state::{Cursor, EditorState, TextBuffer};
use atomicwrites::{AllowOverwrite, AtomicFile};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

pub(crate) enum StartupDocument {
    Empty,
    File { path: PathBuf, lines: Vec<String> },
    Directory { tree: BrowseTree },
}

/// 先頭の `~` をホームへ展開する。
///
/// `$HOME` を先に見て、無ければ `dirs::home_dir()`。⚠️ **Unix ではこの 2 つは一致する**
/// （`dirs` も `$HOME` を読む）ので、順序はバグ対策ではなく **ホストが差し替えた値を明示的に
/// 尊重する**という意思表示。ホストが `HOME` だけ差し替える実装に変わっても追従する。
///
/// なぜ cozy 側で展開するのか: **端末に打つとシェルが `~` を展開してくれるので、通常 cozy は
/// チルダを見ない**。ところが**インプロセスのホスト**（argo が cozy を TUI プロバイダとして
/// 呼ぶ経路）では単語展開を通らず、`~/.ssh/config` が**そのまま**届く。すると
/// `<cwd>/~/.ssh/config` ＝ `~` という名のディレクトリを含む相対パスと解釈され、
/// 親が無いので `Directory not found` で落ちていた（2026-07-30・iOS で実測）。
///
/// 展開するのは **POSIX のチルダ接頭辞**だけ: `~` 単独と `~/…`。`~user` は解決手段が無いので
/// 触らない（そのまま返す）。⚠️ 先頭以外の `~` も触らない —— ファイル名の一部で普通に出る。
fn expand_tilde(path: &str) -> PathBuf {
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(dirs::home_dir)
    };
    match path {
        "~" => home().unwrap_or_else(|| PathBuf::from(path)),
        _ => match path.strip_prefix("~/") {
            // `~/x` → `<home>/x`。home が分からなければ原文のまま（黙って別の場所に
            // 書くより、呼び出し側のエラーとして見えた方が良い）。
            Some(rest) => home().map_or_else(|| PathBuf::from(path), |h| h.join(rest)),
            None => expand_sandbox_root(path),
        },
    }
}

/// ホストがサンドボックス根を宣言しているとき、**論理絶対パス** `/x` を `<root>/x` にする。
///
/// [`shorten_sandbox_root`] の逆で、対で往復する。宣言が無ければ何もしない。
///
/// ⚠️ **既に根の下にあるパスは触らない。** ホスト（argo）は cozy へ渡す前に自分で翻訳して
/// いるので、`<root>/x` がそのまま届く。ここで無条件に前置すると `<root><root>/x` になる。
/// ⚠️ **相対パスも触らない** —— cwd からの解決はプロセスに任せる。
fn expand_sandbox_root(path: &str) -> PathBuf {
    let Some(root) = crate::runtime_env::sandbox_root() else {
        return PathBuf::from(path);
    };
    let p = PathBuf::from(path);
    if !p.is_absolute() || p.starts_with(&root) {
        return p;
    }
    // `/x` → `<root>/x`（先頭の `/` を剥がして join する）
    root.join(path.trim_start_matches('/'))
}

/// [`expand_sandbox_root`] の逆。根の下のパスを `/…` の形にして**見せる**。
///
/// ⚠️ 呼ぶのは [`shorten_home`] が縮められなかったときだけ —— `$HOME` は根の**下**に
/// 在るので（iOS は `<root>/Documents`）、先に `~` を試さないと `~/x` が `/Documents/x`
/// として出てしまう。**より具体的な方を優先する。**
fn shorten_sandbox_root(path: &str) -> Option<String> {
    let root = crate::runtime_env::sandbox_root()?;
    let root = root.to_str()?;
    let root = root.strip_suffix('/').unwrap_or(root);
    if path == root {
        return Some("/".to_string());
    }
    match path.strip_prefix(root) {
        // 区切りが続くときだけ ＝ 根がパス成分として一致したときだけ縮める。
        Some(rest) if rest.starts_with('/') => Some(rest.to_string()),
        _ => None,
    }
}

/// テストが `HOME` を差し替えるときの直列化ロック。
///
/// ⚠️ **モジュールを跨いで共有する。** `HOME` はプロセス全域で、テストハーネスは同じ
/// プロセスの別スレッドで走るので、モジュールごとにロックを持つと**互いに気づかないまま
/// 取り合う**（片方が戻した値をもう片方が読む）。
#[cfg(test)]
pub(crate) static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// [`expand_tilde`] の逆。ホームの下にあるパスを `~/…` の形にして**見せる**。
///
/// なぜ要るのか: cozy は開いたファイルを**解決済みの絶対パスで持っている**ので、保存プロンプトは
/// 利用者が打った `~/notes.md` ではなく `/home/you/notes.md` を出す。desktop では冗長なだけだが、
/// **cozy が電話の中（argo）で動くときは致命的**で、ホストが VFS 翻訳した後の
/// `/data/data/com.hsh.mobile/files/notes.md` のような**コンテナパスがそのまま画面に出る**。
/// hsh 側は「物理パスは利用者が絶対に見てはならない不透明なコンテナパス」と決めているので、
/// そこだけ約束が破れていた（2026-08-04 に実機で指摘された）。
///
/// ⚠️ **境界はパス区切りで見る。** `HOME=/home/al` のとき `/home/alice/x` を
/// `~ice/x` にしてはいけない。
/// ⚠️ **`/` をホームとする環境では縮めない** —— 全部が `~/…` になり、`~` が何も意味しなくなる。
/// ⭐ 戻り値は [`expand_tilde`] が受け付ける形（`~` 単独 / `~/…`）だけ ＝ **往復する**。
/// 画面に出す文字列と、保存時に解決される文字列が同じものである、というのがこの対の要件。
pub(crate) fn shorten_home(path: &str) -> String {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .and_then(|h| h.to_str().map(str::to_string))
        // 空や `/` を home とする環境では縮めない（縮めても情報が増えない）。
        .filter(|h| !h.is_empty() && h != "/");
    let Some(home) = home else {
        // ⭐ home が無くても、根が宣言されていれば `/…` には縮められる。
        return shorten_sandbox_root(path).unwrap_or_else(|| path.to_string());
    };
    let home = home.as_str();
    let home = home.strip_suffix('/').unwrap_or(home);
    if path == home {
        return "~".to_string();
    }
    match path.strip_prefix(home) {
        // 区切りが続くときだけ ＝ home がパス成分として一致したときだけ縮める。
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        // ⭐ home の外なら、根の下かどうかを次に試す（`$HOME` は根の**下**に在るので、
        // 順序は「具体的な方が先」）。宣言が無ければ原文のまま。
        _ => shorten_sandbox_root(path).unwrap_or_else(|| path.to_string()),
    }
}

pub(crate) fn load_startup_document(filename: Option<&str>) -> StartupDocument {
    let Some(path) = filename else {
        return StartupDocument::Empty;
    };

    let expanded = expand_tilde(path);
    let path_ref = expanded.as_path();
    if path_ref.is_dir() {
        return StartupDocument::Directory {
            tree: BrowseTree::build(path_ref),
        };
    }

    let lines = std::fs::read_to_string(path_ref)
        .map(|content| lines_from_content(&content))
        .unwrap_or_else(|_| vec![String::new()]);

    // ⚠️ 保持するのは**展開後**のパス。原文（`~/x`）を持つと、後の `save` が
    // また `~` から解き直すことになり、開いた先と保存先がずれうる。
    StartupDocument::File {
        path: expanded,
        lines,
    }
}

pub(crate) fn build_browse_tree(filename: Option<&PathBuf>, working_dir: &Path) -> BrowseTree {
    let root = existing_browse_root(filename, working_dir);
    let mut tree = BrowseTree::build(&root);
    if let Some(file) = filename {
        tree.select_path(file);
    }
    tree
}

pub fn save(editor: &mut EditorState) -> io::Result<()> {
    let Some(path) = editor.filename.clone() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No filename set. Use Save As to specify a filename.",
        ));
    };
    let target = editor.resolve_in_working_dir(&path);
    write_buffer(
        editor,
        &target,
        &format!("Failed to save '{}'", target.display()),
    )?;
    mark_saved(editor);
    Ok(())
}

pub fn save_as(editor: &mut EditorState, path: &str) -> io::Result<()> {
    if path.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Filename is empty",
        ));
    }

    let path_buf = expand_tilde(path);
    let target = editor.resolve_in_working_dir(&path_buf);
    write_buffer(editor, &target, &format!("Failed to save '{}'", path))?;

    editor.filename = Some(path_buf);
    mark_saved(editor);
    Ok(())
}

pub fn open_file(editor: &mut EditorState, path: &str) -> io::Result<()> {
    if path.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Filename is empty",
        ));
    }

    let path_buf = expand_tilde(path);
    if !path_buf.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("File not found: {}", path),
        ));
    }
    if !path_buf.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Not a file: {}", path),
        ));
    }

    let content = std::fs::read_to_string(&path_buf).map_err(|e| {
        let kind = if e.kind() == io::ErrorKind::PermissionDenied {
            io::ErrorKind::PermissionDenied
        } else {
            e.kind()
        };
        io::Error::new(kind, format!("Failed to open '{}': {}", path, e))
    })?;

    let lines = lines_from_content(&content);
    editor.buffer = TextBuffer::from_lines(lines);
    editor.filename = Some(path_buf);
    editor.cursor = Cursor::default();
    editor.modified = false;
    editor.scroll_offset = 0;
    Ok(())
}

/// バッファをファイルへ書き出す。
///
/// 以前はここで `File::create`（＝ `O_TRUNC`）してから1行ずつ書いていた。つまり
/// **ユーザーのファイルを先に空にしてから**書き直していたので、途中で死ぬと
/// **前半だけのファイル**が残る。空になるより悪い —— 空なら壊れたと一目で分かるが、
/// 200行だけのファイルは普通に開けてしまい、後半が消えたことに当分気づかない。
/// `fsync` も無かったので、書いたつもりの行が電源断で消えることもあった。
/// cozy は hsh-ios にも同梱される＝**OS がいつアプリを殺してもおかしくない**環境で走る。
///
/// ## 素朴な「tmp に書いて rename」ではいけない
///
/// エディタは**他人のファイルを預かる**。rename は新しい inode を持ち込むので、素直に
/// やると **symlink を実体ファイルに化けさせる**。`~/.hshrc` を dotfiles リポへ symlink
/// している人が cozy で編集すると、リンクが切れてリポジトリ側が取り残され、しかも編集は
/// 成功して見える（実測で確認した）。ハードリンクや、元ファイルの mode も同様に飛ぶ。
///
/// nano / vim / neovim を実際に測ったところ、**3つとも tmp→rename をしていない**
/// （その場で `O_TRUNC` して書き、`fsync` する。inode・mode・symlink はすべて保たれる）。
/// 安全性は別口——vim の swap ファイル、nano の緊急保存——で買っている。
///
/// ## ここでの方針（vim の `backupcopy=auto` 相当）
///
/// 1. **symlink は先に解決**し、実体のパスを相手にする。リンクは生き残る。
///    **リンク先がまだ無くても**（消した直後・dotfiles リポ未 clone）リンクは潰さない。
/// 2. 通常は**不可分に置換**（tmp を隣に書く → fsync → 親ディレクトリも fsync → rename）。
///    元ファイルの mode は rename の**前に**一時ファイルへ移すので、権限の緩い瞬間が無い。
/// 3. **置換してはいけない/できない**ときは、その場書き + `fsync` に落ちる（nano と同じ）:
///    - ハードリンクが張られている（`st_nlink > 1`）——rename は別 inode になり、リンクが切れる。
///    - 親ディレクトリに書込権が無い（ファイルは書けるがディレクトリは書けない。`/etc` 等）
///      ——rename にはディレクトリの書込権が要るので失敗する。
fn write_buffer(editor: &EditorState, target: &std::path::Path, context: &str) -> io::Result<()> {
    ensure_parent_dir(target)?;

    // symlink の実体を相手にする（リンク自体を rename で潰さないため）。
    let real = resolve_symlink(target);
    if real != target {
        // リンク先の親が無ければ、そこには書けない（黙って作らない）。
        ensure_parent_dir(&real)?;
    }
    let existing = std::fs::metadata(&real).ok();

    if replaceable(existing.as_ref()) {
        match write_atomically(editor, &real, existing.as_ref()) {
            Ok(()) => return Ok(()),
            // ディレクトリに書込権が無いと rename できない。ファイル自体は書けることが
            // あるので、諦めずにその場書きへ落ちる（まだ target には触れていない）。
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {}
            Err(e) => return Err(annotate(e, context)),
        }
    }

    write_in_place(editor, &real).map_err(|e| annotate(e, context))
}

fn annotate(e: io::Error, context: &str) -> io::Error {
    io::Error::new(e.kind(), format!("{}: {}", context, e))
}

/// Replace a file cozy owns (the swap journal) with `bytes`, atomically.
///
/// This is the state-file case, not the user's-document case: no symlink or mode
/// to preserve, because we created the file. A torn swap is worse than no swap —
/// it looks recoverable — so it goes through the same tmp/fsync/rename.
pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    AtomicFile::new(path, AllowOverwrite)
        .write(|f| f.write_all(bytes))
        .map_err(|e| match e {
            atomicwrites::Error::Internal(e) | atomicwrites::Error::User(e) => e,
        })
}

/// 書く相手の実体パスを返す（symlink はリンクでなくリンク先を書く）。
///
/// `canonicalize` は**リンク先が存在しないと失敗する**ので、それだけに頼ると
/// **リンク先を消した直後・dotfiles リポをまだ clone していないマシン**で、
/// rename が symlink を実体ファイルに化けさせる（＝守ったはずのリンクをそこだけ潰す）。
/// リンクが在る限りは `read_link` で行き先を出し、**リンクが指していた場所を書く**。
/// 存在しない普通のパス（新規ファイル）はそのまま返す。
fn resolve_symlink(target: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(target) {
        return real;
    }
    // リンクは在るがリンク先が無い。行き先を解決して、そこを書きに行く。
    if target.is_symlink() {
        if let Ok(link) = std::fs::read_link(target) {
            if link.is_absolute() {
                return link;
            }
            if let Some(parent) = target.parent() {
                return parent.join(link);
            }
            return link;
        }
    }
    target.to_path_buf()
}

/// rename で置き換えてよいか（ハードリンクの共有先を切り離さないか）。
#[cfg(unix)]
fn replaceable(existing: Option<&std::fs::Metadata>) -> bool {
    use std::os::unix::fs::MetadataExt;
    // 新規ファイル（None）は置換して構わない。既存はリンク数で判断する。
    existing.map(|m| m.nlink() <= 1).unwrap_or(true)
}

#[cfg(not(unix))]
fn replaceable(_existing: Option<&std::fs::Metadata>) -> bool {
    true
}

/// tmp → fsync → 親ディレクトリ fsync → rename。読み手には保存前か保存後しか見えない。
fn write_atomically(
    editor: &EditorState,
    real: &Path,
    existing: Option<&std::fs::Metadata>,
) -> io::Result<()> {
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        // `st_mode` にはファイル種別のビットも乗っている。Linux/macOS の chmod は黙って
        // 落とすが、POSIX では permission bits 以外の扱いは未規定なので手前で削る。
        existing.map(|m| m.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let _ = existing;

    AtomicFile::new(real, AllowOverwrite)
        .write(|file| {
            {
                let mut out = BufWriter::new(&mut *file);
                for line in &editor.buffer.lines {
                    writeln!(out, "{}", line)?;
                }
                out.flush()?;
            }
            // rename の**前に** mode を移す。後から chmod すると、その一瞬だけ
            // 権限の緩いファイルが見える（0600 のファイルを預かっている場合に困る）。
            #[cfg(unix)]
            if let Some(mode) = mode {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(std::fs::Permissions::from_mode(mode))?;
            }
            Ok(())
        })
        .map_err(|e| match e {
            atomicwrites::Error::Internal(e) | atomicwrites::Error::User(e) => e,
        })
}

/// その場で切って書き、`fsync` する（nano と同じ）。不可分ではないが、inode・mode・
/// ハードリンクを保つ。**置換が使えない相手にだけ**使う。
fn write_in_place(editor: &EditorState, real: &Path) -> io::Result<()> {
    let mut file = File::create(real)?;
    {
        let mut out = BufWriter::new(&mut file);
        for line in &editor.buffer.lines {
            writeln!(out, "{}", line)?;
        }
        out.flush()?;
    }
    // 書いたバイトをディスクに載せる。cozy にはこれすら無かった。
    file.sync_all()
}

/// 「保存先の親ディレクトリが無い」を、**文字列ではなく型で**運ぶ。
///
/// 呼び出し側（reducer）はこれを見て「作りますか」の一行を出す。⚠️ メッセージ本文の
/// 一致で判定すると、**文言を直した瞬間に静かに壊れる**（オファーが出なくなり、
/// 利用者にはただのエラーに戻る＝テストも文言を写していれば一緒に緑のまま）。
#[derive(Debug)]
pub struct MissingParent(pub PathBuf);

impl std::fmt::Display for MissingParent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 従来の文言を保つ（この文字列を読んでいる利用者・ログが在りうる）。
        write!(f, "Directory not found:{}", self.0.display())
    }
}

impl std::error::Error for MissingParent {}

/// 保存が「親ディレクトリが無い」で止まり、**利用者の一行の答えを待っている**状態。
///
/// 形は swap 復元のオファー（`swap::Recovery`）に揃えてある —— 一行・2 キー・
/// それ以外は無視。vim の swap ダイアログのような壁は作らない。
///
/// ⚠️ **黙って作らない**（`write_buffer` の方針）と、**mkdir -p のためにエディタを
/// 抜けさせない**（cozy は comfort-first で、しかも iOS では抜けた先のシェルが狭い）の
/// 両方を満たすのがこの形。
pub struct CreateDirOffer {
    /// 作るべきディレクトリ（欠けている親）。
    pub dir: PathBuf,
    /// 止まった保存を**そのまま**やり直すための、呼ばれたときの名前。
    /// ⚠️ 保存先を自前で組み直さないのが肝 —— 組み直すと `save`/`save_as` の
    /// 解決規則と 2 実装になり、片方だけ直る。
    pub fname: String,
    /// Ctrl+X（保存して終了）から来たか。作って保存できたら、その意図どおり終了する。
    pub and_exit: bool,
}

/// `e` が「親ディレクトリが無い」なら、その作るべきディレクトリを返す。
///
/// ⚠️ `write_buffer` の `annotate` は `io::Error` を**文字列に潰す**ので、payload を
/// 残したまま返せるのは `ensure_parent_dir` が `?` で直に上げる経路だけ。
/// 逆に言えば、annotate を通った先でこれを呼んでも `None` になる（＝取り違えない）。
#[must_use]
pub fn missing_parent_of(e: &io::Error) -> Option<PathBuf> {
    e.get_ref()
        .and_then(|inner| inner.downcast_ref::<MissingParent>())
        .map(|m| m.0.clone())
}

fn ensure_parent_dir(target: &std::path::Path) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                MissingParent(parent.to_path_buf()),
            ));
        }
    }
    Ok(())
}

fn mark_saved(editor: &mut EditorState) {
    editor.last_saved_id = editor.undo_stack.len();
    editor.modified = false;
    // The file on disk now says what the buffer says: nothing left to recover.
    crate::swap::remove(editor);
    // Save As points at a different file, so the swap must follow the target.
    editor.swap_path = crate::swap::path_for(editor);
}

fn existing_browse_root(filename: Option<&PathBuf>, working_dir: &Path) -> PathBuf {
    let mut current = filename
        .and_then(|path| path.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| working_dir.to_path_buf());

    while !current.exists() {
        if !current.pop() || current.as_os_str().is_empty() {
            return working_dir.to_path_buf();
        }
    }

    current
}

fn lines_from_content(content: &str) -> Vec<String> {
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tilde_tests {
    use super::*;

    // ここで固定しているのは **「先頭の `~` が展開されること」**。それだけ。
    //
    // 2026-07-30 に iOS で踏んだ穴: 端末に打つとシェルが `~` を展開するので、cozy は普段
    // チルダを見ない。ところが argo が cozy を **インプロセスの TUI プロバイダ**として呼ぶ経路は
    // 単語展開を通らず、`~/.ssh/config` が**そのまま**届いていた。cozy に展開は
    // **1 行も無かった**ので `<cwd>/~/.ssh/config`（`~` という名のディレクトリを含む相対パス）と
    // 解釈され、親が無いので `Directory not found` になっていた。
    //
    // ⚠️ **`$HOME` を `dirs::home_dir()` より先に見るのは「明示のため」で、バグの原因ではない。**
    // 最初は「iOS では両者が別の場所を指す」と考えて、それを固定するテストを書いた ——
    // **が、カナリアが鳴らなかった**。Unix（iOS を含む）の `dirs::home_dir()` は
    // まず `$HOME` を読むので、**両者は一致する**。テストは差を検出できていなかった。
    // ∴ ここが主張できるのは「展開する」ことだけ。⚠️ 展開そのものを外すと 2 本落ちる（実測）。

    /// `HOME` を差し替えて `f` を走らせ、必ず元に戻す。
    /// ⚠️ ロックは `file_io::HOME_LOCK`（モジュール跨ぎで共有・上の doc）。
    fn with_home<T>(home: &str, f: impl FnOnce() -> T) -> T {
        let _guard = super::HOME_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let original = std::env::var_os("HOME");
        // SAFETY: HOME_LOCK で直列化済み。この区間で他スレッドは env を触らない。
        unsafe { std::env::set_var("HOME", home) };
        let out = f();
        unsafe {
            match original {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
        out
    }

    /// `HOME` と `COZY_SANDBOX_ROOT` を同時に敷いて `f` を走らせ、必ず元に戻す。
    /// ⚠️ ロックは `HOME_LOCK` を共用する（どちらもプロセス全域の env なので）。
    fn with_sandbox<T>(root: &str, home: &str, f: impl FnOnce() -> T) -> T {
        let _guard = super::HOME_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (oh, or_) = (
            std::env::var_os("HOME"),
            std::env::var_os("COZY_SANDBOX_ROOT"),
        );
        // SAFETY: HOME_LOCK で直列化済み。
        unsafe {
            std::env::set_var("HOME", home);
            std::env::set_var("COZY_SANDBOX_ROOT", root);
        }
        let out = f();
        unsafe {
            match oh {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match or_ {
                Some(v) => std::env::set_var("COZY_SANDBOX_ROOT", v),
                None => std::env::remove_var("COZY_SANDBOX_ROOT"),
            }
        }
        out
    }

    #[test]
    fn a_path_outside_home_is_shown_from_the_sandbox_root() {
        // ⭐ **`cozy /notes.md` のための対**。`$HOME` の下は `~` で縮むが、その外側
        // （コンテナ直下）は縮めようがなく、保存プロンプトに物理パスが出ていた。
        // iOS の形をそのまま敷く: 根＝コンテナ、`$HOME` はその下の `Documents`。
        with_sandbox("/private/container", "/private/container/Documents", || {
            // home の外 ＝ 根からの論理パスで見せる
            assert_eq!(shorten_home("/private/container/notes.md"), "/notes.md");
            assert_eq!(shorten_home("/private/container/Library/x"), "/Library/x");
            assert_eq!(shorten_home("/private/container"), "/");
            // ⭐ **home の下は今までどおり `~`** —— 根より具体的な方を優先する。
            // これが逆だと `~/notes.md` が `/Documents/notes.md` として出る。
            assert_eq!(
                shorten_home("/private/container/Documents/notes.md"),
                "~/notes.md"
            );
            // 根の外は触らない（縮めすぎの否定）
            assert_eq!(shorten_home("/etc/hosts"), "/etc/hosts");
            // 成分単位でしか一致させない
            assert_eq!(
                shorten_home("/private/container-2/x"),
                "/private/container-2/x"
            );
        });
    }

    #[test]
    fn the_sandbox_root_round_trips() {
        // ⭐ 画面に出す文字列は、保存時に解決される文字列と同じ場所を指す必要がある。
        with_sandbox("/private/container", "/private/container/Documents", || {
            for original in [
                "/private/container/notes.md",
                "/private/container/Library/x",
                "/private/container/Documents/notes.md",
            ] {
                let shown = shorten_home(original);
                assert_eq!(
                    expand_tilde(&shown),
                    PathBuf::from(original),
                    "{shown} must resolve back to {original}"
                );
            }
        });
    }

    #[test]
    fn an_already_physical_path_is_not_prefixed_twice() {
        // ⚠️ ホスト（argo）は cozy へ渡す前に自分で翻訳しているので `<root>/x` が届く。
        // それを無条件に前置すると `<root><root>/x` になる。
        with_sandbox("/private/container", "/private/container/Documents", || {
            assert_eq!(
                expand_tilde("/private/container/notes.md"),
                PathBuf::from("/private/container/notes.md")
            );
            // 相対パスも触らない
            assert_eq!(expand_tilde("notes.md"), PathBuf::from("notes.md"));
        });
    }

    #[test]
    fn without_a_declared_root_nothing_is_translated() {
        // ⭐ **desktop のカナリア**。宣言が無ければ `/…` は `/…` のまま。
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(shorten_home("/etc/hosts"), "/etc/hosts");
            assert_eq!(expand_tilde("/etc/hosts"), PathBuf::from("/etc/hosts"));
        });
    }

    #[test]
    fn a_path_under_home_is_shown_with_a_tilde() {
        // ⭐ **表示方向**。cozy は解決済みの絶対パスを持っているので、これが無いと
        // 保存プロンプトが `/home/you/notes.md` を出す —— argo の中では
        // `/data/data/com.hsh.mobile/files/notes.md` という**コンテナパスが画面に出る**。
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(shorten_home("/tmp/cozy-home-test/notes.md"), "~/notes.md");
            assert_eq!(shorten_home("/tmp/cozy-home-test"), "~");
        });
    }

    #[test]
    fn shorten_home_only_matches_whole_path_components() {
        // ⚠️ `HOME=/tmp/cozy-home-test` で `/tmp/cozy-home-testing/x` を
        // `~ing/x` にしてはいけない。前方一致だけで判定すると必ずこれを踏む。
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(
                shorten_home("/tmp/cozy-home-testing/x"),
                "/tmp/cozy-home-testing/x"
            );
            // home の外は素通り。
            assert_eq!(shorten_home("/etc/hosts"), "/etc/hosts");
            // 相対パスも素通り（そのまま見せるのが正しい）。
            assert_eq!(shorten_home("notes.md"), "notes.md");
        });
    }

    #[test]
    fn a_root_home_is_never_shortened() {
        // `HOME=/` の環境で縮めると**全部が `~/…`** になり、`~` が何も意味しなくなる。
        with_home("/", || {
            assert_eq!(shorten_home("/etc/hosts"), "/etc/hosts");
        });
    }

    #[test]
    fn shortening_and_expanding_are_a_round_trip() {
        // ⭐ **対であることが要件**。画面に出す文字列は、保存時に解決される文字列と
        // 同じ場所を指していなければならない（利用者はこの buffer をそのまま編集する）。
        with_home("/tmp/cozy-home-test", || {
            for original in [
                "/tmp/cozy-home-test/notes.md",
                "/tmp/cozy-home-test/a/b/c.txt",
                "/tmp/cozy-home-test",
            ] {
                let shown = shorten_home(original);
                assert_eq!(
                    expand_tilde(&shown),
                    PathBuf::from(original),
                    "{shown} must resolve back to {original}"
                );
            }
        });
    }

    #[test]
    fn tilde_slash_expands_to_home() {
        // ⚠️ カナリア: expand_tilde の展開を外すと `~/.ssh/config` がそのまま残って落ちる（実測）。
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(
                expand_tilde("~/.ssh/config"),
                PathBuf::from("/tmp/cozy-home-test/.ssh/config")
            );
        });
    }

    #[test]
    fn bare_tilde_is_the_home_itself() {
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(expand_tilde("~"), PathBuf::from("/tmp/cozy-home-test"));
        });
    }

    #[test]
    fn tilde_user_is_left_alone() {
        // `~alice` を解決する手段が無い。勝手に別の場所へ向けるより原文のまま渡す。
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(expand_tilde("~alice/x"), PathBuf::from("~alice/x"));
        });
    }

    #[test]
    fn a_tilde_that_is_not_a_prefix_is_left_alone() {
        // ファイル名の中の `~` は普通に出る（バックアップ名など）。触らない。
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(expand_tilde("notes~"), PathBuf::from("notes~"));
            assert_eq!(expand_tilde("dir/~/x"), PathBuf::from("dir/~/x"));
        });
    }

    #[test]
    fn ordinary_paths_are_untouched() {
        with_home("/tmp/cozy-home-test", || {
            assert_eq!(expand_tilde("notes.md"), PathBuf::from("notes.md"));
            assert_eq!(expand_tilde("/etc/hosts"), PathBuf::from("/etc/hosts"));
        });
    }
}

#[cfg(all(test, unix))]
mod write_buffer_tests {
    use super::*;
    use crate::state::{EditorState, TextBuffer};
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    /// 一時ディレクトリ（tempfile 非依存・テストごとに一意。browse の慣習に揃える）。
    fn scratch(name: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("cozy_write_test_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn editor_with(lines: &[&str]) -> EditorState {
        let mut editor = EditorState::new(None);
        editor.buffer = TextBuffer::from_lines(lines.iter().map(|s| s.to_string()).collect());
        editor
    }

    fn save_to(path: &Path, lines: &[&str]) {
        write_buffer(&editor_with(lines), path, "test").unwrap();
    }

    /// リンク先がまだ無い symlink（リンク先を消した直後・dotfiles リポ未 clone）でも、
    /// **リンクを食い潰さずリンク先に書く**。`canonicalize` は dangling で失敗するので、
    /// それだけに頼っているとここだけ素朴な rename に落ちてリンクが実体化する。
    #[test]
    fn editing_through_a_dangling_symlink_keeps_the_link() {
        let dir = scratch("dangling");
        fs::create_dir_all(dir.join("dotfiles")).unwrap();
        let real = dir.join("dotfiles/hshrc"); // まだ存在しない
        let link = dir.join("hshrc");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        save_to(&link, &["edited"]);

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "symlink が実体ファイルに置き換わった"
        );
        assert_eq!(
            fs::read_to_string(&real).unwrap(),
            "edited\n",
            "リンク先に書けていない"
        );
    }

    /// 相対リンクも同じ（リンクのある場所を基準に解決する）。
    #[test]
    fn editing_through_a_relative_dangling_symlink_keeps_the_link() {
        let dir = scratch("dangling_relative");
        fs::create_dir_all(dir.join("dotfiles")).unwrap();
        let link = dir.join("hshrc");
        std::os::unix::fs::symlink("dotfiles/hshrc", &link).unwrap();

        save_to(&link, &["edited"]);

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(dir.join("dotfiles/hshrc")).unwrap(),
            "edited\n"
        );
    }

    /// リンク先の**親ディレクトリ**が無いときは、黙って作らずに断る
    /// （`ensure_parent_dir` の流儀＝エディタは勝手にディレクトリを生やさない）。
    #[test]
    fn a_dangling_symlink_into_a_missing_directory_is_refused() {
        let dir = scratch("dangling_no_parent");
        let link = dir.join("hshrc");
        std::os::unix::fs::symlink(dir.join("no_such_dir/hshrc"), &link).unwrap();

        let err = write_buffer(&editor_with(&["edited"]), &link, "test").unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "断ったのに symlink を壊している"
        );
    }

    /// `~/.hshrc` を dotfiles リポへ symlink している人が cozy で編集しても、
    /// **リンクが実体ファイルに化けない**（素朴な rename はここで壊す）。
    #[test]
    fn editing_through_a_symlink_keeps_the_link() {
        let dir = scratch("symlink");
        fs::create_dir_all(dir.join("dotfiles")).unwrap();
        let real = dir.join("dotfiles/hshrc");
        fs::write(&real, "original\n").unwrap();
        let link = dir.join("hshrc");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        save_to(&link, &["edited"]);

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "symlink が実体ファイルに置き換わった"
        );
        // 編集がリンク先＝リポジトリ側に届いていること（届かないと黙って乖離する）
        assert_eq!(fs::read_to_string(&real).unwrap(), "edited\n");
    }

    /// 預かったファイルの mode を変えない（0600 の rc が 0664 になって漏れない）。
    #[test]
    fn preserves_the_mode_of_the_file_it_was_given() {
        let dir = scratch("mode");
        let path = dir.join("secret.conf");
        fs::write(&path, "old\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        save_to(&path, &["new"]);

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "mode が rename で飛んだ");
    }

    /// ハードリンクされたファイルは rename すると相方が取り残される。
    /// その場書きへ落ちて inode を保つ（nano と同じ振る舞い）。
    #[test]
    fn hardlinked_file_stays_the_same_inode() {
        let dir = scratch("hardlink");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        fs::write(&a, "old\n").unwrap();
        fs::hard_link(&a, &b).unwrap();
        let ino = fs::metadata(&a).unwrap().ino();

        save_to(&a, &["new"]);

        assert_eq!(
            fs::metadata(&a).unwrap().ino(),
            ino,
            "inode が変わった＝リンクが切れた"
        );
        assert_eq!(
            fs::read_to_string(&b).unwrap(),
            "new\n",
            "相方に反映されていない"
        );
    }

    /// 普通のファイルは rename で置換される（＝保存中の姿が観測されない）。
    #[test]
    fn plain_file_is_replaced_by_rename() {
        let dir = scratch("plain");
        let path = dir.join("note.md");
        fs::write(&path, "old\n").unwrap();
        let ino = fs::metadata(&path).unwrap().ino();

        save_to(&path, &["new", "lines"]);

        assert_ne!(
            fs::metadata(&path).unwrap().ino(),
            ino,
            "その場書きに落ちている"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "new\nlines\n");
    }

    /// 一時ファイルを置き去りにしない（`.note.md.tmp` が残ると壊れて見える）。
    #[test]
    fn leaves_no_temp_file_behind() {
        let dir = scratch("strays");
        save_to(&dir.join("note.md"), &["x"]);

        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != "note.md")
            .collect();
        assert!(names.is_empty(), "残骸: {names:?}");
    }

    /// 新規ファイルもちゃんと作れる（存在しないので canonicalize は失敗する経路）。
    #[test]
    fn creates_a_new_file() {
        let dir = scratch("new");
        let path = dir.join("fresh.md");
        save_to(&path, &["hello"]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\n");
    }
}
