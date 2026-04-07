use chrono::{DateTime, Duration, Utc};

use crate::models::Item;

/// Apply a 1–5 confidence rating to `item` and return the updated version.
///
/// Rating semantics:
///   1 → Again  (total blank; short reset)
///   2 → Hard   (recalled with significant effort)
///   3 → Good   (recalled correctly with some effort)
///   4 → Great  (recalled quickly and confidently)
///   5 → Easy   (effortless recall)
pub fn apply_confidence(mut item: Item, confidence: u8, reviewed_at: DateTime<Utc>) -> Item {
    let prev = item.interval_days.max(1.0);
    let ease = item.ease.max(1.3);

    let (new_interval, ease_delta, lapse) = match confidence {
        1 => (0.5,                           -0.20_f64, true),
        2 => (f64::max(1.0, prev * 1.2),     -0.05,     false),
        3 => (f64::max(1.0, prev * 2.0),      0.00,     false),
        4 => (f64::max(1.0, prev * 2.5),      0.05,     false),
        5 => (f64::max(1.0, prev * 3.5),      0.10,     false),
        _ => (f64::max(1.0, prev * 1.5),      0.00,     false),
    };

    if lapse {
        item.lapses += 1;
    }
    item.ease = (ease + ease_delta).clamp(1.3, 3.0);
    item.review_count += 1;

    // Rolling average confidence
    let n = item.review_count as f64;
    item.confidence_avg = if item.review_count == 1 {
        confidence as f64
    } else {
        (item.confidence_avg * (n - 1.0) + confidence as f64) / n
    };

    item.interval_days = new_interval;
    item.last_reviewed_at = Some(reviewed_at);
    item.due_at = reviewed_at + Duration::seconds((new_interval * 86_400.0) as i64);
    item
}
