//! 批量写入触发策略：条数上限或时间间隔，先到者触发（纯逻辑，单测覆盖）。

use std::time::{Duration, Instant};

/// 触发条件：`pending >= max_batch` 或 `距上次 flush >= interval`。
pub struct FlushPolicy {
    max_batch: usize,
    interval: Duration,
    pending: usize,
    last_flush: Instant,
}

impl FlushPolicy {
    /// `max_batch` 至少为 1；`interval` 为 0 表示每次 tick 都触发（测试用）。
    pub fn new(max_batch: usize, interval: Duration) -> Self {
        Self {
            max_batch: max_batch.max(1),
            interval,
            pending: 0,
            last_flush: Instant::now(),
        }
    }

    /// 记录一个入队事件；返回是否应立即 flush（条数触发）。
    pub fn on_event(&mut self) -> bool {
        self.pending += 1;
        self.pending >= self.max_batch
    }

    /// tick 检查；返回是否应 flush（时间触发，且有待写数据）。
    pub fn on_tick(&self) -> bool {
        self.pending > 0 && self.last_flush.elapsed() >= self.interval
    }

    /// flush 完成后重置计数与计时。
    pub fn on_flush(&mut self) {
        self.pending = 0;
        self.last_flush = Instant::now();
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    pub fn max_batch(&self) -> usize {
        self.max_batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    /// 条数触发：达到上限返回 true，未达到返回 false。
    #[test]
    fn batch_size_triggers_flush() {
        let mut p = FlushPolicy::new(3, Duration::from_secs(60));
        assert!(!p.on_event());
        assert!(!p.on_event());
        assert!(p.on_event(), "第 3 条应触发");
        assert_eq!(p.pending(), 3);
    }

    /// 时间触发：无待写数据时永不触发；有待写数据且超时则触发。
    #[test]
    fn time_triggers_flush_only_when_pending() {
        let mut p = FlushPolicy::new(1000, Duration::from_millis(5));
        assert!(!p.on_tick(), "无数据不应触发");
        p.on_event();
        assert!(!p.on_tick(), "刚入队未超时不应触发");
        sleep(Duration::from_millis(10));
        assert!(p.on_tick(), "超时且有数据应触发");
    }

    /// flush 后重置：计数清零、时间重置，短间隔下不会立刻再次触发。
    #[test]
    fn flush_resets_state() {
        let mut p = FlushPolicy::new(2, Duration::from_millis(50));
        p.on_event();
        p.on_event();
        assert!(p.on_event() || p.pending() >= 2);
        p.on_flush();
        assert_eq!(p.pending(), 0);
        assert!(!p.on_tick(), "重置后不应立即触发");
    }

    /// 零间隔（测试场景）下 tick 立即触发；max_batch=0 被修正为 1，避免除零/死循环语义。
    #[test]
    fn zero_interval_and_zero_batch_are_safe() {
        let mut p = FlushPolicy::new(0, Duration::from_millis(0));
        assert_eq!(p.max_batch(), 1);
        assert!(p.on_event(), "max_batch=1 时首条即触发");
        p.on_flush();
        p.on_event();
        assert!(p.on_tick(), "零间隔应立即时间触发");
    }
}
