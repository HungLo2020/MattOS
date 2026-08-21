use std::time::{Duration, Instant};
use std::fmt;

/// 汎用ループガードエンジン (General Purpose LoopGuard Engine)
/// 
/// 無限ループ、CPU暴走、および長時間実行を検知・予防するための堅牢なコンポーネント。
/// TUIエディタのイベントループなど、任意の大規模反復処理に組み込み可能です。

/// ループガードエラー
#[derive(Debug, Clone)]
pub enum LoopGuardError {
    /// 最大反復回数を超過
    MaxIterationsExceeded { 
        name: String,
        current: usize, 
        max: usize 
    },
    /// 実行時間を超過
    Timeout { 
        name: String,
        elapsed: Duration, 
        max: Duration 
    },
}

impl fmt::Display for LoopGuardError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::MaxIterationsExceeded { name, current, max } => {
                write!(f, "loop '{}' aborted: exceeded max iterations ({} / {})", name, current, max)
            }
            Self::Timeout { name, elapsed, max } => {
                write!(f, "loop '{}' aborted: timeout ({:?} > {:?})", name, elapsed, max)
            }
        }
    }
}

impl std::error::Error for LoopGuardError {}

/// ガードレベルのプリセット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardLevel {
    /// デバッグ用: 非常に厳しい制限
    Strict,
    /// 通常用: 標準的な制限
    Normal,
    /// 本番用: 緩やかな制限
    Relaxed,
}

/// ループガードエンジンの詳細設定
#[derive(Debug, Clone)]
pub struct GuardConfig {
    pub max_iterations: usize,
    pub timeout: Option<Duration>,
    pub soft_warning_ratio: f64,
}

impl GuardConfig {
    pub fn from_level(level: GuardLevel) -> Self {
        match level {
            GuardLevel::Strict => Self {
                max_iterations: 10_000,
                timeout: Some(Duration::from_secs(1)),
                soft_warning_ratio: 0.5,
            },
            GuardLevel::Normal => Self {
                max_iterations: 1_000_000,
                timeout: Some(Duration::from_secs(5)),
                soft_warning_ratio: 0.8,
            },
            GuardLevel::Relaxed => Self {
                max_iterations: 10_000_000,
                timeout: Some(Duration::from_secs(30)),
                soft_warning_ratio: 0.9,
            },
        }
    }
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self::from_level(GuardLevel::Normal)
    }
}

/// LoopGuard Engine
pub struct LoopGuard {
    name: String,
    config: GuardConfig,
    iteration_count: usize,
    start_time: Instant,
    soft_warning_issued: bool,
}

impl LoopGuard {
    /// 名前、最大回数、タイムアウト秒を指定して新規作成
    pub fn new(name: &str, max_iterations: usize, timeout_secs: u64) -> Self {
        let config = GuardConfig {
            max_iterations,
            timeout: if timeout_secs > 0 { Some(Duration::from_secs(timeout_secs)) } else { None },
            ..Default::default()
        };
        Self::with_config(name, config)
    }

    /// 設定オブジェクトを使用して新規作成
    pub fn with_config(name: &str, config: GuardConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            iteration_count: 0,
            start_time: Instant::now(),
            soft_warning_issued: false,
        }
    }

    /// ガードレベルを指定して新規作成
    pub fn from_level(name: &str, level: GuardLevel) -> Self {
        Self::with_config(name, GuardConfig::from_level(level))
    }

    /// イテレーションを記録し、安全性をチェックする。
    /// エラーが発生した場合はErrを返す。
    pub fn tick(&mut self) -> Result<(), LoopGuardError> {
        self.iteration_count += 1;

        // 1. 反復回数チェック
        if self.iteration_count >= self.config.max_iterations {
            return Err(LoopGuardError::MaxIterationsExceeded {
                name: self.name.clone(),
                current: self.iteration_count,
                max: self.config.max_iterations,
            });
        }

        // 2. ソフト警告
        let soft_limit = (self.config.max_iterations as f64 * self.config.soft_warning_ratio) as usize;
        if !self.soft_warning_issued && self.iteration_count >= soft_limit {
            eprintln!(
                "warning: loop '{}' approaching limit: {} / {} iterations",
                self.name, self.iteration_count, self.config.max_iterations
            );
            self.soft_warning_issued = true;
        }

        // 3. タイムアウトチェック
        if let Some(timeout) = self.config.timeout {
            let elapsed = self.start_time.elapsed();
            if elapsed > timeout {
                return Err(LoopGuardError::Timeout {
                    name: self.name.clone(),
                    elapsed,
                    max: timeout,
                });
            }
        }

        Ok(())
    }

    pub fn iterations(&self) -> usize { self.iteration_count }
    pub fn elapsed(&self) -> Duration { self.start_time.elapsed() }
    pub fn name(&self) -> &str { &self.name }
}

impl Drop for LoopGuard {
    fn drop(&mut self) {
        let elapsed = self.elapsed();
        let avg_time = if self.iteration_count > 0 {
            elapsed.as_millis() as f64 / self.iteration_count as f64
        } else {
            0.0
        };
        eprintln!(
            "info: loop '{}' completed: {} iterations, {:?} total, {:.2}ms/iter",
            self.name, self.iteration_count, elapsed, avg_time
        );
    }
}
