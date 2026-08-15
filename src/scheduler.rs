use chrono::{DateTime, Duration, Utc};

use crate::models::Item;

// Forgetting-curve constants

/// power-law decay exponent for the FSRS forgetting curve.
const DECAY: f64 = -0.5;

/// scale factor derived so that R(t=S, S) = 0.9 exactly
/// derivation:
///   R(t, S)  = (1 + FACTOR·t/S)^DECAY  =  desired_retention
///   at t = S:
///   (1 + FACTOR)^DECAY  = 0.9
///    1 + FACTOR         = 0.9^(1/DECAY) = 0.9^(1/−0.5) = 0.9^(−2) = 1/0.81
///    FACTOR             = 1/0.81 − 1 = 19/81
const FACTOR: f64 = 19.0 / 81.0;

/// target recall probability at each scheduled review
const DESIRED_RETENTION: f64 = 0.9;

// ~~~ weak-step handling ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
/// shared threshold for leech detection, and scaffold graduation
pub const LEECH_STREAK_THRESHOLD: u32 = 4;

/// Number of most-recent rating events (from `review_log`) inspected for the
/// oscillating Again/Hard/Again/Hard leech pattern that never produces a
/// clean consecutive streak of either type alone
pub const LEECH_LOOKBACK_EVENTS: usize = 6;

// FSRS-5 default weights
// source: https://github.com/open-spaced-repetition/py-fsrs
//
// parameter roles
// ---------------
// w[0..3]   initial stability S_0 for first-review ratings Again/Hard/Good/Easy
// w[4]      initial difficulty base value
// w[5]      exponential scale for initial difficulty by rating
// w[6]      difficulty change per rating step (signed delta)
// w[7]      mean-reversion weight pulling D toward its "Easy" baseline each review
// w[8]      log-scale base for the stability-increase factor after recall
// w[9]      stability decay exponent (how quickly gains shrink as S grows)
// w[10]     retrievability factor in the stability-increase formula
// w[11]     base multiplier for stability after a lapse
// w[12]     difficulty exponent in the post-lapse stability formula
// w[13]     previous-stability exponent in the post-lapse formula
// w[14]     retrievability factor in the post-lapse formula
// w[15]     hard-rating stability penalty (multiplied into S′ after Hard recall)
// w[16]     easy-rating stability bonus  (multiplied into S′ after Easy recall)
// w[17..18] short-term (same-day) re-study parameters, not exercised by myoso yet
//           but kept for completeness

const W: [f64; 19] = [
    0.40255, 1.18385, 3.17325, 15.69105, // w[0..3]   S_0 per first-review rating
    7.1949,  0.5345,  1.4604,             // w[4..6]   difficulty params
    0.0046,  1.54575, 0.1192,  1.01925,   // w[7..10]  stability-after-recall
    1.9395,  0.11,    0.29605, 2.2698,    // w[11..14] stability-after-lapse
    0.2315,  2.9898,                       // w[15..16] Hard penalty / Easy bonus
    0.51655, 0.6621,                       // w[17..18] short-term (unused here)
];

// internal algorithm

/// P(recall at time `t` days | memory stability `s` days)
/// Power-law forgetting curve shared by FSRS-4.5 and FSRS-5
/// Clamped to a small positive floor so downstream callers never see zero
fn retrievability(t: f64, s: f64) -> f64 {
    ((1.0 + FACTOR * t / s).powf(DECAY)).max(0.0001)
}

/// initial stability for brand-new item given first-review `rating` (1–4)
fn init_stability(rating: u8) -> f64 {
    let idx = (rating as usize).saturating_sub(1).min(3);
    W[idx].max(0.1)
}

/// initial difficulty for brand-new item given first-review `rating` (1–4)
/// D_0(r) = w[4] − exp(w[5] * (r − 1)) + 1,  clamped to [1, 10]
fn init_difficulty(rating: u8) -> f64 {
    let r = rating as f64;
    (W[4] - (W[5] * (r - 1.0)).exp() + 1.0).clamp(1.0, 10.0)
}

/// update difficulty after a review
/// step 1: apply a signed delta proportional to how far from "Good" the
///           rating was:  D′ = D − w[6] * (r − 3)
/// step 2: mean-revert toward the Easy baseline with weight w[7] so
///           difficulty can't drift permanently in one direction:
///           D′ = w[7] * D₀(4) + (1 − w[7]) * D′,  clamped to [1, 10]
fn next_difficulty(d: f64, rating: u8) -> f64 {
    let r      = rating as f64;
    let d_step = d - W[6] * (r - 3.0);
    let target = init_difficulty(4); // "Easy" baseline  ≈ w[4] − 1
    (W[7] * target + (1.0 - W[7]) * d_step).clamp(1.0, 10.0)
}

