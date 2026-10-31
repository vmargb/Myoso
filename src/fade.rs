// ~~~ fade.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// Adaptive worked-example fading
//
// A root -> leaf path has a `fade_boundary` (1-based step index), stored on the
// LEAF item of the path (`Item::fade_boundary`):
//
//     boundary = 3 on  S1 -> S2 -> S3 -> S4
//         S1, S2   worked  (prompt + answer shown, never rated)
//         S3, S4   tested  (answer hidden, rated as usual)
//
// Stored value of `Item::fade_boundary`:
//   None       never entered fading, existing Myoso behaviour until the path
//              first needs support (then it starts at `path_len`)
//   Some(N>=1) fading is active, steps N.. are tested
//   Some(0)    GRADUATED: every step is strong, back to legacy behaviour. The
//              path re-enters fading at its earliest weak step if it weakens
//
// Key invariant: fading changes what the user is asked to retrieve, it never
// fabricates evidence about what they remembered. Worked steps are never
// passed to `Store::record_review`.

use chrono::{DateTime, Utc};

use crate::models::Item;
use crate::tree::StepTree;

/// a step below this stability (days) needs support. Tunable, not a constant of
/// learning science
pub const REENTER_STABILITY_THRESHOLD: f64 = 7.0;
/// every step at or above this stability means the path no longer needs fading
pub const EXIT_STABILITY_THRESHOLD: f64 = 21.0;
/// stored boundary meaning "graduated out of fading"
pub const GRADUATED: i32 = 0;

// ~~~ weak / strong ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// NOTE: the handoff lists `lapses > 0` as a weakness signal and `lapses == 0`
// as an exit requirement. `Item::lapses` is a LIFETIME counter that is never
// reset, so using it would make a path that lapsed once unable to ever leave
// fading. A recent lapse is already visible through `consecutive_fails` (reset
// by the next Good) and through the post-lapse stability drop, so those two
// carry the signal instead.

/// placeholder steps have no answer and are never reviewed, they are ignored
/// when judging a path
fn counted(it: &Item) -> bool {
    !it.is_pass
}

pub fn is_weak(it: &Item) -> bool {
    it.review_count == 0
        || it.consecutive_fails > 0
        || it.stability < REENTER_STABILITY_THRESHOLD
}

pub fn is_strong(it: &Item) -> bool {
    it.review_count > 0
        && it.consecutive_fails == 0
        && it.stability >= EXIT_STABILITY_THRESHOLD
}

/// any real step is weak
pub fn needs_support(path: &[Item]) -> bool {
    path.iter().filter(|it| counted(it)).any(is_weak)
}

/// every real step is strong (and there is at least one)
pub fn is_graduated(path: &[Item]) -> bool {
    let mut any = false;
    for it in path.iter().filter(|it| counted(it)) {
        if !is_strong(it) {
            return false;
        }
        any = true;
    }
    any
}

/// 1-based position of the first weak step
pub fn earliest_weak(path: &[Item]) -> Option<usize> {
    path.iter()
        .position(|it| counted(it) && is_weak(it))
        .map(|i| i + 1)
}

// ~~~ per-path state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Is this root -> leaf path reviewed with fading right now? `Some(boundary)`
/// (1-based, clamped to the path) or `None` for legacy behaviour.
///
/// Evaluated from the stored boundary on the leaf plus current step state.
pub fn plan(path: &[Item]) -> Option<usize> {
    let len = path.len();
    let leaf = path.last()?;
    if is_graduated(path) {
        return None;
    }
    match leaf.fade_boundary {
        // never entered, but only if it needs support. A brand-new path starts
        // fully supported. A path that already has review history (a card from
        // before fading, or one that lapsed while reviewed normally) re-enters
        // at its earliest weak step, like a graduated path does
        None if !needs_support(path) => None,
        None if path.iter().any(|it| counted(it) && it.review_count > 0) => earliest_weak(path),
        None => Some(len),
        // graduated earlier: re-enter at the earliest weak step, if any
        Some(GRADUATED) => earliest_weak(path),
        // active: stays active until graduation (hysteresis between the
        // re-entry and exit thresholds)
        Some(b) => Some((b.max(1) as usize).min(len)),
    }
}

