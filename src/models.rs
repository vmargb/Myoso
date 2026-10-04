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
    /// cards are converted the first time they are saved from the tree-aware editor
    #[serde(default)]
    pub is_tree: bool,
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
    /// Display / sibling order. In a linear card this is the chain order. In a
    /// tree card it only orders siblings and gives a stable DFS order
    pub position: i32,
    /// Tree cards only `Card::is_tree` the step this one follows
    /// `None` = attached directly to the card's question (a root path)
    /// several items sharing one parent are alternative branches
    #[serde(default)]
    pub parent_id: Option<String>,
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
    /// a placeholder step, doesn't know the answer yet
    #[serde(default)]
    pub is_pass: bool,
}

/// A Simple card attached to a Multi card as a "margin" context clue
/// shown alongside the problem during review without being graded
#[derive(Debug, Clone)]
pub struct CardRef {
    pub id:       String,
    pub question: String,
    pub answer:   String,
}

/// A card together with the subset of items that are due (or needed for context).
#[derive(Debug, Clone)]
pub struct ReviewCard {
    pub card: Card,
    pub items: Vec<Item>,
    /// Simple cards attached to this card as margin context clues
    pub references: Vec<CardRef>,
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
    pub pass_count: i32,
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
    pub pass_steps: i64,
    pub pass_cards: i64,
}

/// drafts are passed to `Store::add_multi_card` / `update_multi_card` as a
/// slice *in DFS pre-order* (every step appears after its parent, and
/// siblings appear in the order they should be displayed in)
/// `key` is an editor-local identity used only to express `parent` inside
/// the slice. `db_id` is the existing `items.id` when editing (so SRS state
/// follows the step, not its position) and `None` for steps that are new
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepDraft {
    pub key:    u32,
    pub db_id:  Option<String>,
    /// `key` of the parent draft, `None` = child of the card's question
    pub parent: Option<u32>,
    pub name:   String,
    pub answer: String,
    pub image:  Option<String>,
    /// true only when this step was created through an explicit "start a
    /// branch" action
    pub branched: bool,
    /// a placeholder step with a prompt but no answer yet, see `Item::is_pass`
    pub is_pass: bool,
}

impl StepDraft {
    /// build a plain linear chain (each step the child of the previous one)
    /// from `(name, answer, image)` tuples, no `db_id`s
    #[allow(dead_code)]
    pub fn chain(steps: &[(String, String, Option<String>)]) -> Vec<StepDraft> {
        steps
            .iter()
            .enumerate()
            .map(|(i, (name, answer, image))| StepDraft {
                key: i as u32,
                db_id: None,
                parent: if i == 0 { None } else { Some(i as u32 - 1) },
                name: name.clone(),
                answer: answer.clone(),
                image: image.clone(),
                branched: false,
                is_pass: false,
            })
            .collect()
    }
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

// ~~~ Cloze markup ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// A cloze is not a separate kind of card or step: it is ordinary text that
// contains deletion markers. A step whose answer has markers is a cloze step,
// and a simple card whose question has markers is a cloze card. Nothing is
// stored besides the text, so export, import and editing all just work.
//
//   {{ln(x)}}          blank, revealed as "ln(x)"
//   {{c1::ln(x)}}      same (Anki style, the number is ignored)
//   {{ln(x)::a log}}   same, with a hint shown inside the blank
//   \{{               a literal "{{" (so code containing braces is safe)
//
// Every blank in a step is tested together, so one step is still one
// retrievable unit with one rating.

#[derive(Debug, Clone, PartialEq)]
pub enum ClozePart {
    Text(String),
    Blank { answer: String, hint: Option<String> },
}

/// split `{{c1::answer::hint}}` / `{{answer}}` contents into (answer, hint)
fn parse_cloze_inner(inner: &str) -> (String, Option<String>) {
    let mut body = inner.trim();
    // optional Anki-style "c<digits>::" prefix
    if let Some(rest) = body.strip_prefix('c') {
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && rest[digits..].starts_with("::") {
            body = rest[digits + 2..].trim();
        }
    }
    match body.split_once("::") {
        Some((a, h)) if !h.trim().is_empty() => (a.trim().to_string(), Some(h.trim().to_string())),
        Some((a, _)) => (a.trim().to_string(), None),
        None => (body.to_string(), None),
    }
}

pub fn cloze_parts(s: &str) -> Vec<ClozePart> {
    let mut parts: Vec<ClozePart> = Vec::new();
    let mut text = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        let Some(open) = rest.find("{{") else {
            text.push_str(rest);
            break;
        };
        // "\{{" is an escaped literal
        if rest[..open].ends_with('\\') {
            text.push_str(&rest[..open - 1]);
            text.push_str("{{");
            rest = &rest[open + 2..];
            continue;
        }
        let after = &rest[open + 2..];
        match after.find("}}") {
            Some(close) if !after[..close].trim().is_empty() && !after[..close].contains("{{") => {
                text.push_str(&rest[..open]);
                if !text.is_empty() {
                    parts.push(ClozePart::Text(std::mem::take(&mut text)));
                }
                let (answer, hint) = parse_cloze_inner(&after[..close]);
                parts.push(ClozePart::Blank { answer, hint });
                rest = &after[close + 2..];
            }
            _ => {
                // not a valid blank, keep the braces as plain text
                text.push_str(&rest[..open + 2]);
                rest = after;
            }
        }
    }
    if !text.is_empty() {
        parts.push(ClozePart::Text(text));
    }
    parts
}

