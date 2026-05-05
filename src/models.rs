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
            "multi" => Ok(CardKind::Multi),
            _ => Err(format!("unknown card kind: {s}")),
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
            ItemKind::Step => "step",
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
}

fn default_show_chain() -> bool { true }

/// A single reviewable unit within a card.
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
    pub ease: f64,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub lapses: i32,
    pub review_count: i32,
    pub confidence_avg: f64,
    #[serde(default)]
    pub image_path: Option<String>,
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