/// Boundary after a completed faded review, from the ratings of the TESTED
/// steps only. All Good/Easy moves the boundary back by exactly one step,
/// anything else (any Again or Hard) leaves it alone. Easy never skips ahead.
pub fn next_boundary(boundary: usize, tested_ratings: &[u8]) -> usize {
    let boundary = boundary.max(1);
    if !tested_ratings.is_empty() && tested_ratings.iter().all(|&r| r >= 3) {
        (boundary - 1).max(1)
    } else {
        boundary
    }
}

// ~~~ review paths ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// a path to walk in review, root -> target, plus its fade state
#[derive(Debug, Clone)]
pub struct PlannedPath {
    pub items: Vec<Item>,
    /// `Some(boundary)` = faded review, `None` = legacy
    pub fade: Option<usize>,
}

fn clone_path(tree: &StepTree, p: &[usize]) -> Vec<Item> {
    p.iter().map(|&i| tree.item(i).clone()).collect()
}

/// A fading path is due when any TESTED step is due. Worked steps are never
/// reviewed, so they stay "due" (review_count 0) and must not count here, or
/// the path would come back forever.
fn tested_region_due(path: &[Item], boundary: usize, now: DateTime<Utc>) -> bool {
    path.iter()
        .skip(boundary.saturating_sub(1))
        .any(|it| counted(it) && it.due_at <= now)
}

/// Spaced-repetition paths of one multi card.
///
///  * a path that is not fading is selected exactly as before: the frontier
///    (earliest due step) and the path root -> that step
///  * a fading path is the COMPLETE root -> leaf path, due when a tested step
///    is due
///  * a frontier step is dropped only when every leaf path through it is
///    fading, so a trunk shared with a legacy branch is still reviewed
pub fn plan_due_paths(is_tree: bool, items: &[Item], now: DateTime<Utc>) -> Vec<PlannedPath> {
    let tree = StepTree::new(is_tree, items);
    let leaves: Vec<(Vec<usize>, Option<usize>)> = tree
        .leaf_paths()
        .into_iter()
        .map(|p| {
            let f = plan(&clone_path(&tree, &p));
            (p, f)
        })
        .collect();

    let mut out = Vec::new();

    for t in tree.frontier(now) {
        let through: Vec<&Option<usize>> = leaves
            .iter()
            .filter(|(p, _)| p.contains(&t))
            .map(|(_, f)| f)
            .collect();
        if !through.is_empty() && through.iter().all(|f| f.is_some()) {
            continue;
        }
        out.push(PlannedPath { items: clone_path(&tree, &tree.path_to(t)), fade: None });
    }

    for (p, f) in &leaves {
        if let Some(b) = *f {
            let items = clone_path(&tree, p);
            if tested_region_due(&items, b, now) {
                out.push(PlannedPath { items, fade: Some(b) });
            }
        }
    }
    out
}

/// Daily-mode paths of one multi card. The existing "reviewed today" gate is
/// kept. For a fading path the gate is the LEAF (always tested), because worked
/// steps never get a review time and would otherwise keep the path due all day.
pub fn plan_daily_paths(is_tree: bool, items: &[Item], today: &str) -> Vec<PlannedPath> {
    let not_today = |it: &Item| {
        it.last_reviewed_at
            .map(|t| t.format("%Y-%m-%d").to_string() != today)
            .unwrap_or(true)
    };
    let tree = StepTree::new(is_tree, items);
    let mut out = Vec::new();
    for p in tree.leaf_paths() {
        let path = clone_path(&tree, &p);
        match plan(&path) {
            Some(b) => {
                if path.last().map_or(false, not_today) {
                    out.push(PlannedPath { items: path, fade: Some(b) });
                }
            }
            None => {
                if path.iter().any(not_today) {
                    out.push(PlannedPath { items: path, fade: None });
                }
            }
        }
    }
    out
}

/// Is any leaf path through `item_id` fading right now? Used to keep the
/// descendant de-unlock off while a branch is being faded. Must be called with
/// the state from BEFORE the rating being applied.
pub fn fading_through(is_tree: bool, items: &[Item], item_id: &str) -> bool {
    let tree = StepTree::new(is_tree, items);
    let Some(idx) = tree.index_of(item_id) else { return false };
    tree.leaf_paths()
        .into_iter()
        .filter(|p| p.contains(&idx))
        .any(|p| plan(&clone_path(&tree, &p)).is_some())
}