/// new stability after a *successful* recall (FSRS ratings 2, 3, or 4)
///
/// S′ = S * (e^w[8] * (11−D) · S^(−w[9]) * (e^(w[10]·(1−R)) − 1) + 1)
///        * hard_penalty * easy_bonus
///
/// hard_penalty = w[15] for rating == 2 (Hard), else 1.0
/// easy_bonus   = w[16] for rating == 4 (Easy), else 1.0
fn stability_after_recall(s: f64, d: f64, r: f64, rating: u8) -> f64 {
    let hard_penalty = if rating == 2 { W[15] } else { 1.0 };
    let easy_bonus   = if rating == 4 { W[16] } else { 1.0 };

    let sinc = W[8].exp()
        * (11.0 - d)
        * s.powf(-W[9])
        * ((W[10] * (1.0 - r)).exp() - 1.0);

    (s * (sinc * hard_penalty * easy_bonus + 1.0)).max(0.1)
}

/// New stability after a *lapse* (FSRS rating 1: Again)
///
/// S′ = w[11] * D^(−w[12]) * ((S+1)^w[13] − 1) * e^(w[14]·(1−R))
fn stability_after_lapse(s: f64, d: f64, r: f64) -> f64 {
    (W[11]
        * d.powf(-W[12])
        * ((s + 1.0).powf(W[13]) - 1.0)
        * (W[14] * (1.0 - r)).exp())
    .max(0.1)
}

/// Convert a target-retention probability and stability into a scheduled
/// interval in whole days (minimum 1)
/// Derived by inverting R(t, S) = retention:
///   (1 + FACTOR*t/S)^DECAY = retention
///   t = S/FACTOR * (retention^(1/DECAY) − 1)
fn next_interval(s: f64, retention: f64) -> i64 {
    let t = (s / FACTOR) * (retention.powf(1.0 / DECAY) - 1.0);
    (t.round() as i64).max(1)
}

// rating conversion

fn to_fsrs_rating(confidence: u8) -> u8 {
    confidence.clamp(1, 4)
}

// public interface

