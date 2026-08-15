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
                interval_days    REAL NOT NULL DEFAULT 0.0,
                -- 'ease' column now stores FSRS memory stability (S) instead
                -- the column is kept as 'ease' just for schema compatibility
                ease             REAL NOT NULL DEFAULT 0.0,
                last_reviewed_at TEXT,
                lapses           INTEGER NOT NULL DEFAULT 0,
                review_count     INTEGER NOT NULL DEFAULT 0,
                confidence_avg   REAL NOT NULL DEFAULT 0.0,
                image_path       TEXT,
                -- FSRS item difficulty (D) 0.0 = never reviewed by FSRS
                -- bootstrapped fresh on next rating
                difficulty       REAL NOT NULL DEFAULT 0.0,
                -- weak-step handling rolling consecutive-rating
                -- counters, distinct from lifetime lapses
                -- scheduler::LEECH_STREAK_THRESHOLD
                consecutive_fails INTEGER NOT NULL DEFAULT 0,
                consecutive_hards INTEGER NOT NULL DEFAULT 0,
                -- weak-step handling (Phase 2): scaffolding
                scaffold_state     TEXT NOT NULL DEFAULT 'normal',
                scaffold_passes    INTEGER NOT NULL DEFAULT 0,
                weak_spans         TEXT NOT NULL DEFAULT '[]',
                scaffold_pass_date TEXT
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
                new_interval_days      REAL NOT NULL,
                chain_reexposure       INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS tags (
                id   TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS card_tags (
                card_id TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                tag_id  TEXT NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
                PRIMARY KEY (card_id, tag_id)
            );

            CREATE INDEX IF NOT EXISTS idx_card_tags_card ON card_tags(card_id);
            CREATE INDEX IF NOT EXISTS idx_card_tags_tag  ON card_tags(tag_id);
            ",
            )
            .context("schema migration")?;

        // safe migrations for databases that predate a given column
        // each ALTER TABLE is intentionally allowed to fail silently when the
        // column already exists
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
        let _ = self.conn.execute(
            "ALTER TABLE review_log ADD COLUMN chain_reexposure INTEGER NOT NULL DEFAULT 0",
            [],
        );
        // FSRS migration, add the difficulty column
        // existing rows get difficulty = 0.0, which the scheduler treats as
        // "never reviewed by FSRS" and bootstraps  afresh state on next rating
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN difficulty REAL NOT NULL DEFAULT 0.0",
            [],
        );
        // weak-step handling leech-detection counters
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN consecutive_fails INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN consecutive_hards INTEGER NOT NULL DEFAULT 0",
            [],
        );
        // Weak-step handling Phase 2: scaffolding.
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN scaffold_state TEXT NOT NULL DEFAULT 'normal'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN scaffold_passes INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN weak_spans TEXT NOT NULL DEFAULT '[]'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN scaffold_pass_date TEXT",
            [],
        );

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
    ) -> Result<String> {
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
        Ok(card_id)
    }

    // steps: (step name, answer, optional image path)
    pub fn add_multi_card(
        &self,
        deck: &str,
        question: &str,
        steps: &[(String, String, Option<String>)],
        show_chain: bool,
        review_mode: &crate::models::ReviewMode,
    ) -> Result<String> {
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
            self.insert_item(&card_id, pos as i32, "step", label.as_str(), answer, now, img.as_deref())?;
        }
        Ok(card_id)
    }

    fn insert_item(
        &self,
        card_id: &str,
        pos: i32,
        kind: &str,
        prompt: &str,
        answer: &str,
        now: DateTime<Utc>,
        image_path: Option<&str>,
    ) -> Result<()> {
        let id = new_id();
        // due_at is set just before now so every new item is immediately reviewable
        // interval_days, ease (stability), and difficulty all start at 0.0, so the
        // scheduler treats difficulty == 0.0 as "never reviewed by FSRS" and will
        // bootstrap fresh state on the first rating.
        let due = (now - chrono::Duration::seconds(1)).to_rfc3339();
        self.conn
            .execute(
                "INSERT INTO items(id,card_id,position,kind,prompt,answer,due_at,
                                   interval_days,ease,lapses,review_count,confidence_avg,image_path,difficulty,
                                   consecutive_fails,consecutive_hards,
                                   scaffold_state,scaffold_passes,weak_spans,scaffold_pass_date)
                 VALUES(?1,?2,?3,?4,?5,?6,?7, 0.0,0.0,0,0,0.0,?8,0.0, 0,0,
                        'normal',0,'[]',NULL)",
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
                        MIN(i.due_at) AS due_at, COUNT(DISTINCT i.id) AS item_count,
                        c.review_mode,
                        GROUP_CONCAT(DISTINCT t.name) AS tag_names,
                        GROUP_CONCAT(i.answer, ' ')   AS answers_text
                FROM cards c
                JOIN items i ON i.card_id = c.id
                LEFT JOIN card_tags ct ON ct.card_id = c.id
                LEFT JOIN tags t ON t.id = ct.tag_id
                WHERE (?1 = '' OR c.deck = ?1 OR c.deck LIKE ?1 || '::%')
                GROUP BY c.id
                ORDER BY due_at ASC, c.created_at ASC",
            )
            .context("prepare list_cards")?;

        let rows = stmt
            .query_map(params![filter], |row| {
                let kind: String            = row.get(1)?;
                let rev: i32                = row.get(4)?;
                let due: String             = row.get(5)?;
                let rm: String              = row.get(7)?;
                let tag_csv: Option<String> = row.get(8)?;
                let answers: Option<String> = row.get(9)?;
                let tags = tag_csv
                    .map(|s| s.split(',').map(|t| t.to_string()).collect::<Vec<_>>())
                    .unwrap_or_default();
                Ok(CardSummary {
                    card_id:      row.get(0)?,
                    kind:         kind.parse().unwrap_or(CardKind::Simple),
                    deck:         row.get(2)?,
                    question:     row.get(3)?,
                    reversible:   rev != 0,
                    item_count:   row.get(6)?,
                    due_at:       due.parse().unwrap_or_else(|_| Utc::now()),
                    review_mode:  rm.parse().unwrap_or_default(),
                    tags,
                    answers_text: answers.unwrap_or_default(),
                })
            })
            .context("query list_cards")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect list_cards")
    }

    // load all cards that have at least one due item and build a review session
    // `limits' enforces Anki-style per-session caps:
    //    up to `limits.max_reviews` SR cards that have been seen before
    //    up to `limits.new_cards`   SR cards being introduced for the FIRST time
    // the caps don't apply to daily mode cards
    // pass `SessionLimits::unlimited()' to get unlimited
    pub fn due_session(&self, deck: Option<&str>, limits: &SessionLimits) -> Result<Vec<ReviewCard>> {
        let filter = deck.unwrap_or("");
        let now    = Utc::now();
        let today  = now.format("%Y-%m-%d").to_string();

        let all_cards = self.load_cards_for_session(filter)?;

        let (daily_cards, sr_cards): (Vec<Card>, Vec<Card>) = all_cards
            .into_iter()
            .partition(|c| c.review_mode.is_daily());

        let mut session = Vec::new();

        // daily cards first
        // include ALL items, skip card only if every item was reviewed today
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

        // SRS cards (not dailies)
        // split due SR cards into two buckets before enforcing caps:
        //  reviews – at least one item in the card has been reviewed before
        //             These are retention-critical and fill the session first
        //  new     – every item still has review_count == 0 (never introduced)
        //             New cards are appended after reviews, up to the new-card cap
        //
        // The "is new" test looks at all items, not just the due subset, so a
        // multi-step card where step 1 has been reviewed but step 2 is newly due
        // is correctly classified as a review card, not a new card
        let mut sr_reviews: Vec<ReviewCard> = Vec::new();
        let mut sr_new:     Vec<ReviewCard> = Vec::new();

        for card in sr_cards {
            let items    = self.load_items(&card.id)?;
            let selected = due_items_for_card(&card.kind, &items, now);
            if selected.is_empty() { continue; }

            if items.iter().all(|it| it.review_count == 0) {
                sr_new.push(ReviewCard { card, items: selected });
            } else {
                sr_reviews.push(ReviewCard { card, items: selected });
            }
        }

        // Apply caps then merge: reviews before new cards
        sr_reviews.truncate(limits.max_reviews);
        sr_new.truncate(limits.new_cards);

        session.extend(sr_reviews);
        session.extend(sr_new);

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
                    tags:        vec![],
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
                // Column indices (0-based):
                //  0 id | 1 card_id | 2 position | 3 kind | 4 prompt | 5 answer |
                //  6 due_at | 7 interval_days | 8 ease(stability) |
                //  9 last_reviewed_at | 10 lapses | 11 review_count |
                //  12 confidence_avg | 13 image_path | 14 difficulty |
                //  15 consecutive_fails | 16 consecutive_hards |
                //  17 scaffold_state | 18 scaffold_passes | 19 weak_spans |
                //  20 scaffold_pass_date
                "SELECT id, card_id, position, kind, prompt, answer, due_at,
                        interval_days, ease, last_reviewed_at, lapses, review_count,
                        confidence_avg, image_path, difficulty,
                        consecutive_fails, consecutive_hards,
                        scaffold_state, scaffold_passes, weak_spans, scaffold_pass_date
                 FROM items WHERE card_id=?1 ORDER BY position ASC",
            )
            .context("prepare load_items")?;

        let rows = stmt
            .query_map(params![card_id], |row| {
                let kind: String             = row.get(3)?;
                let due: String              = row.get(6)?;
                let last: Option<String>     = row.get(9)?;
                let image_path: Option<String> = row.get(13)?;
                let scaffold_state: String   = row.get(17)?;
                let weak_spans: String       = row.get(19)?;
                let scaffold_pass_date: Option<String> = row.get(20)?;
                Ok(Item {
                    id:               row.get(0)?,
                    card_id:          row.get(1)?,
                    position:         row.get(2)?,
                    kind:             kind.parse().unwrap_or(ItemKind::Forward),
                    prompt:           row.get(4)?,
                    answer:           row.get(5)?,
                    due_at:           due.parse().unwrap_or_else(|_| Utc::now()),
                    interval_days:    row.get(7)?,
                    stability:        row.get(8)?,
                    last_reviewed_at: last.and_then(|s| s.parse().ok()),
                    lapses:           row.get(10)?,
                    review_count:     row.get(11)?,
                    confidence_avg:   row.get(12)?,
                    image_path,
                    difficulty:       row.get(14)?,
                    consecutive_fails: row.get(15)?,
                    consecutive_hards: row.get(16)?,
                    scaffold_state:   scaffold_state.parse().unwrap_or_default(),
                    scaffold_passes:  row.get(18)?,
                    weak_spans:       parse_weak_spans(&weak_spans),
                    scaffold_pass_date: scaffold_pass_date
                        .and_then(|s| chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
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
                    let rev: i32     = row.get(4)?;
                    let sc: i32      = row.get(5)?;
                    let ca: String   = row.get(6)?;
                    let ua: String   = row.get(7)?;
                    let rm: String   = row.get(8)?;
                    Ok(Card {
                        id:          row.get(0)?,
                        deck:        row.get(1)?,
                        kind:        kind.parse().unwrap_or(CardKind::Simple),
                        question:    row.get(3)?,
                        reversible:  rev != 0,
                        show_chain:  sc != 0,
                        created_at:  ca.parse().unwrap_or_else(|_| Utc::now()),
                        updated_at:  ua.parse().unwrap_or_else(|_| Utc::now()),
                        review_mode: rm.parse().unwrap_or_default(),
                        tags:        vec![],
                    })
                },
            )
            .context("load card")
    }

    // ~~ Review recording ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// record a review result and advance the item's FSRS schedule
    /// chain_reexposure == true means this item is a preceding step that was
    /// already fully scheduled earlier in the same session (so the user is
    /// replaying the chain to unlock a step they failed). In that case the FSRS
    /// state (stability, difficulty, interval_days, due_at, lapses) is left
    /// completely untouched, and only the analytics counters are updated. Which
    /// prevents the chain-replay reviews from *inflating* the item's stability
    pub fn record_review(
        &self,
        item_id: &str,
        confidence: u8,
        duration: Duration,
        chain_reexposure: bool,
    ) -> Result<LeechStatus> {
        // load current item state
        let item = self
            .conn
            .query_row(
                "SELECT id, card_id, position, kind, prompt, answer, due_at,
                        interval_days, ease, last_reviewed_at, lapses, review_count,
                        confidence_avg, image_path, difficulty,
                        consecutive_fails, consecutive_hards,
                        scaffold_state, scaffold_passes, weak_spans, scaffold_pass_date
                 FROM items WHERE id=?1",
                params![item_id],
                |row| {
                    let kind: String             = row.get(3)?;
                    let due: String              = row.get(6)?;
                    let last: Option<String>     = row.get(9)?;
                    let image_path: Option<String> = row.get(13)?;
                    let scaffold_state: String   = row.get(17)?;
                    let weak_spans: String       = row.get(19)?;
                    let scaffold_pass_date: Option<String> = row.get(20)?;
                    Ok(Item {
                        id:               row.get(0)?,
                        card_id:          row.get(1)?,
                        position:         row.get(2)?,
                        kind:             kind.parse().unwrap_or(ItemKind::Forward),
                        prompt:           row.get(4)?,
                        answer:           row.get(5)?,
                        due_at:           due.parse().unwrap_or_else(|_| Utc::now()),
                        interval_days:    row.get(7)?,
                        stability:        row.get(8)?,
                        last_reviewed_at: last.and_then(|s| s.parse().ok()),
                        lapses:           row.get(10)?,
                        review_count:     row.get(11)?,
                        confidence_avg:   row.get(12)?,
                        image_path,
                        difficulty:       row.get(14)?,
                        consecutive_fails: row.get(15)?,
                        consecutive_hards: row.get(16)?,
                        scaffold_state:   scaffold_state.parse().unwrap_or_default(),
                        scaffold_passes:  row.get(18)?,
                        weak_spans:       parse_weak_spans(&weak_spans),
                        scaffold_pass_date: scaffold_pass_date
                            .and_then(|s| chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
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
        // snapshot before `item` is moved into the branches below (they
        // each consume it by value, either via `..item` or apply_confidence)
        let prev_fails              = item.consecutive_fails;
        let prev_hards              = item.consecutive_hards;
        let is_scaffolded           = item.scaffold_state.is_scaffolded();
        let prev_scaffold_state     = item.scaffold_state;
        let prev_scaffold_passes    = item.scaffold_passes;
        let prev_scaffold_pass_date = item.scaffold_pass_date;
        let now = Utc::now();

        let updated = if is_daily {
            // daily mode, FSRS state is frozen, so only update analytics so that
            // last_reviewed_at reflects today (used for the "reviewed today" gate)
            let n       = item.review_count + 1;
            let new_avg = rolling_avg(item.confidence_avg, item.review_count, confidence);
            Item { review_count: n, confidence_avg: new_avg, last_reviewed_at: Some(now), ..item }
        } else if chain_reexposure || is_scaffolded {
            // chain re-exposure OR a scaffolded rating, both are weaker
            // FSRS state (stability, difficulty, interval_days, due_at,
            // lapses) left completely untouched only analytics update
            let n       = item.review_count + 1;
            let new_avg = rolling_avg(item.confidence_avg, item.review_count, confidence);
            Item { review_count: n, confidence_avg: new_avg, last_reviewed_at: Some(now), ..item }
        } else {
            // normal SR review: hand off to the FSRS scheduler
            scheduler::apply_confidence(item, confidence, now)
        };

        // weak-step handling rolling consecutive-fail/hard counters
        //   Again (1)   -> consecutive_fails += 1, consecutive_hards = 0
        //   Hard  (2)   -> consecutive_hards += 1, consecutive_fails untouched
        //   Good+ (>=3) -> both reset to 0
        // chain_reexposure and scaffolded reviews are both weaker evidence
        // already fully scheduled once this session, or hint-assisted
        // the leech prompt while the chosen intervention is still active
        let (new_fails, new_hards) = if chain_reexposure || is_scaffolded {
            (prev_fails, prev_hards)
        } else if confidence == 1 {
            (prev_fails + 1, 0)
        } else if confidence == 2 {
            (prev_fails, prev_hards + 1)
        } else {
            (0, 0)
        };

        // scaffold graduation bookkeeping
        let (new_scaffold_state, new_scaffold_passes, new_scaffold_pass_date) = if is_scaffolded {
            if confidence >= 3 {
                // scaffolded pass, Daily-capped: only the first pass
                let today = now.date_naive();
                let already_today = prev_scaffold_pass_date == Some(today);
                let passes = if already_today { prev_scaffold_passes } else { prev_scaffold_passes + 1 };

                if passes as u32 >= scheduler::LEECH_STREAK_THRESHOLD {
                    // Graduate immediately
                    (ScaffoldState::Normal, passes, Some(today))
                } else {
                    (ScaffoldState::Scaffolded, passes, Some(today))
                }
            } else {
                // Scaffolded fail: reset progress toward graduation
                (ScaffoldState::Scaffolded, 0, None)
            }
        } else {
            (prev_scaffold_state, prev_scaffold_passes, prev_scaffold_pass_date)
        };

        self.conn
            .execute(
                "UPDATE items
                 SET due_at=?1, interval_days=?2, ease=?3, difficulty=?4,
                     last_reviewed_at=?5, lapses=?6, review_count=?7, confidence_avg=?8,
                     consecutive_fails=?9, consecutive_hards=?10,
                     scaffold_state=?11, scaffold_passes=?12, scaffold_pass_date=?13
                 WHERE id=?14",
                params![
                    updated.due_at.to_rfc3339(),
                    updated.interval_days,
                    updated.stability,   // stored in 'ease' column
                    updated.difficulty,
                    updated.last_reviewed_at.map(|t| t.to_rfc3339()),
                    updated.lapses,
                    updated.review_count,
                    updated.confidence_avg,
                    new_fails,
                    new_hards,
                    new_scaffold_state.as_str(),
                    new_scaffold_passes,
                    new_scaffold_pass_date.map(|d| d.format("%Y-%m-%d").to_string()),
                    updated.id,
                ],
            )
            .context("update item")?;

        // step-chain de-unlock
        // when a step is rated Again(1) or Hard(2) it was not recalled well
        // enough. So all subsequent steps from it are immediately due so the user
        // must rebuild the full chain from this point on their next session
        // This only applies to SR cards (never daily mode) (obviously)
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
                                        previous_interval_days,new_interval_days,chain_reexposure)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    new_id(),
                    &updated.id,
                    &updated.card_id,
                    now.to_rfc3339(),
                    confidence as i32,
                    duration.as_millis() as i64,
                    prev_interval,
                    updated.interval_days,
                    chain_reexposure as i32,
                ],
            )
            .context("insert review_log")?;

        // weak-step handling: while actively scaffolded, suppress the leech
        if is_scaffolded {
            Ok(LeechStatus::None)
        } else {
            self.leech_status_for(item_id, new_fails)
        }
    }

    /// primary trigger: `fails >= LEECH_STREAK_THRESHOLD` -> `Leech`
    /// secondary trigger: 4+ fails-or-hards within the last
    /// `LEECH_LOOKBACK_EVENTS` rating events (from `review_log`), to catch
    /// an oscillating Again/Hard pattern that never produces a clean
    /// consecutive streak of either kind alone -> `Leech`
    fn leech_status_for(&self, item_id: &str, fails: i32) -> Result<LeechStatus> {
        if fails as u32 >= scheduler::LEECH_STREAK_THRESHOLD {
            return Ok(LeechStatus::Leech);
        }

        let (recent_weak, recent_fails): (i64, i64) = self
            .conn
            .query_row(
                "SELECT
                    SUM(CASE WHEN confidence <= 2 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN confidence = 1  THEN 1 ELSE 0 END)
                 FROM (
                    SELECT confidence FROM review_log
                    WHERE item_id = ?1
                    ORDER BY reviewed_at DESC
                    LIMIT ?2
                 )",
                params![item_id, scheduler::LEECH_LOOKBACK_EVENTS as i64],
                |r| Ok((r.get::<_, Option<i64>>(0)?.unwrap_or(0), r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
            .context("query recent leech pattern")?;

        if recent_weak as u32 >= scheduler::LEECH_STREAK_THRESHOLD && recent_fails >= 2 {
            return Ok(LeechStatus::Leech);
        }

        Ok(LeechStatus::None)
    }

    /// from the leech-intervention prompt's "Enable scaffolding" option
    /// resets the Phase 1 counters to 0 alongside the scaffold fields so
    /// that a future graduation-then-relapse cycle starts clean rather than
    /// immediately re-triggering the leech prompt on stale counter values.
    fn enter_scaffold_mode(&self, item_id: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE items
                 SET scaffold_state='scaffolded', scaffold_passes=0, scaffold_pass_date=NULL,
                     consecutive_fails=0, consecutive_hards=0
                 WHERE id=?1",
                params![item_id],
            )
            .context("enter scaffold mode")?;
        Ok(())
    }

    /// increments the existing entrys weight if phrase is already in pool
    /// otherwise inserts it with weight 1. No-ops on an empty phrase
    pub fn add_weak_span(&self, item_id: &str, phrase: &str) -> Result<()> {
        let phrase = phrase.trim();
        if phrase.is_empty() {
            return Ok(());
        }

        let (raw, scaffold_state): (String, String) = self
            .conn
            .query_row(
                "SELECT weak_spans, scaffold_state FROM items WHERE id=?1",
                params![item_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .context("load weak_spans")?;

        let mut spans = parse_weak_spans(&raw);
        if let Some(existing) = spans.iter_mut().find(|s| s.phrase == phrase) {
            existing.weight += 1;
        } else {
            spans.push(WeakSpan { phrase: phrase.to_string(), weight: 1 });
        }

        let serialized = serde_json::to_string(&spans).context("serialize weak_spans")?;
        self.conn
            .execute(
                "UPDATE items SET weak_spans=?1 WHERE id=?2",
                params![serialized, item_id],
            )
            .context("save weak_spans")?;

        if scaffold_state != "scaffolded" {
            self.enter_scaffold_mode(item_id)?;
        }

        Ok(())
    }

    // ~~ Deck listing ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// Return every distinct deck name, sorted alphabetically.
    pub fn list_decks(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn
            .prepare("SELECT DISTINCT deck FROM cards ORDER BY deck ASC")
            .context("prepare list_decks")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .context("query list_decks")?;
        // keep everything sorted and deduped automatically with BTreeSet
        let mut decks: std::collections::BTreeSet<String> = rows
            .collect::<rusqlite::Result<_>>()
            .context("collect list_decks")?;
        // The sorted set is then used to take a snapshot of existing decks and
        // insert any missing ancestor paths (if "A::B::C" exists, it inserts "A" and "A::B")
        // then decks.into_iter().collect() converts the BTreeSet back into a Vec in sorted order
        let existing: Vec<String> = decks.iter().cloned().collect();
        for deck in &existing {
            let parts: Vec<&str> = deck.split("::").collect();
            for i in 1..parts.len() {
                decks.insert(parts[..i].join("::"));
            }
        }

        Ok(decks.into_iter().collect())
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
        let now   = Utc::now().to_rfc3339();
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
            let mut card = self.load_card(&s.card_id)?;
            card.tags = self.get_card_tags(&s.card_id)?;
            let mut items = self.load_items(&s.card_id)?;
            if reset_metadata {
                for item in &mut items {
                    item.due_at           = now - chrono::Duration::seconds(1);
                    item.interval_days    = 0.0;
                    item.stability        = 0.0;
                    item.difficulty       = 0.0;
                    item.last_reviewed_at = None;
                    item.lapses           = 0;
                    item.review_count     = 0;
                    item.confidence_avg   = 0.0;
                    item.consecutive_fails = 0;
                    item.consecutive_hards = 0;
                    item.scaffold_state    = ScaffoldState::Normal;
                    item.scaffold_passes   = 0;
                    item.scaffold_pass_date = None;
                    item.weak_spans        = Vec::new();
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


    /// both full exports and single-deck exports
    /// Cards/items whose ID already exists in the DB are replaced
    /// Older exports that predate FSRS carry `ease` (SM-2 factor) and no
    /// `difficulty` fieldm the `#[serde(default)]` on `Item::difficulty`
    /// ensures those deserialise to 0.0, which the scheduler treats as
    /// "never reviewed by FSRS" and bootstraps fresh on next rating
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

            // restore tags. ic.card.tags is an empty vec if the JSON predates tags support
            self.set_card_tags(&ic.card.id, &ic.card.tags)?;

            for item in ic.items {
                let weak_spans_json = serde_json::to_string(&item.weak_spans)
                    .unwrap_or_else(|_| "[]".to_string());
                self.conn
                    .execute(
                        "INSERT OR REPLACE INTO items
                            (id, card_id, position, kind, prompt, answer, due_at,
                            interval_days, ease, last_reviewed_at,
                            lapses, review_count, confidence_avg, image_path, difficulty,
                            consecutive_fails, consecutive_hards,
                            scaffold_state, scaffold_passes, weak_spans, scaffold_pass_date)
                        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
                        params![
                            item.id,
                            item.card_id,
                            item.position,
                            item.kind.as_str(),
                            item.prompt,
                            item.answer,
                            item.due_at.to_rfc3339(),
                            item.interval_days,
                            item.stability,      // stored in 'ease' column
                            item.last_reviewed_at.map(|t| t.to_rfc3339()),
                            item.lapses,
                            item.review_count,
                            item.confidence_avg,
                            item.image_path,
                            item.difficulty,
                            item.consecutive_fails,
                            item.consecutive_hards,
                            item.scaffold_state.as_str(),
                            item.scaffold_passes,
                            weak_spans_json,
                            item.scaffold_pass_date.map(|d| d.format("%Y-%m-%d").to_string()),
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
                self.insert_item(card_id, pos, "step", label.as_str(), answer, now, img.as_deref())?;
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
    
    // ~~ Tag helpers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    /// get the id of an existing tag by name or create it and return the new id
    fn get_or_create_tag(&self, raw_name: &str) -> Result<String> {
        let name = raw_name.trim().to_lowercase();
        anyhow::ensure!(!name.is_empty(), "tag name cannot be empty");

        match self.conn.query_row(
            "SELECT id FROM tags WHERE name=?1",
            params![name],
            |r| r.get::<_, String>(0),
        ) {
            Ok(id) => Ok(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let id = new_id();
                self.conn
                    .execute("INSERT INTO tags(id, name) VALUES(?1, ?2)", params![id, name])
                    .context("insert tag")?;
                Ok(id)
            }
            Err(e) => Err(e).context("look up tag"),
        }
    }

    /// replace the entire tag set for a card atomically
    /// and pass an empty slice to clear all tags
    pub fn set_card_tags(&self, card_id: &str, tags: &[String]) -> Result<()> {
        self.conn
            .execute("DELETE FROM card_tags WHERE card_id=?1", params![card_id])
            .context("clear card tags")?;

        for raw in tags {
            let name = raw.trim().to_lowercase();
            if name.is_empty() { continue; }
            let tag_id = self.get_or_create_tag(&name)?;
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO card_tags(card_id, tag_id) VALUES(?1, ?2)",
                    params![card_id, tag_id],
                )
                .context("link card tag")?;
        }
        Ok(())
    }

    /// return all tag names for a card, sorted alphabetically
    pub fn get_card_tags(&self, card_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT t.name FROM tags t
                JOIN card_tags ct ON ct.tag_id = t.id
                WHERE ct.card_id = ?1
                ORDER BY t.name ASC",
            )
            .context("prepare get_card_tags")?;

        let rows = stmt
            .query_map(params![card_id], |r| r.get(0))
            .context("query card tags")?;

        rows.collect::<rusqlite::Result<Vec<String>>>()
            .context("collect card tags")
    }

    /// return every tag name in the database sorted alphabetically
    pub fn list_all_tags(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM tags ORDER BY name ASC")
            .context("prepare list_all_tags")?;

        let rows = stmt
            .query_map([], |r| r.get(0))
            .context("query all tags")?;

        rows.collect::<rusqlite::Result<Vec<String>>>()
            .context("collect tags")
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

/// increment a running average without keeping a running sum
fn rolling_avg(prev_avg: f64, prev_count: i32, new_value: u8) -> f64 {
    let n = (prev_count + 1) as f64;
    if prev_count == 0 {
        new_value as f64
    } else {
        (prev_avg * prev_count as f64 + new_value as f64) / n
    }
}

/// parse the `weak_spans` TEXT column (JSON array of `WeakSpan`)
/// or empty content degrades to an empty pool rather than erroring, the
/// fallback-to-random-words render path already handles an empty pool
fn parse_weak_spans(raw: &str) -> Vec<WeakSpan> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}
