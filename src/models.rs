use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ~~~ Card kind ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    Simple,
    Multi,
}

impl CardKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CardKind::Simple => "simple",
            CardKind::Multi => "multi",
        }
    }
}

impl fmt::Display for CardKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CardKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "simple" => Ok(CardKind::Simple),
            "multi"  => Ok(CardKind::Multi),
            _        => Err(format!("unknown card kind: {s}")),
        }
    }
}

// ~~~ Item kind ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Forward,
    Reverse,
    Step,
}

impl ItemKind {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemKind::Forward => "forward",
            ItemKind::Reverse => "reverse",
            ItemKind::Step    => "step",
        }
    }
}

impl FromStr for ItemKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "forward" => Ok(ItemKind::Forward),
            "reverse" => Ok(ItemKind::Reverse),
            "step" => Ok(ItemKind::Step),
            _ => Err(format!("unknown item kind: {s}")),
        }
    }
}

// ~~~ weak-step handling: leech status ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// computed at read time in `db::Store::record_review` right after an item's
// consecutive-fail/hard counters are updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeechStatus {
    None,
    /// Consecutive-fail streak
    Leech,
    /// Consecutive-hard streak only: the step is known but effortful, not
    /// forgotten. Surfaces a passive, non-blocking rename/split nudge
    /// never the blocking prompt, and never offers scaffolding
    HardStreak,
}

// ~~~ weak-step handling: scaffolding ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScaffoldState {
    #[default]
    Normal,
    Scaffolded,
}

impl ScaffoldState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScaffoldState::Normal     => "normal",
            ScaffoldState::Scaffolded => "scaffolded",
        }
    }
    pub fn is_scaffolded(&self) -> bool { *self == ScaffoldState::Scaffolded }
}

impl fmt::Display for ScaffoldState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ScaffoldState {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "scaffolded" => Ok(ScaffoldState::Scaffolded),
            _            => Ok(ScaffoldState::Normal), // graceful fallback
        }
    }
}

/// single (phrase, weight) entry in an item's cloze-deletion span pool
/// the literal marked substring is stored, not character offsets
/// if user edits the step's answer later, offsets would silently rot
/// substring needle just needs a `.contains()` at render time, and if it no
/// longer matches, it's dropped from the render set rather than an error
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeakSpan {
    pub phrase: String,
    pub weight: u32,
}

// ~~~ Review mode ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMode {
    #[default]
    SpacedRepetition,
    Daily,
}

impl ReviewMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewMode::SpacedRepetition => "spaced_repetition",
            ReviewMode::Daily            => "daily",
        }
    }
    pub fn is_daily(&self) -> bool { *self == ReviewMode::Daily }
}

impl fmt::Display for ReviewMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ReviewMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "daily" => Ok(ReviewMode::Daily),
            _       => Ok(ReviewMode::SpacedRepetition), // graceful fallback
        }
    }
}

// ~~~ Core structs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub deck: String,
    pub kind: CardKind,
    pub question: String,
    pub reversible: bool,
    #[serde(default = "default_show_chain")]
    pub show_chain: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub review_mode: ReviewMode,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_show_chain() -> bool { true }