/// Apply a 1–5 confidence rating to `item` and return the updated version
///
/// First review / migration sentinel
/// `difficulty == 0.0` signals that the item has never been reviewed by FSRS
/// (brand-new card or a legacy SM-2 item being migrated).  The algorithm then
/// bootstraps `stability` and `difficulty` from the initial-state formulas
/// rather than the update formulas.  After this first review both fields carry
/// real FSRS values and all subsequent scheduling is fully algorithm-driven
///
/// Daily mode and chain re-exposure
/// `db::record_review` bypasses this function entirely for daily-mode items
/// and for chain-re-exposure reviews, so the step-chain unlock / de-unlock
/// logic is completely unaffected by this scheduler
pub fn apply_confidence(mut item: Item, confidence: u8, reviewed_at: DateTime<Utc>) -> Item {
    let rating = to_fsrs_rating(confidence);

    // days elapsed since the last honest FSRS review of this item
    let days_elapsed: f64 = item
        .last_reviewed_at
        .map(|t| (reviewed_at - t).num_days().max(0) as f64)
        .unwrap_or(0.0);

    let (new_stability, new_difficulty) = if item.difficulty == 0.0 {
        // Brand-new item (or SM-2 migration): bootstrap fresh FSRS state
        (init_stability(rating), init_difficulty(rating))
    } else {
        // Existing item: apply FSRS update formulas
        let r = retrievability(days_elapsed, item.stability);

        let new_s = if rating == 1 {
            stability_after_lapse(item.stability, item.difficulty, r)
        } else {
            stability_after_recall(item.stability, item.difficulty, r, rating)
        };

        let new_d = next_difficulty(item.difficulty, rating);
        (new_s, new_d)
    };

    // Lapses are only counted on non-new items rated Again.
    if confidence == 1 && item.difficulty != 0.0 {
        item.lapses += 1;
    }

    let interval = next_interval(new_stability, DESIRED_RETENTION);

    // Rolling average of raw 1–5 confidence scores (analytics only;
    // does not influence scheduling)
    item.review_count  += 1;
    let n               = item.review_count as f64;
    item.confidence_avg = if item.review_count == 1 {
        confidence as f64
    } else {
        (item.confidence_avg * (n - 1.0) + confidence as f64) / n
    };

    item.stability        = new_stability;
    item.difficulty       = new_difficulty;
    item.interval_days    = interval as f64;
    item.last_reviewed_at = Some(reviewed_at);
    item.due_at           = reviewed_at + Duration::days(interval);
    item
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ItemKind;

    fn blank_item() -> Item {
        Item {
            id:               "test-id".into(),
            card_id:          "card-id".into(),
            position:         1,
            kind:             ItemKind::Forward,
            prompt:           "q".into(),
            answer:           "a".into(),
            due_at:           Utc::now(),
            interval_days:    0.0,
            stability:        0.0,
            difficulty:       0.0,
            last_reviewed_at: None,
            lapses:           0,
            review_count:     0,
            confidence_avg:   0.0,
            image_path:       None,
            consecutive_fails: 0,
            consecutive_hards: 0,
            scaffold_state: crate::models::ScaffoldState::Normal,
            scaffold_passes: 0,
            scaffold_pass_date: None,
            weak_spans: Vec::new(),
        }
    }

    // the forgetting curve should give exactly DESIRED_RETENTION when t = S
    #[test]
    fn retrievability_at_stability_is_desired_retention() {
        let s = 10.0;
        let r = retrievability(s, s);
        assert!(
            (r - DESIRED_RETENTION).abs() < 1e-9,
            "expected R(S,S) ≈ 0.9, got {r}"
        );
    }

    // a brand-new item rated Good should produce valid FSRS state
    #[test]
    fn first_review_good_produces_valid_state() {
        let out = apply_confidence(blank_item(), 3, Utc::now());
        assert!(out.stability    >  0.0,  "stability must be positive");
        assert!(out.difficulty   >= 1.0,  "difficulty floor is 1.0");
        assert!(out.difficulty   <= 10.0, "difficulty ceiling is 10.0");
        assert!(out.interval_days >= 1.0, "interval must be ≥ 1 day");
        assert_eq!(out.review_count, 1);
        assert_eq!(out.lapses, 0);
    }

    // easy should always schedule further out than Good on the same fresh item
    #[test]
    fn easy_schedules_further_than_good() {
        let good = apply_confidence(blank_item(), 3, Utc::now());
        let easy = apply_confidence(blank_item(), 4, Utc::now());
        assert!(
            easy.interval_days > good.interval_days,
            "Easy interval ({}) should exceed Good interval ({})",
            easy.interval_days, good.interval_days
        );
    }

    // again on a previously-reviewed item should count as a lapse and produce
    // a shorter interval than the prior Good review
    #[test]
    fn again_after_good_creates_lapse_and_short_interval() {
        let after_good  = apply_confidence(blank_item(), 3, Utc::now());
        let good_ivl    = after_good.interval_days;
        let after_again = apply_confidence(after_good, 1, Utc::now());
        assert_eq!(after_again.lapses, 1, "one lapse should be recorded");
        assert!(
            after_again.interval_days < good_ivl,
            "lapse interval ({}) should be shorter than prior Good interval ({})",
            after_again.interval_days, good_ivl
        );
    }

    // difficulty should move in the correct direction for extreme ratings
    #[test]
    fn difficulty_decreases_on_easy_increases_on_again() {
        // seed a stable item with a known difficulty via a first Good review
        let seeded   = apply_confidence(blank_item(), 3, Utc::now());
        let d_base   = seeded.difficulty;

        let on_easy  = apply_confidence(seeded.clone(), 4, Utc::now());
        let on_again = apply_confidence(seeded,         1, Utc::now());

        assert!(
            on_easy.difficulty < d_base,
            "Easy should lower difficulty: {} → {}",
            d_base, on_easy.difficulty
        );
        assert!(
            on_again.difficulty > d_base,
            "Again should raise difficulty: {} → {}",
            d_base, on_again.difficulty
        );
    }

    // a brand-new item rated Again should NOT increment lapses (it is not yet
    // a lapse the item was simply answered wrong on its first exposure)
    #[test]
    fn first_review_again_does_not_count_as_lapse() {
        let out = apply_confidence(blank_item(), 1, Utc::now());
        assert_eq!(out.lapses, 0, "first Again should not be counted as a lapse");
        assert!(out.stability > 0.0);
    }

    // confidence average should reflect all ratings seen so far
    #[test]
    fn confidence_avg_is_rolling_mean() {
        let a = apply_confidence(blank_item(), 4, Utc::now()); // avg = 4.0
        let b = apply_confidence(a,            2, Utc::now()); // avg = 3.0
        assert!((b.confidence_avg - 3.0).abs() < 1e-9);
    }
}
