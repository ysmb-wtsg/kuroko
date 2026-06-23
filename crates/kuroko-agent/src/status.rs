//! エージェントの稼働状態と、PTY出力の活動からの状態推定。
//! 出力の流れと経過時間からWorking/Idle等をプロバイダー非依存で判定する。

use std::time::{Duration, Instant};

/// アイドル判定のしきい値。最後の出力からこの時間が途絶えたらIdle（ユーザーの番）とみなす。
/// エージェントの出力（トークンストリームやスピナー）は連続的に届くため、
/// 短いバースト間の隙間でWorking↔Idleがちらつかない程度の値にする。
const IDLE_AFTER: Duration = Duration::from_millis(600);

/// エージェントの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// 起動中（まだ一度も出力がない）
    Starting,
    /// 入力待ち（出力が途絶え、ユーザーの番）
    Idle,
    /// 処理中（出力が流れている）
    Working,
    /// 終了済み
    Exited,
}

/// PTY出力の活動からエージェント状態を推定するトラッカー。
///
/// 出力が流れている間はWorking、一定時間途絶えたらIdle（ユーザーの番）、
/// プロセス終了でExited。「ツール承認待ち」と「完了して入力待ち」は
/// プロバイダー固有の出力解析なしには区別できないため、どちらもIdleに集約する
/// （特定エージェント向けの出力パースは行わない）。
pub struct ActivityTracker {
    /// 最後に出力を受信した時刻。未受信ならNone（=Starting）。
    last_output: Option<Instant>,
    /// プロセスが終了したか
    exited: bool,
}

impl ActivityTracker {
    /// 新しいトラッカーを生成する（初期状態はStarting）。
    ///
    /// @returns ActivityTrackerインスタンス
    pub fn new() -> Self {
        Self {
            last_output: None,
            exited: false,
        }
    }

    /// 出力受信を記録する。以降しきい値までの間はWorking扱いになる。
    ///
    /// @param now - 受信時刻
    pub fn record_output(&mut self, now: Instant) {
        self.last_output = Some(now);
    }

    /// プロセス終了を記録する。以降は常にExitedを返す。
    pub fn mark_exited(&mut self) {
        self.exited = true;
    }

    /// 指定時刻時点の推定状態を返す。
    ///
    /// @param now - 判定時刻
    /// @returns 推定されたエージェント状態
    pub fn status_at(&self, now: Instant) -> AgentStatus {
        if self.exited {
            return AgentStatus::Exited;
        }
        match self.last_output {
            None => AgentStatus::Starting,
            Some(last) if now.saturating_duration_since(last) < IDLE_AFTER => AgentStatus::Working,
            Some(_) => AgentStatus::Idle,
        }
    }
}

impl Default for ActivityTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// 入力待ちへの遷移を「ユーザーのターンごとに一度だけ」通知するトラッカー。
///
/// エージェントが処理を終えてユーザーの番（Idle）になった「瞬間」を捉える。
/// ただし1回の応答ターンの途中でも、ツール実行や長考で出力が IDLE_AFTER 以上
/// 途絶えると Working↔Idle がちらつく。これを毎回通知すると「1度の入力待ち」で
/// 何度も鳴ってしまうため、一度発火したら武装解除し、ユーザーが次の入力を送って
/// `rearm()` で再武装するまで再発火しない（ターン単位で1通知に集約する）。
pub struct IdleNotifier {
    /// 直前に観測した状態。未観測ならNone。
    prev: Option<AgentStatus>,
    /// 通知可能か。発火で false、ユーザー入力（rearm）で true に戻る。
    armed: bool,
}

impl IdleNotifier {
    /// 新しい通知トラッカーを生成する（初期状態は武装済み）。
    ///
    /// @returns IdleNotifierインスタンス
    pub fn new() -> Self {
        Self {
            prev: None,
            armed: true,
        }
    }

    /// ユーザー入力を受けて再武装する。次の Working→Idle 遷移で再び通知できるようになる。
    pub fn rearm(&mut self) {
        self.armed = true;
    }

    /// 現在状態を観測し、武装中に Working→Idle へ遷移した瞬間なら true を返す。
    /// 発火すると武装解除し、`rearm()` まで再発火しない。
    ///
    /// @param current - 現在のエージェント状態
    /// @returns 通知すべき遷移が起きたら true
    pub fn observe(&mut self, current: AgentStatus) -> bool {
        let fired =
            self.armed && self.prev == Some(AgentStatus::Working) && current == AgentStatus::Idle;
        if fired {
            self.armed = false;
        }
        self.prev = Some(current);
        fired
    }
}

