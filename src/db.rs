use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::time::Duration;
use uuid::Uuid;

use crate::models::*;
use crate::scheduler;

// ~~~ Store ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub struct Store {
    conn: Connection,
}

impl Store {
    // Open or create a database at path and run schema migrations
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path).context("open database")?;
        // foreign-key enforcement (SQLite disables it by default)
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .context("enable foreign keys")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    // ~~ Schema ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS cards (
                id          TEXT PRIMARY KEY,
                deck        TEXT NOT NULL,
                kind        TEXT NOT NULL,
                question    TEXT NOT NULL,
                reversible  INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS items (
                id               TEXT PRIMARY KEY,
                card_id          TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                position         INTEGER NOT NULL,
                kind             TEXT NOT NULL,
                prompt           TEXT NOT NULL,
                answer           TEXT NOT NULL,
                due_at           TEXT NOT NULL,
                interval_days    REAL NOT NULL DEFAULT 1.0,
                ease             REAL NOT NULL DEFAULT 2.5,
                last_reviewed_at TEXT,
                lapses           INTEGER NOT NULL DEFAULT 0,
                review_count     INTEGER NOT NULL DEFAULT 0,
                confidence_avg   REAL NOT NULL DEFAULT 0.0
            );

            CREATE INDEX IF NOT EXISTS idx_items_card_pos ON items(card_id, position);
            CREATE INDEX IF NOT EXISTS idx_items_due      ON items(due_at);

            CREATE TABLE IF NOT EXISTS review_log (
                id                     TEXT PRIMARY KEY,
                item_id                TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                card_id                TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                reviewed_at            TEXT NOT NULL,
                confidence             INTEGER NOT NULL,
                duration_ms            INTEGER NOT NULL,
                previous_interval_days REAL NOT NULL,
                new_interval_days      REAL NOT NULL
            );
            ",
            )
            .context("schema migration")?;
        Ok(())
    }

    // ~~ Seeding ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// Insert a sample card if the database is empty.
    pub fn seed_if_empty(&self) -> Result<()> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0))
            .context("seed check")?;
        if count == 0 {
            self.add_simple_card(
                "default",
                "What is a closure?",
                "A function that captures variables from its surrounding lexical scope.",
                true,
            )?;
        }
        Ok(())
    }

    // ~~ Card creation ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    pub fn add_simple_card(
        &self,
        deck: &str,
        question: &str,
        answer: &str,
        reversible: bool,
    ) -> Result<()> {
        let card_id = new_id();
        let now = Utc::now();
        let ts = now.to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO cards(id,deck,kind,question,reversible,created_at,updated_at)
                 VALUES(?1,?2,'simple',?3,?4,?5,?6)",
                params![card_id, deck, question, reversible as i32, ts, ts],
            )
            .context("insert simple card")?;

        self.insert_item(&card_id, 1, "forward", question, answer, now)?;
        if reversible {
            self.insert_item(&card_id, 2, "reverse", answer, question, now)?;
        }
        Ok(())
    }

    pub fn add_multi_card(&self, deck: &str, question: &str, steps: &[(String, String)]) -> Result<()> {
        if steps.is_empty() {
            anyhow::bail!("multi-step cards need at least one step");
        }
        let card_id = new_id();
        let now = Utc::now();
        let ts = now.to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO cards(id,deck,kind,question,reversible,created_at,updated_at)
                 VALUES(?1,?2,'multi',?3,0,?4,?5)",
                params![card_id, deck, question, ts, ts],
            )
            .context("insert multi card")?;

        let mut pos = 0usize;
        for (name, answer) in steps.iter().filter(|(_, a)| !a.trim().is_empty()) {
            pos += 1;
            let label = if name.trim().is_empty() {
                format!("Step {pos}")
            } else {
                name.trim().to_string()
            };
            self.insert_item(&card_id, pos as i32, "step", &label, answer, now)?;
        }
        Ok(())
    }

    fn insert_item(
        &self,
        card_id: &str,
        pos: i32,
        kind: &str,
        prompt: &str,
        answer: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let id = new_id();
        // set due_at just before now so every new item is immediately reviewable
        let due = (now - chrono::Duration::seconds(1)).to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO items(id,card_id,position,kind,prompt,answer,due_at,
                                   interval_days,ease,lapses,review_count,confidence_avg)
                 VALUES(?1,?2,?3,?4,?5,?6,?7, 1.0,2.5,0,0,0.0)",
                params![id, card_id, pos, kind, prompt, answer, due],
            )
            .context("insert item")?;
        Ok(())
    }

    // ~~ Querying ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    pub fn list_cards(&self, deck: Option<&str>) -> Result<Vec<CardSummary>> {
        let filter = deck.unwrap_or("");
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.id, c.kind, c.deck, c.question, c.reversible,
                        MIN(i.due_at) AS due_at, COUNT(i.id) AS item_count
                 FROM cards c
                 JOIN items i ON i.card_id = c.id
                 WHERE (?1 = '' OR c.deck = ?1)
                 GROUP BY c.id
                 ORDER BY due_at ASC, c.created_at ASC",
            )
            .context("prepare list_cards")?;

        let rows = stmt
            .query_map(params![filter], |row| {
                let kind: String = row.get(1)?;
                let rev: i32 = row.get(4)?;
                let due: String = row.get(5)?;
                Ok(CardSummary {
                    card_id: row.get(0)?,
                    kind: kind.parse().unwrap_or(CardKind::Simple),
                    deck: row.get(2)?,
                    question: row.get(3)?,
                    reversible: rev != 0,
                    item_count: row.get(6)?,
                    due_at: due.parse().unwrap_or_else(|_| Utc::now()),
                })
            })
            .context("query list_cards")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect list_cards")
    }

    // load all cards that have at least one due item and build a review session
    pub fn due_session(&self, deck: Option<&str>) -> Result<Vec<ReviewCard>> {
        let filter = deck.unwrap_or("");
        let now = Utc::now();

        let mut stmt = self
            .conn
            .prepare(
                "SELECT id,kind,deck,question,reversible,created_at,updated_at
                 FROM cards WHERE (?1='' OR deck=?1) ORDER BY created_at ASC",
            )
            .context("prepare due_session")?;

        let cards: Vec<Card> = stmt
            .query_map(params![filter], |row| {
                let kind: String = row.get(1)?;
                let rev: i32 = row.get(4)?;
                let ca: String = row.get(5)?;
                let ua: String = row.get(6)?;
                Ok(Card {
                    id: row.get(0)?,
                    kind: kind.parse().unwrap_or(CardKind::Simple),
                    deck: row.get(2)?,
                    question: row.get(3)?,
                    reversible: rev != 0,
                    created_at: ca.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: ua.parse().unwrap_or_else(|_| Utc::now()),
                })
            })
            .context("query cards")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("collect cards")?;

        let mut session = Vec::new();
        for card in cards {
            let items = self.load_items(&card.id)?;
            let selected = due_items_for_card(&card.kind, &items, now);
            if !selected.is_empty() {
                session.push(ReviewCard {
                    card,
                    items: selected,
                });
            }
        }
        Ok(session)
    }

    // Re-check a single card after a rating: returns an updated ReviewCard if the
    // card now has due items, or None if nothing is due. UI then dynamically
    // extends the running session queue
    pub fn review_card_if_due(&self, card_id: &str) -> Result<Option<ReviewCard>> {
        let now   = Utc::now();
        let card  = self.load_card(card_id)?;
        let items = self.load_items(card_id)?;
        let selected = due_items_for_card(&card.kind, &items, now);
        if selected.is_empty() {
            Ok(None) // if nothing is due make no changes
        } else {
            Ok(Some(ReviewCard { card, items: selected }))
        }
    }

    pub fn load_items(&self, card_id: &str) -> Result<Vec<Item>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id,card_id,position,kind,prompt,answer,due_at,
                        interval_days,ease,last_reviewed_at,lapses,review_count,confidence_avg
                 FROM items WHERE card_id=?1 ORDER BY position ASC",
            )
            .context("prepare load_items")?;

        let rows = stmt
            .query_map(params![card_id], |row| {
                let kind: String = row.get(3)?;
                let due: String = row.get(6)?;
                let last: Option<String> = row.get(9)?;
                Ok(Item {
                    id: row.get(0)?,
                    card_id: row.get(1)?,
                    position: row.get(2)?,
                    kind: kind.parse().unwrap_or(ItemKind::Forward),
                    prompt: row.get(4)?,
                    answer: row.get(5)?,
                    due_at: due.parse().unwrap_or_else(|_| Utc::now()),
                    interval_days: row.get(7)?,
                    ease: row.get(8)?,
                    last_reviewed_at: last.and_then(|s| s.parse().ok()),
                    lapses: row.get(10)?,
                    review_count: row.get(11)?,
                    confidence_avg: row.get(12)?,
                })
            })
            .context("query items")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect items")
    }

    pub fn load_card(&self, card_id: &str) -> Result<Card> {
        self.conn
            .query_row(
                "SELECT id,deck,kind,question,reversible,created_at,updated_at
                 FROM cards WHERE id=?1",
                params![card_id],
                |row| {
                    let kind: String = row.get(2)?;
                    let rev: i32 = row.get(4)?;
                    let ca: String = row.get(5)?;
                    let ua: String = row.get(6)?;
                    Ok(Card {
                        id: row.get(0)?,
                        deck: row.get(1)?,
                        kind: kind.parse().unwrap_or(CardKind::Simple),
                        question: row.get(3)?,
                        reversible: rev != 0,
                        created_at: ca.parse().unwrap_or_else(|_| Utc::now()),
                        updated_at: ua.parse().unwrap_or_else(|_| Utc::now()),
                    })
                },
            )
            .context("load card")
    }

    // ~~ Review recording ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    pub fn record_review(&self, item_id: &str, confidence: u8, duration: Duration) -> Result<()> {
        // Load the current item state
        let item = self
            .conn
            .query_row(
                "SELECT id,card_id,position,kind,prompt,answer,due_at,
                        interval_days,ease,last_reviewed_at,lapses,review_count,confidence_avg
                 FROM items WHERE id=?1",
                params![item_id],
                |row| {
                    let kind: String = row.get(3)?;
                    let due: String = row.get(6)?;
                    let last: Option<String> = row.get(9)?;
                    Ok(Item {
                        id: row.get(0)?,
                        card_id: row.get(1)?,
                        position: row.get(2)?,
                        kind: kind.parse().unwrap_or(ItemKind::Forward),
                        prompt: row.get(4)?,
                        answer: row.get(5)?,
                        due_at: due.parse().unwrap_or_else(|_| Utc::now()),
                        interval_days: row.get(7)?,
                        ease: row.get(8)?,
                        last_reviewed_at: last.and_then(|s| s.parse().ok()),
                        lapses: row.get(10)?,
                        review_count: row.get(11)?,
                        confidence_avg: row.get(12)?,
                    })
                },
            )
            .context("load item for review")?;

        let prev_interval = item.interval_days;
        let now = Utc::now();
        let updated = scheduler::apply_confidence(item, confidence, now);

        self.conn
            .execute(
                "UPDATE items
                 SET due_at=?1, interval_days=?2, ease=?3, last_reviewed_at=?4,
                     lapses=?5, review_count=?6, confidence_avg=?7
                 WHERE id=?8",
                params![
                    updated.due_at.to_rfc3339(),
                    updated.interval_days,
                    updated.ease,
                    updated.last_reviewed_at.map(|t| t.to_rfc3339()),
                    updated.lapses,
                    updated.review_count,
                    updated.confidence_avg,
                    updated.id,
                ],
            )
            .context("update item")?;

        // De-unlock subsequent steps when an earlier step is failed or barely
        // recalled (confidence ≤ 2).  Setting their due_at to now means
        // due_items_for_card will find them on the next session and, because it
        // always includes every step up to and including the earliest due one,
        // the learner is forced to rebuild the chain from the weak link forward.
        if updated.kind == ItemKind::Step && confidence <= 2 {
            self.conn
                .execute(
                    "UPDATE items
                     SET due_at = ?1
                     WHERE card_id = ?2
                       AND position > ?3
                       AND kind = 'step'",
                    params![
                        now.to_rfc3339(), // subsequent due now
                        //updated.due_at.to_rfc3339(), // syncs with the failed step
                        &updated.card_id,
                        updated.position,
                    ],
                )
                .context("de-unlock subsequent steps")?;
        }

        self.conn
            .execute(
                "INSERT INTO review_log(id,item_id,card_id,reviewed_at,confidence,duration_ms,
                                        previous_interval_days,new_interval_days)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    new_id(),
                    &updated.id,
                    &updated.card_id,
                    now.to_rfc3339(),
                    confidence as i32,
                    duration.as_millis() as i64,
                    prev_interval,
                    updated.interval_days,
                ],
            )
            .context("insert review_log")?;

        Ok(())
    }

    // ~~ Deck listing ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// Return every distinct deck name, sorted alphabetically.
    pub fn list_decks(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT deck FROM cards ORDER BY deck ASC")
            .context("prepare list_decks")?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .context("query list_decks")?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect list_decks")
    }

    // ~~ Deletion ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// Delete a single card (items and review_log cascade via FK).
    pub fn delete_card(&self, card_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM cards WHERE id=?1", params![card_id])
            .context("delete card")?;
        Ok(())
    }

    /// Delete every card (and their items/logs) in a deck.
    pub fn delete_deck(&self, deck: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM cards WHERE deck=?1", params![deck])
            .context("delete deck")?;
        Ok(())
    }

    // ~~ Stats & export ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    pub fn stats(&self) -> Result<Stats> {
        let now = Utc::now().to_rfc3339();
        Ok(Stats {
            cards: self
                .conn
                .query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0))
                .context("count cards")?,
            items: self
                .conn
                .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
                .context("count items")?,
            due_items: self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM items WHERE due_at <= ?1",
                    params![now],
                    |r| r.get(0),
                )
                .context("count due items")?,
            review_logs: self
                .conn
                .query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0))
                .context("count review_log")?,
        })
    }

    // returns the (due_at, question, deck) of the item due soonest
    pub fn next_due(&self) -> Result<Option<(DateTime<Utc>, String, String)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT i.due_at, c.question, c.deck
                FROM items i
                JOIN cards c ON c.id = i.card_id
                ORDER BY i.due_at ASC
                LIMIT 1",
            )
            .context("prepare next_due")?;

        match stmt.query_row([], |row| {
            let due: String      = row.get(0)?;
            let question: String = row.get(1)?;
            let deck: String     = row.get(2)?;
            Ok((due, question, deck))
        }) {
            Ok((due, question, deck)) => {
                let due_at = due.parse().unwrap_or_else(|_| Utc::now());
                Ok(Some((due_at, question, deck)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e).context("query next_due"),
        }
    }

    pub fn export_json(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Export {
            exported_at: DateTime<Utc>,
            cards: Vec<ExportCard>,
        }
        #[derive(Serialize)]
        struct ExportCard {
            card: Card,
            items: Vec<Item>,
        }

        let summaries = self.list_cards(None)?;
        let mut export_cards = Vec::with_capacity(summaries.len());
        for s in &summaries {
            let card = self.load_card(&s.card_id)?;
            let items = self.load_items(&s.card_id)?;
            export_cards.push(ExportCard { card, items });
        }

        serde_json::to_vec_pretty(&Export {
            exported_at: Utc::now(),
            cards: export_cards,
        })
        .context("serialize export")
    }

    // ~~ Editing ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// Update an existing simple card's content in-place.
    /// The forward item's SRS state is preserved; the reverse item is
    /// added/removed/updated to match the new `reversible` flag.
    pub fn update_simple_card(
        &self,
        card_id: &str,
        deck: &str,
        question: &str,
        answer: &str,
        reversible: bool,
    ) -> Result<()> {
        let now = Utc::now();
        let ts  = now.to_rfc3339();
        self.conn
            .execute(
                "UPDATE cards
                 SET deck=?1, question=?2, reversible=?3, updated_at=?4
                 WHERE id=?5",
                params![deck, question, reversible as i32, ts, card_id],
            )
            .context("update simple card")?;

        self.conn
            .execute(
                "UPDATE items SET prompt=?1, answer=?2
                 WHERE card_id=?3 AND kind='forward'",
                params![question, answer, card_id],
            )
            .context("update forward item")?;

        let reverse_exists: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE card_id=?1 AND kind='reverse'",
                params![card_id],
                |r| r.get::<_, i64>(0),
            )
            .context("check reverse item")?
            > 0;

        match (reversible, reverse_exists) {
            (true, true) => {
                self.conn
                    .execute(
                        "UPDATE items SET prompt=?1, answer=?2
                         WHERE card_id=?3 AND kind='reverse'",
                        params![answer, question, card_id],
                    )
                    .context("update reverse item")?;
            }
            (true, false) => {
                self.insert_item(card_id, 2, "reverse", answer, question, now)?;
            }
            (false, true) => {
                self.conn
                    .execute(
                        "DELETE FROM items WHERE card_id=?1 AND kind='reverse'",
                        params![card_id],
                    )
                    .context("delete reverse item")?;
            }
            (false, false) => {}
        }
        Ok(())
    }

    /// Update an existing multi-step card's content in-place.
    ///
    /// SRS state is preserved for steps whose **position** is unchanged.
    /// New steps are inserted fresh; steps that no longer exist are deleted.
    pub fn update_multi_card(
        &self,
        card_id: &str,
        deck: &str,
        question: &str,
        steps: &[(String, String)],
    ) -> Result<()> {
        let now = Utc::now();
        let ts  = now.to_rfc3339();
        self.conn
            .execute(
                "UPDATE cards SET deck=?1, question=?2, updated_at=?3 WHERE id=?4",
                params![deck, question, ts, card_id],
            )
            .context("update multi card")?;

        let existing_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE card_id=?1 AND kind='step'",
                params![card_id],
                |r| r.get(0),
            )
            .context("count step items")?;

        let valid: Vec<&(String, String)> = steps
            .iter()
            .filter(|(_, a)| !a.trim().is_empty())
            .collect();

        for (i, (name, answer)) in valid.iter().enumerate() {
            let pos = (i + 1) as i32;
            let label = if name.trim().is_empty() {
                format!("Step {}", i + 1)
            } else {
                name.trim().to_string()
            };
            if (i as i64) < existing_count {
                self.conn
                    .execute(
                        "UPDATE items SET prompt=?1, answer=?2
                         WHERE card_id=?3 AND position=?4 AND kind='step'",
                        params![label, answer, card_id, pos],
                    )
                    .context("update step item")?;
            } else {
                self.insert_item(card_id, pos, "step", &label, answer, now)?;
            }
        }

        // Remove steps that were deleted.
        let new_count = valid.len() as i32;
        self.conn
            .execute(
                "DELETE FROM items WHERE card_id=?1 AND kind='step' AND position > ?2",
                params![card_id, new_count],
            )
            .context("delete extra step items")?;

        Ok(())
    }
}

// ~~~ Helpers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

// Determine which items from a card should appear in a review session
// For `Simple` cards every individually-due item is included
//
// For `Multi` cards the logic is find the earliest due step by position, then
// include ALL steps up to and including it.  This forces the learner to rebuild
// the full reasoning chain from the start, not just practise the due step in
// isolation
fn due_items_for_card(kind: &CardKind, items: &[Item], now: DateTime<Utc>) -> Vec<Item> {
    match kind {
        CardKind::Multi => match items.iter().position(|it| it.due_at <= now) {
            Some(idx) => items[..=idx].to_vec(),
            None => vec![],
        },
        CardKind::Simple => items.iter().filter(|it| it.due_at <= now).cloned().collect(),
    }
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}
