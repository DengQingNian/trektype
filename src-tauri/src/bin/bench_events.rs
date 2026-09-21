//! 事件管线压测：模拟高频键鼠事件 → 有界队列 → 批量写入，输出吞吐/丢弃/延迟基线。
//!
//! 运行：`cargo run --release --bin bench_events -- [秒数] [每秒事件数]`
//! 默认 10 秒 × 10000 事件/秒（计划验收目标：丢弃率 < 0.1%，flush P99 < 20ms）。
//!
//! 注意：该 bin 直接驱动管线与写入器，不安装任何全局钩子（不采集真实输入）。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use typetrek_lib::capture::event::{epoch_ms_fast, EventKind, MouseButton, RawEvent};
use typetrek_lib::capture::{CaptureShared, QUEUE_CAPACITY};
use typetrek_lib::pipeline;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let rate: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10_000);

    println!("== TypeTrek 管线压测：{seconds}s @ {rate} 事件/秒 ==");

    // 临时数据库（压测结束后删除）
    let db_path = std::env::temp_dir().join(format!("typetrek_bench_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db_path);

    let (tx, rx) = crossbeam_channel::bounded::<RawEvent>(QUEUE_CAPACITY);
    let shared = Arc::new(CaptureShared::new(tx));
    let writer = match pipeline::spawn_writer(shared.clone(), rx, db_path.clone(), false) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("写入器启动失败：{e}");
            return;
        }
    };

    // 等写入器就绪（数据库打开 + 迁移完成）
    let ready_deadline = Instant::now() + Duration::from_secs(10);
    while !shared.writer_ready.load(Ordering::SeqCst) {
        if Instant::now() > ready_deadline {
            eprintln!("写入器 10s 内未就绪，终止");
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let tick = Duration::from_millis(100); // 每 100ms 发送一批
    let per_tick = (rate / 10).max(1);
    let started = Instant::now();
    let mut sent: u64 = 0;
    let mut click = false;

    while started.elapsed() < Duration::from_secs(seconds) {
        let batch_start = Instant::now();
        for i in 0..per_tick {
            let ts = epoch_ms_fast();
            let ev = if click {
                RawEvent {
                    ts_ms: ts,
                    kind: EventKind::Click,
                    key_code: None,
                    button: Some(if i % 3 == 0 {
                        MouseButton::Right
                    } else {
                        MouseButton::Left
                    }),
                    x: Some((i as i32 * 7) % 1920),
                    y: Some((i as i32 * 13) % 1080),
                    hwnd_foreground: 0,
                    is_repeat: false,
                    is_injected: false,
                }
            } else {
                RawEvent {
                    ts_ms: ts,
                    kind: EventKind::KeyDown,
                    key_code: Some("KeyA"),
                    button: None,
                    x: None,
                    y: None,
                    hwnd_foreground: 0,
                    is_repeat: false,
                    is_injected: false,
                }
            };
            click = !click;
            if shared.try_emit(ev) {
                sent += 1;
            }
        }
        // 模拟真实采集的节奏（10k/s 分摊到 100ms）
        let elapsed = batch_start.elapsed();
        if elapsed < tick {
            std::thread::sleep(tick - elapsed);
        }
    }

    // 停止并等待 flush 完成
    let stop_started = Instant::now();
    shared.request_stop();
    let _ = writer.join();
    let drain_ms = stop_started.elapsed().as_millis();

    let dropped = shared.dropped.load(Ordering::Relaxed);
    let flushed = shared.flushed_events.load(Ordering::Relaxed);
    let flushes = shared.flush_count.load(Ordering::Relaxed);
    let errors = shared.write_errors.load(Ordering::Relaxed);
    let last_flush_ms = shared.last_flush_ms.load(Ordering::Relaxed);
    let dropped_rate = if sent > 0 {
        dropped as f64 / (sent + dropped) as f64
    } else {
        0.0
    };

    println!("--- 结果 ---");
    println!("发送事件：{sent}");
    println!("丢弃事件：{dropped}（丢弃率 {:.4}%）", dropped_rate * 100.0);
    println!("落库事件：{flushed}");
    println!(
        "写入批次：{flushes}（平均 {:.0} 事件/批）",
        flushed as f64 / flushes.max(1) as f64
    );
    println!("最近一批耗时：{last_flush_ms} ms");
    println!("退出排空耗时：{drain_ms} ms");
    println!("写入错误：{errors}");
    println!(
        "验收线：丢弃率 < 0.1% → {}；写入错误 = 0 → {}",
        if dropped_rate < 0.001 {
            "达标"
        } else {
            "未达标"
        },
        if errors == 0 { "达标" } else { "未达标" }
    );

    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }
}
