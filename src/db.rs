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
                show_chain  INTEGER NOT NULL DEFAULT 1,
                review_mode TEXT NOT NULL DEFAULT 'spaced_repetition',
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
        // Safe migration for existing databases, add show_chain and markdown if absent
        let _ = self.conn.execute(
            "ALTER TABLE cards ADD COLUMN show_chain INTEGER NOT NULL DEFAULT 1",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE cards ADD COLUMN review_mode TEXT NOT NULL DEFAULT 'spaced_repetition'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN image_path TEXT",
            [],
        );
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
                None,
                &crate::models::ReviewMode::SpacedRepetition,
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
        image_path: Option<&str>,
        review_mode: &crate::models::ReviewMode,
    ) -> Result<()> {
        let card_id = new_id();
        let now = Utc::now();
        let ts = now.to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO cards(id,deck,kind,question,reversible,review_mode,created_at,updated_at)
                 VALUES(?1,?2,'simple',?3,?4,?5,?6,?7)",
                params![card_id, deck, question, reversible as i32, review_mode.as_str(), ts, ts],
            )
            .context("insert simple card")?;

        self.insert_item(&card_id, 1, "forward", question, answer, now, image_path)?;
        if reversible {
            self.insert_item(&card_id, 2, "reverse", answer, question, now, None)?;
        }
        Ok(())
    }

    // steps: (string, string, bool) -> (step name, answer, is_markdown)
    pub fn add_multi_card(&self, deck: &str, question: &str, steps: &[(String, String, Option<String>)], show_chain: bool, review_mode: &crate::models::ReviewMode) -> Result<()> {
        if steps.is_empty() {
            anyhow::bail!("multi-step cards need at least one step");
        }
        let card_id = new_id();
        let now = Utc::now();
        let ts = now.to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO cards(id,deck,kind,question,reversible,show_chain,review_mode,created_at,updated_at)
                 VALUES(?1,?2,'multi',?3,0,?4,?5,?6,?7)",
                params![card_id, deck, question, show_chain as i32, review_mode.as_str(), ts, ts],
            )
            .context("insert multi card")?;

        let mut pos = 0usize;
        for (name, answer, img) in steps.iter().filter(|(_, a, _)| !a.trim().is_empty()) {
            pos += 1;
            let label = if name.trim().is_empty() {
                format!("Step {pos}")
            } else {
                name.trim().to_string()
            };
            self.insert_item(&card_id, pos as i32, "step", &label, answer, now, img.as_deref())?;
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
        image_path: Option<&str>
    ) -> Result<()> {
        let id = new_id();
        // set due_at just before now so every new item is immediately reviewable
        let due = (now - chrono::Duration::seconds(1)).to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO items(id,card_id,position,kind,prompt,answer,due_at,
                                   interval_days,ease,lapses,review_count,confidence_avg,image_path)
                 VALUES(?1,?2,?3,?4,?5,?6,?7, 1.0,2.5,0,0,0.0,?8)",
                params![id, card_id, pos, kind, prompt, answer, due, image_path],
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
                        MIN(i.due_at) AS due_at, COUNT(i.id) AS item_count,
                        c.review_mode
                 FROM cards c
                 JOIN items i ON i.card_id = c.id
                 WHERE (?1 = '' OR c.deck = ?1 OR c.deck LIKE ?1 || '::%')
                 GROUP BY c.id
                 ORDER BY due_at ASC, c.created_at ASC",
            )
            .context("prepare list_cards")?;

        let rows = stmt
            .query_map(params![filter], |row| {
                let kind: String = row.get(1)?;
                let rev: i32 = row.get(4)?;
                let due: String = row.get(5)?;
                let rm: String  = row.get(7)?;
                Ok(CardSummary {
                    card_id: row.get(0)?,
                    kind: kind.parse().unwrap_or(CardKind::Simple),
                    deck: row.get(2)?,
                    question: row.get(3)?,
                    reversible: rev != 0,
                    item_count: row.get(6)?,
                    due_at: due.parse().unwrap_or_else(|_| Utc::now()),
                    review_mode: rm.parse().unwrap_or_default(),
                })
            })
            .context("query list_cards")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect list_cards")
    }

    // load all cards that have at least one due item and build a review session
    pub fn due_session(&self, deck: Option<&str>) -> Result<Vec<ReviewCard>> {
        let filter = deck.unwrap_or("");
        let now    = Utc::now();
        let today  = now.format("%Y-%m-%d").to_string();

        let all_cards = self.load_cards_for_session(filter)?;

        let (daily_cards, sr_cards): (Vec<Card>, Vec<Card>) = all_cards
            .into_iter()
            .partition(|c| c.review_mode.is_daily());

        let mut session = Vec::new();

        // ── Daily cards first ─────────────────────────────────────────────
        // Include ALL items; skip card only if every item was reviewed today.
        for card in daily_cards {
            let items = self.load_items(&card.id)?;
            let needs_review = items.iter().any(|it| {
                it.last_reviewed_at
                    .map(|t| t.format("%Y-%m-%d").to_string() != today)
                    .unwrap_or(true)
            });
            if needs_review {
                session.push(ReviewCard { card, items });
            }
        }

        // ── Spaced-repetition cards ───────────────────────────────────────
        for card in sr_cards {
            let items    = self.load_items(&card.id)?;
            let selected = due_items_for_card(&card.kind, &items, now);
            if !selected.is_empty() {
                session.push(ReviewCard { card, items: selected });
            }
        }

        Ok(session)
    }

    /// Load all cards in `filter` scope, ordered by creation time.
    fn load_cards_for_session(&self, filter: &str) -> Result<Vec<Card>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id,kind,deck,question,reversible,show_chain,created_at,updated_at,review_mode
                 FROM cards WHERE (?1='' OR deck=?1 OR deck LIKE ?1 || '::%') ORDER BY created_at ASC",
            )
            .context("prepare load_cards_for_session")?;

        let rows = stmt
            .query_map(params![filter], |row| {
                let kind: String = row.get(1)?;
                let rev: i32     = row.get(4)?;
                let sc: i32      = row.get(5)?;
                let ca: String   = row.get(6)?;
                let ua: String   = row.get(7)?;
                let rm: String   = row.get(8)?;
                Ok(Card {
                    id:          row.get(0)?,
                    kind:        kind.parse().unwrap_or(CardKind::Simple),
                    deck:        row.get(2)?,
                    question:    row.get(3)?,
                    reversible:  rev != 0,
                    show_chain:  sc  != 0,
                    created_at:  ca.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at:  ua.parse().unwrap_or_else(|_| Utc::now()),
                    review_mode: rm.parse().unwrap_or_default(),
                })
            })
            .context("query cards for session")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect cards for session")
    }

    // Re-check a single card after a rating: returns an updated ReviewCard if the
    // card now has due items, or None if nothing is due. UI then dynamically
    // extends the running session queue
    pub fn review_card_if_due(&self, card_id: &str) -> Result<Option<ReviewCard>> {
        let now   = Utc::now();
        let card  = self.load_card(card_id)?;
        let items = self.load_items(card_id)?;

        if card.review_mode.is_daily() {
            let today = now.format("%Y-%m-%d").to_string();
            let needs = items.iter().any(|it| {
                it.last_reviewed_at
                    .map(|t| t.format("%Y-%m-%d").to_string() != today)
                    .unwrap_or(true)
            });
            return Ok(if needs { Some(ReviewCard { card, items }) } else { None });
        }

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
                        interval_days,ease,last_reviewed_at,lapses,review_count,confidence_avg,image_path
                 FROM items WHERE card_id=?1 ORDER BY position ASC",
            )
            .context("prepare load_items")?;

        let rows = stmt
            .query_map(params![card_id], |row| {
                let kind: String = row.get(3)?;
                let due: String = row.get(6)?;
                let last: Option<String> = row.get(9)?;
                let image_path: Option<String> = row.get(13)?;
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
                    image_path
                })
            })
            .context("query items")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect items")
    }

    pub fn load_card(&self, card_id: &str) -> Result<Card> {
        self.conn
            .query_row(
                "SELECT id,deck,kind,question,reversible,show_chain,created_at,updated_at,review_mode
                 FROM cards WHERE id=?1",
                params![card_id],
                |row| {
                    let kind: String = row.get(2)?;
                    let rev: i32  = row.get(4)?;
                    let sc: i32   = row.get(5)?;
                    let ca: String = row.get(6)?;
                    let ua: String = row.get(7)?;
                    let rm: String = row.get(8)?;
                    Ok(Card {
                        id: row.get(0)?,
                        deck: row.get(1)?,
                        kind: kind.parse().unwrap_or(CardKind::Simple),
                        question: row.get(3)?,
                        reversible: rev != 0,
                        show_chain: sc != 0,
                        created_at: ca.parse().unwrap_or_else(|_| Utc::now()),
                        updated_at: ua.parse().unwrap_or_else(|_| Utc::now()),
                        review_mode: rm.parse().unwrap_or_default(),
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
                        interval_days,ease,last_reviewed_at,lapses,review_count,confidence_avg,image_path
                 FROM items WHERE id=?1",
                params![item_id],
                |row| {
                    let kind: String = row.get(3)?;
                    let due: String = row.get(6)?;
                    let last: Option<String> = row.get(9)?;
                    let image_path: Option<String> = row.get(13)?;
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
                        image_path
                    })
                },
            )
            .context("load item for review")?;

        // Check whether this card is in daily mode
        let mode_str: String = self
            .conn
            .query_row(
                "SELECT review_mode FROM cards WHERE id=?1",
                params![&item.card_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "spaced_repetition".to_string());
        let is_daily = mode_str == "daily";

        let prev_interval = item.interval_days;
        let now = Utc::now();

        let updated = if is_daily {
            // Daily mode: preserve SRS scheduling data entirely.
            // Only update review tracking fields so last_reviewed_at is today.
            let n      = item.review_count + 1;
            let new_avg = if n == 1 {
                confidence as f64
            } else {
                (item.confidence_avg * (n - 1) as f64 + confidence as f64) / n as f64
            };
            Item { review_count: n, confidence_avg: new_avg, last_reviewed_at: Some(now), ..item }
        } else {
            scheduler::apply_confidence(item, confidence, now)
        };

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

        // Step-chain de-unlock only applies to SR cards
        if !is_daily && updated.kind == ItemKind::Step && confidence <= 2 {
            self.conn
                .execute(
                    "UPDATE items
                     SET due_at = ?1
                     WHERE card_id = ?2
                       AND position > ?3
                       AND kind = 'step'",
                    params![
                        now.to_rfc3339(),
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

    /// Delete every card (and their items/logs) in a deck and all its sub-decks.
    pub fn delete_deck(&self, deck: &str) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM cards WHERE deck=?1 OR deck LIKE ?1 || '::%'",
                params![deck],
            )
            .context("delete deck")?;
        Ok(())
    }

    // ~~ Stats & export ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    pub fn stats(&self) -> Result<Stats> {
        let now = Utc::now().to_rfc3339();
        let today = Utc::now().format("%Y-%m-%d").to_string();
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
                    "SELECT COUNT(*) FROM items i
                     JOIN cards c ON c.id = i.card_id
                     WHERE c.review_mode != 'daily' AND i.due_at <= ?1",
                    params![now],
                    |r| r.get(0),
                )
                .context("count due items")?,
            review_logs: self
                .conn
                .query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0))
                .context("count review_log")?,
            daily_cards: self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM cards WHERE review_mode='daily'",
                    [],
                    |r| r.get(0),
                )
                .context("count daily cards")?,
            daily_due: self
                .conn
                .query_row(
                    "SELECT COUNT(DISTINCT c.id) FROM cards c
                     JOIN items i ON i.card_id = c.id
                     WHERE c.review_mode = 'daily'
                       AND (i.last_reviewed_at IS NULL
                            OR date(i.last_reviewed_at) < ?1)",
                    params![today],
                    |r| r.get(0),
                )
                .context("count daily due")?,
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
                WHERE c.review_mode != 'daily'
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

    pub fn export_json(&self, deck: Option<&str>, reset_metadata: bool) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Export {
            exported_at: DateTime<Utc>,
            reset_metadata: bool,
            deck: Option<String>,
            cards: Vec<ExportCard>,
        }
        #[derive(Serialize)]
        struct ExportCard {
            card: Card,
            items: Vec<Item>,
        }

        let summaries = self.list_cards(deck)?;
        let mut export_cards = Vec::with_capacity(summaries.len());
        let now = Utc::now();

        for s in &summaries {
            let card = self.load_card(&s.card_id)?;
            let mut items = self.load_items(&s.card_id)?;
            if reset_metadata {
                for item in &mut items {
                    item.due_at           = now - chrono::Duration::seconds(1);
                    item.interval_days    = 1.0;
                    item.ease             = 2.5;
                    item.last_reviewed_at = None;
                    item.lapses           = 0;
                    item.review_count     = 0;
                    item.confidence_avg   = 0.0;
                }
            }
            export_cards.push(ExportCard { card, items });
        }

        serde_json::to_vec_pretty(&Export {
            exported_at: now,
            reset_metadata,
            deck: deck.map(|s| s.to_string()),
            cards: export_cards,
        })
        .context("serialize export")
    }


    // ~~ Import ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// works for both full exports and single-deck exports
    /// cards/items where ID already exists in the DB are replaced
    pub fn import_json(&self, data: &[u8]) -> Result<ImportSummary> {
        #[derive(serde::Deserialize)]
        struct ImportFile {
            cards: Vec<ImportCard>,
        }
        #[derive(serde::Deserialize)]
        struct ImportCard {
            card:  Card,
            items: Vec<Item>,
        }

        let file: ImportFile =
            serde_json::from_slice(data).context("parse import JSON")?;

        let mut summary = ImportSummary::default();

        for ic in file.cards {
            // check if this card ID is already in the DB.
            let exists: bool = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM cards WHERE id=?1",
                    params![ic.card.id],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                > 0;

            self.conn
                .execute(
                    "INSERT OR REPLACE INTO cards
                        (id, deck, kind, question, reversible, show_chain, review_mode, created_at, updated_at)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![
                        ic.card.id,
                        ic.card.deck,
                        ic.card.kind.as_str(),
                        ic.card.question,
                        ic.card.reversible as i32,
                        ic.card.show_chain as i32,
                        ic.card.review_mode.as_str(),
                        ic.card.created_at.to_rfc3339(),
                        ic.card.updated_at.to_rfc3339(),
                    ],
                )
                .context("upsert card")?;

            if exists { summary.cards_replaced += 1; } else { summary.cards_imported += 1; }

            for item in ic.items {
                self.conn
                    .execute(
                        "INSERT OR REPLACE INTO items
                            (id, card_id, position, kind, prompt, answer, due_at,
                            interval_days, ease, last_reviewed_at,
                            lapses, review_count, confidence_avg, image_path)
                        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                        params![
                            item.id,
                            item.card_id,
                            item.position,
                            item.kind.as_str(),
                            item.prompt,
                            item.answer,
                            item.due_at.to_rfc3339(),
                            item.interval_days,
                            item.ease,
                            item.last_reviewed_at.map(|t| t.to_rfc3339()),
                            item.lapses,
                            item.review_count,
                            item.confidence_avg,
                            item.image_path
                        ],
                    )
                    .context("upsert item")?;
                summary.items_imported += 1;
            }
        }

        Ok(summary)
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
        image_path: Option<&str>,
        review_mode: &crate::models::ReviewMode,
    ) -> Result<()> {
        let now = Utc::now();
        let ts  = now.to_rfc3339();
        self.conn
            .execute(
                "UPDATE cards
                 SET deck=?1, question=?2, reversible=?3, review_mode=?4, updated_at=?5
                 WHERE id=?6",
                params![deck, question, reversible as i32, review_mode.as_str(), ts, card_id],
            )
            .context("update simple card")?;

        self.conn
            .execute(
                "UPDATE items SET prompt=?1, answer=?2, image_path=?3
                 WHERE card_id=?4 AND kind='forward'",
                params![question, answer, image_path, card_id],
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
                self.insert_item(card_id, 2, "reverse", answer, question, now, None)?;
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
        steps: &[(String, String, Option<String>)],
        show_chain: bool,
        review_mode: &crate::models::ReviewMode,
    ) -> Result<()> {
        let now = Utc::now();
        let ts  = now.to_rfc3339();
        self.conn
            .execute(
                "UPDATE cards SET deck=?1, question=?2, show_chain=?3, review_mode=?4, updated_at=?5 WHERE id=?6",
                params![deck, question, show_chain as i32, review_mode.as_str(), ts, card_id],
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

        let valid: Vec<&(String, String, Option<String>)> = steps
            .iter()
            .filter(|(_, a, _)| !a.trim().is_empty())
            .collect();

        for (i, (name, answer, img)) in valid.iter().enumerate() {
            let pos = (i + 1) as i32;
            let label = if name.trim().is_empty() {
                format!("Step {}", i + 1)
            } else {
                name.trim().to_string()
            };
            if (i as i64) < existing_count {
                self.conn
                    .execute(
                        "UPDATE items SET prompt=?1, answer=?2, image_path=?3
                         WHERE card_id=?4 AND position=?5 AND kind='step'",
                        params![label, answer, img, card_id, pos],
                    )
                    .context("update step item")?;
            } else {
                self.insert_item(card_id, pos, "step", &label, answer, now, img.as_deref())?;
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
    // ~~ Review-mode helpers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// Toggle a single card between Daily and Spaced Repetition.
    pub fn set_card_mode(&self, card_id: &str, mode: &crate::models::ReviewMode) -> Result<()> {
        self.conn
            .execute(
                "UPDATE cards SET review_mode=?1, updated_at=?2 WHERE id=?3",
                params![mode.as_str(), Utc::now().to_rfc3339(), card_id],
            )
            .context("set card mode")?;
        Ok(())
    }

    /// Set the review mode for every card in a deck (and all sub-decks).
    pub fn set_deck_mode(&self, deck: &str, mode: &crate::models::ReviewMode) -> Result<()> {
        self.conn
            .execute(
                "UPDATE cards SET review_mode=?1, updated_at=?2
                 WHERE deck=?3 OR deck LIKE ?3 || '::%'",
                params![mode.as_str(), Utc::now().to_rfc3339(), deck],
            )
            .context("set deck mode")?;
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