/// does this text contain at least one blank?
pub fn has_cloze(s: &str) -> bool {
    s.contains("{{") && cloze_parts(s).iter().any(|p| matches!(p, ClozePart::Blank { .. }))
}

/// the text with every marker removed and the hidden words filled in
pub fn cloze_plain(s: &str) -> String {
    if !s.contains("{{") {
        return s.to_string();
    }
    cloze_parts(s)
        .into_iter()
        .map(|p| match p {
            ClozePart::Text(t) => t,
            ClozePart::Blank { answer, .. } => answer,
        })
        .collect()
}

#[cfg(test)]
mod cloze_tests {
    use super::*;

    #[test]
    fn plain_text_has_no_cloze() {
        assert!(!has_cloze("no blanks here"));
        assert_eq!(cloze_plain("no blanks here"), "no blanks here");
    }

    #[test]
    fn basic_anki_and_hint_forms_are_understood() {
        assert_eq!(
            cloze_parts("d/dx {{ln(x)}} = 1/x"),
            vec![
                ClozePart::Text("d/dx ".into()),
                ClozePart::Blank { answer: "ln(x)".into(), hint: None },
                ClozePart::Text(" = 1/x".into()),
            ]
        );
        assert_eq!(
            cloze_parts("{{c2::Paris::capital}}"),
            vec![ClozePart::Blank { answer: "Paris".into(), hint: Some("capital".into()) }]
        );
        assert_eq!(cloze_plain("a {{b}} c {{c1::d}}"), "a b c d");
    }

    #[test]
    fn unterminated_empty_and_escaped_braces_stay_literal() {
        assert!(!has_cloze("open {{ never closed"));
        assert!(!has_cloze("empty {{}} braces"));
        assert!(!has_cloze("{{   }}"));
        assert!(!has_cloze("literal \\{{x}} braces"));
        assert_eq!(cloze_plain("literal \\{{x}} braces"), "literal {{x}} braces");
    }

    #[test]
    fn blanks_may_span_lines_and_non_ascii_is_safe() {
        assert!(has_cloze("héllo {{wörld\nline}} ✓"));
        assert_eq!(cloze_plain("héllo {{wörld\nline}} ✓"), "héllo wörld\nline ✓");
    }
}
