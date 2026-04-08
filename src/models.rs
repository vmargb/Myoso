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

// ~~~ Core structs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub deck: String,
    pub kind: CardKind,
    pub question: String,
    pub reversible: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

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
}

/// Aggregate counts for the `stats` command.
#[derive(Debug, Default)]
pub struct Stats {
    pub cards: i64,
    pub items: i64,
    pub due_items: i64,
    pub review_logs: i64,
}