/// A single reviewable unit within a card.
///
/// # FSRS scheduling fields
/// - `stability`  – FSRS memory stability S (days until ~90 % retention).
///                  Stored in the `ease` DB column for schema compatibility.
/// - `difficulty` – FSRS item difficulty D (1–10 scale internally).
///                  Stored in the newly-added `difficulty` DB column.
///
/// Items that have never been reviewed by FSRS carry `stability == 0.0` and
/// `difficulty == 0.0`; the scheduler uses this as a sentinel to bootstrap
/// fresh FSRS state on the first rating rather than treating stale SM-2
/// `ease` values as valid FSRS stability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub card_id: String,
    pub position: i32,
    pub kind: ItemKind,
    pub prompt: String,
    pub answer: String,
    pub due_at: DateTime<Utc>,
    pub interval_days: f64,
    /// FSRS memory stability S.  Serialised as `"ease"` for backward
    /// compatibility with existing JSON exports.
    #[serde(rename = "ease", alias = "stability")]
    pub stability: f64,
    /// FSRS item difficulty D.  Defaults to 0.0 in older exports / DB rows,
    /// which triggers a fresh FSRS bootstrap on the next review.
    #[serde(default)]
    pub difficulty: f64,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub lapses: i32,
    pub review_count: i32,
    pub confidence_avg: f64,
    #[serde(default)]
    pub image_path: Option<String>,
    /// rolling count of consecutive `Again` (confidence == 1) ratings.
    /// Reset to 0 on any confidence >= 3 rating
    #[serde(default)]
    pub consecutive_fails: i32, // leach detection
    /// Rolling count of consecutive `Hard` (confidence == 2) ratings.
    /// Reset to 0 on any confidence >= 3 rating
    #[serde(default)]
    pub consecutive_hards: i32,
    #[serde(default)]
    pub scaffold_state: ScaffoldState,
    /// daily-capped count of scaffolded passes toward graduation back to normal
    #[serde(default)]
    pub scaffold_passes: i32,
    /// Date (RFC3339 date-only, e.g. "2026-08-13") of the last scaffolded
    #[serde(default)]
    pub scaffold_pass_date: Option<chrono::NaiveDate>,
    /// Cloze-deletion span pool, substrings of `answer` the user has
    /// identified as their own blind spot, with a weight that increases
    /// each time the same phrase is marked again. Rendered highest-weight
    /// first when the item is scaffolded.
    #[serde(default)]
    pub weak_spans: Vec<WeakSpan>,
}

/// A card together with the subset of items that are due (or needed for context).
#[derive(Debug, Clone)]
pub struct ReviewCard {
    pub card: Card,
    pub items: Vec<Item>,
}

/// Lightweight row returned by `list_cards`.
#[derive(Debug, Clone)]
pub struct CardSummary {
    pub card_id: String,
    pub kind: CardKind,
    pub deck: String,
    pub question: String,
    #[allow(dead_code)]
    pub reversible: bool,
    pub item_count: i32,
    pub due_at: DateTime<Utc>,
    pub review_mode: ReviewMode,
    pub tags: Vec<String>,
    // space-joined item answers used for answer-scope searching
    pub answers_text: String,
}

/// Aggregate counts for the `stats` command.
#[derive(Debug, Default)]
pub struct Stats {
    pub cards: i64,
    pub items: i64,
    pub due_items: i64,
    pub review_logs: i64,
    pub daily_cards: i64,
    pub daily_due: i64,
}

#[derive(Debug, Default)]
pub struct ImportSummary {
    pub cards_imported: usize,
    pub items_imported: usize,
    pub cards_replaced: usize,  // cards whose ID already existed
}

// ~~~ Session limits ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// controls how many cards enter a single review session to not overwhelm you
//
// `due_session` splits SR cards into two buckets:
//   reviews – cards where at least one item has been reviewed before
//             capped by `max_reviews` (default 200)
//   new – cards where every item still has `review_count == 0`
//         capped by `new_cards` (default 20)
//
// daily-mode cards are never subject to these limits (hence dailies)
// `SessionLimits::unlimited()` to remove the cap entirely
#[derive(Debug, Clone)]
pub struct SessionLimits {
    // maximum review cards shown per session
    pub max_reviews: usize,
    // maximum new cards introduced per session
    pub new_cards: usize,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            max_reviews: 200,
            new_cards:   20,
        }
    }
}

// Use this later for unlimited flag
// impl SessionLimits {
//     pub fn new(max_reviews: usize, new_cards: usize) -> Self {
//         Self { max_reviews, new_cards }
//     }
//     // removes both caps, equivalent to setting 9999 in Anki's deck options
//     pub fn unlimited() -> Self {
//         Self { max_reviews: usize::MAX, new_cards: usize::MAX }
//     }
// }

// ~~~ Sub-deck ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// deck names use '::' separators: "Math::Calculus::Integrals"
// no schema changes since the deck column stores the full path as TEXT

/// (0 = top-level, 1 = one sub-level, …)
pub fn deck_depth(deck: &str) -> usize {
    deck.split("::").count().saturating_sub(1)
}

/// ("Math::Calc::Limits" = "Limits")
pub fn deck_leaf(deck: &str) -> &str {
    deck.rsplit("::").next().unwrap_or(deck)
}

/// "Math::Calculus::Integrals" -> Some("Math::Calculus")
/// "Math"                      -> None
pub fn deck_parent(deck: &str) -> Option<&str> {
    deck.rfind("::").map(|i| &deck[..i])
}