impl Default for IdleNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_in_starting_before_any_output() {
        let tracker = ActivityTracker::new();
        let now = Instant::now();
        assert_eq!(tracker.status_at(now), AgentStatus::Starting);
    }

    #[test]
    fn working_right_after_output() {
        let mut tracker = ActivityTracker::new();
        let t0 = Instant::now();
        tracker.record_output(t0);
        assert_eq!(tracker.status_at(t0), AgentStatus::Working);
    }

    #[test]
    fn still_working_within_threshold() {
        let mut tracker = ActivityTracker::new();
        let t0 = Instant::now();
        tracker.record_output(t0);
        let within = t0 + IDLE_AFTER - Duration::from_millis(1);
        assert_eq!(tracker.status_at(within), AgentStatus::Working);
    }

    #[test]
    fn idle_after_threshold() {
        let mut tracker = ActivityTracker::new();
        let t0 = Instant::now();
        tracker.record_output(t0);
        let after = t0 + IDLE_AFTER + Duration::from_millis(1);
        assert_eq!(tracker.status_at(after), AgentStatus::Idle);
    }

    #[test]
    fn new_output_resets_to_working() {
        let mut tracker = ActivityTracker::new();
        let t0 = Instant::now();
        tracker.record_output(t0);
        let later = t0 + Duration::from_secs(5);
        // 一旦Idleになった後でも、新たな出力でWorkingに戻る
        assert_eq!(tracker.status_at(later), AgentStatus::Idle);
        tracker.record_output(later);
        assert_eq!(tracker.status_at(later), AgentStatus::Working);
    }

    #[test]
    fn exited_overrides_everything() {
        let mut tracker = ActivityTracker::new();
        let t0 = Instant::now();
        tracker.record_output(t0);
        tracker.mark_exited();
        // 出力直後でもExitedが優先される
        assert_eq!(tracker.status_at(t0), AgentStatus::Exited);
    }

    #[test]
    fn notifier_fires_on_working_to_idle() {
        let mut n = IdleNotifier::new();
        assert!(!n.observe(AgentStatus::Working));
        // Working→Idle の瞬間だけ発火する
        assert!(n.observe(AgentStatus::Idle));
    }

    #[test]
    fn notifier_does_not_refire_while_idle() {
        let mut n = IdleNotifier::new();
        n.observe(AgentStatus::Working);
        assert!(n.observe(AgentStatus::Idle));
        // Idleが継続している間は再発火しない
        assert!(!n.observe(AgentStatus::Idle));
        assert!(!n.observe(AgentStatus::Idle));
    }

    #[test]
    fn notifier_does_not_refire_within_same_turn() {
        // 1ターンの途中でツール実行等により Working↔Idle がちらついても、
        // rearm（ユーザー入力）がない限り再発火しない＝ターンあたり1通知
        let mut n = IdleNotifier::new();
        n.observe(AgentStatus::Working);
        assert!(n.observe(AgentStatus::Idle));
        // 出力が再開→再び途絶えても、まだ同じユーザーのターンなので鳴らない
        assert!(!n.observe(AgentStatus::Working));
        assert!(!n.observe(AgentStatus::Idle));
        assert!(!n.observe(AgentStatus::Working));
        assert!(!n.observe(AgentStatus::Idle));
    }

    #[test]
    fn notifier_refires_after_rearm() {
        let mut n = IdleNotifier::new();
        n.observe(AgentStatus::Working);
        assert!(n.observe(AgentStatus::Idle));
        // ユーザーが入力を送って次のターンが始まれば、もう一度発火する
        n.rearm();
        assert!(!n.observe(AgentStatus::Working));
        assert!(n.observe(AgentStatus::Idle));
    }

    #[test]
    fn notifier_ignores_starting_and_exited_to_idle() {
        // Starting→Idle（出力ゼロのケース）では発火しない
        let mut n = IdleNotifier::new();
        n.observe(AgentStatus::Starting);
        assert!(!n.observe(AgentStatus::Idle));

        // Exited→Idle のような異常遷移でも発火しない
        let mut n2 = IdleNotifier::new();
        n2.observe(AgentStatus::Exited);
        assert!(!n2.observe(AgentStatus::Idle));
    }
}