// ~~~ tests ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ItemKind;
    use chrono::Duration;

    fn it(id: &str, pos: i32, parent: Option<&str>) -> Item {
        Item {
            id: id.into(),
            card_id: "c".into(),
            position: pos,
            parent_id: parent.map(String::from),
            kind: ItemKind::Step,
            prompt: id.into(),
            answer: id.into(),
            due_at: Utc::now() - Duration::minutes(1),
            interval_days: 0.0,
            stability: 0.0,
            difficulty: 0.0,
            last_reviewed_at: None,
            lapses: 0,
            review_count: 0,
            confidence_avg: 0.0,
            image_path: None,
            consecutive_fails: 0,
            consecutive_hards: 0,
            is_pass: false,
            body: String::new(),
            heavy: false,
            fade_boundary: None,
        }
    }

    fn strong(mut i: Item) -> Item {
        i.review_count = 5;
        i.stability = 40.0;
        i.difficulty = 5.0;
        i.due_at = Utc::now() + Duration::days(30);
        i.last_reviewed_at = Some(Utc::now() - Duration::days(1));
        i
    }

    fn chain(n: usize) -> Vec<Item> {
        (0..n)
            .map(|k| it(&format!("s{}", k + 1), k as i32, if k == 0 { None } else { Some(Box::leak(format!("s{}", k).into_boxed_str())) }))
            .collect()
    }

    #[test]
    fn new_linear_path_starts_faded_with_only_the_last_step_tested() {
        let items = chain(4);
        let paths = plan_due_paths(true, &items, Utc::now());
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].fade, Some(4));
        assert_eq!(paths[0].items.len(), 4);
    }

    #[test]
    fn boundary_fades_back_one_step_per_successful_review() {
        let mut b = 4;
        for expect in [3, 2, 1, 1] {
            b = next_boundary(b, &[3]);
            assert_eq!(b, expect);
        }
        assert_eq!(next_boundary(3, &[4, 3]), 2, "Easy never skips a step");
    }

    #[test]
    fn again_and_hard_keep_the_boundary() {
        assert_eq!(next_boundary(3, &[1, 3, 3]), 3);
        assert_eq!(next_boundary(2, &[2, 3]), 2);
        assert_eq!(next_boundary(2, &[3, 3, 2]), 2);
        assert_eq!(next_boundary(2, &[]), 2, "no tested steps, no change");
    }

    #[test]
    fn a_never_faded_path_with_history_reenters_at_its_earliest_weak_step() {
        let mut items: Vec<Item> = chain(4).into_iter().map(strong).collect();
        items[1].stability = 2.0;
        items[1].consecutive_fails = 1;
        assert_eq!(items[3].fade_boundary, None);
        assert_eq!(plan(&items), Some(2));
    }

    #[test]
    fn mature_path_is_not_faded() {
        let items: Vec<Item> = chain(3).into_iter().map(strong).collect();
        assert_eq!(plan(&items), None);
        assert!(is_graduated(&items));
    }

    #[test]
    fn middling_stability_without_state_stays_legacy() {
        let mut items: Vec<Item> = chain(3).into_iter().map(strong).collect();
        for i in items.iter_mut() { i.stability = 10.0; }
        assert_eq!(plan(&items), None, "7..21 days and never entered: legacy");
        // but once fading is active it holds until every step is strong
        items.last_mut().unwrap().fade_boundary = Some(2);
        assert_eq!(plan(&items), Some(2));
        for i in items.iter_mut() { i.stability = 25.0; }
        assert_eq!(plan(&items), None);
    }

    #[test]
    fn lapse_reenters_at_the_earliest_weak_step_not_the_last() {
        let mut items: Vec<Item> = chain(4).into_iter().map(strong).collect();
        items[3].fade_boundary = Some(GRADUATED);
        items[1].stability = 3.0; // S2 forgotten
        items[1].consecutive_fails = 1;
        assert_eq!(plan(&items), Some(2));
        // S3 and S4 are not weak, so this is NOT "S1-S3 worked, S4 tested"
        items[2].stability = 3.0;
        items[3].stability = 3.0;
        assert_eq!(plan(&items), Some(2), "still the earliest weak step");
    }

    #[test]
    fn an_old_lapse_does_not_trap_a_path_in_fading() {
        let mut items: Vec<Item> = chain(3).into_iter().map(strong).collect();
        for i in items.iter_mut() { i.lapses = 2; }
        assert!(is_graduated(&items));
        assert_eq!(plan(&items), None);
    }

    #[test]
    fn stored_boundary_is_clamped_to_the_path() {
        let mut items = chain(3);
        items[2].fade_boundary = Some(9);
        assert_eq!(plan(&items), Some(3));
        items[2].fade_boundary = Some(-4);
        assert_eq!(plan(&items), Some(1));
    }

    #[test]
    fn worked_steps_never_keep_a_path_due() {
        // boundary 3: S1, S2 are due-and-unreviewed but only S3, S4 count
        let mut items = chain(4);
        items[3].fade_boundary = Some(3);
        let later = Utc::now() + Duration::days(2);
        items[2].due_at = later;
        items[3].due_at = later;
        items[2].review_count = 1;
        items[3].review_count = 1;
        items[2].stability = 2.0;
        items[3].stability = 2.0;
        assert!(plan_due_paths(true, &items, Utc::now()).is_empty());
        assert_eq!(plan_due_paths(true, &items, later + Duration::minutes(1)).len(), 1);
    }

    //  T1 -> T2 -> A1   and   T1 -> T2 -> B1
    fn forked() -> Vec<Item> {
        vec![
            it("t1", 0, None),
            it("t2", 1, Some("t1")),
            it("a1", 2, Some("t2")),
            it("b1", 3, Some("t2")),
        ]
    }

    #[test]
    fn branches_have_independent_boundaries() {
        let mut items = forked();
        items[2].fade_boundary = Some(3);
        items[3].fade_boundary = Some(2);
        let paths = plan_due_paths(true, &items, Utc::now());
        assert_eq!(paths.len(), 2);
        let a = paths.iter().find(|p| p.items.last().unwrap().id == "a1").unwrap();
        let b = paths.iter().find(|p| p.items.last().unwrap().id == "b1").unwrap();
        assert_eq!(a.fade, Some(3));
        assert_eq!(b.fade, Some(2));
    }

    #[test]
    fn a_legacy_branch_beside_a_fading_one_keeps_the_shared_trunk_reviewable() {
        // B is mature and its leaf is the only due step; A is still new
        let mut items = forked();
        let far = Utc::now() + Duration::days(30);
        for i in [0usize, 1, 3] { items[i] = strong(items[i].clone()); }
        items[3].due_at = Utc::now() - Duration::minutes(1);
        let _ = far;
        let paths = plan_due_paths(true, &items, Utc::now());
        // B1 is the legacy target, A is fading
        assert!(paths.iter().any(|p| p.fade.is_none() && p.items.last().unwrap().id == "b1"));
        assert!(paths.iter().any(|p| p.fade.is_some() && p.items.last().unwrap().id == "a1"));
    }

    #[test]
    fn fading_through_sees_only_paths_that_contain_the_step() {
        let mut items = forked();
        for i in [0usize, 1, 3] { items[i] = strong(items[i].clone()); }
        // A1 is new, so anything on its path (T1, T2, A1) is on a fading path
        assert!(fading_through(true, &items, "t2"));
        assert!(fading_through(true, &items, "a1"));
        // B1 only lies on the mature path
        assert!(!fading_through(true, &items, "b1"));
    }

    #[test]
    fn daily_gate_is_the_leaf_for_a_fading_path() {
        let mut items = chain(3);
        let today = Utc::now().format("%Y-%m-%d").to_string();
        assert_eq!(plan_daily_paths(true, &items, &today).len(), 1);
        // leaf reviewed today, worked steps never were: not offered again
        items[2].last_reviewed_at = Some(Utc::now());
        assert!(plan_daily_paths(true, &items, &today).is_empty());
    }

    #[test]
    fn pass_steps_do_not_count_as_weak_or_strong() {
        let mut items: Vec<Item> = chain(3).into_iter().map(strong).collect();
        items[1].is_pass = true;
        items[1].stability = 0.0;
        items[1].review_count = 0;
        assert!(is_graduated(&items));
        assert_eq!(plan(&items), None);
    }
}
