use std::path::PathBuf;

pub(crate) fn current_working_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn startup_args() -> Vec<String> {
    std::env::args().collect()
}

/// ホストが宣言する**物理**のサンドボックス根（＝利用者から見た `/`）。
///
/// cozy が電話アプリ（argo）の中で動くとき、利用者が読み書きするのは `/notes.md` のような
/// **論理パス**だが、cozy が実際に開くのはホストが翻訳した
/// `/data/data/com.hsh.mobile/notes.md` のような**物理パス**。この対応を知らないと、
/// 保存プロンプトがコンテナパスを画面に出す（2026-08-04 に iOS 実機で指摘された。
/// `$HOME` の下は `~` で縮められるが、**その外側 —— まさに `/notes.md` —— は
/// 縮めようが無かった**）。
///
/// ⭐ **ホストが宣言する**（`COZY_COMPACT` と同じ形）。cozy には知りようがない。
/// 未設定なら翻訳は一切起きない ＝ **desktop は 1 ビットも変わらない**。
///
/// ⚠️ **キャッシュしない**（`compact()` と違う点）。読むのは保存/展開のときだけで頻度が低く、
/// 何より **`OnceLock` にするとテストが値を変えられない**（プロセスに 1 度しか読まれない）。
/// ⚠️ `/` を根とする宣言は無視する —— 全部が `/…` になって翻訳が恒等になり、意味が無い。
pub(crate) fn sandbox_root() -> Option<PathBuf> {
    std::env::var_os("COZY_SANDBOX_ROOT")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p != std::path::Path::new("/"))
}

/// Low-spec / mobile host hint. When `COZY_COMPACT` is set (and not empty or
/// "0"), cozy uses the lightweight welcome and hides line numbers by default,
/// cutting per-frame cells on full-repaint GPUs (e.g. an Android tablet's
/// Mali-G52 driven by hsh-ios). The host sets it — cozy can't detect the GPU —
/// and cozy just honours it. Cached: the env doesn't change mid-run.
pub(crate) fn compact() -> bool {
    use std::sync::OnceLock;
    static COMPACT: OnceLock<bool> = OnceLock::new();
    *COMPACT.get_or_init(|| {
        std::env::var("COZY_COMPACT")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}
