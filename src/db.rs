use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use rand::{rng, RngExt};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use uuid::Uuid;

use crate::models::*;
use crate::scheduler;
use crate::tree::{self, StepTree};

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
                -- 0 = legacy linear chain (parent = previous step by position)
                -- 1 = step tree (items.parent_id is authoritative), see tree.rs
                is_tree     INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS items (
                id               TEXT PRIMARY KEY,
                card_id          TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
                position         INTEGER NOT NULL,
                -- tree cards only, NULL = attached to the cards question
                -- deliberately NO `ON DELETE` action, deleting a step that still has
                -- children must fail loudly
                parent_id        TEXT REFERENCES items(id),
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
        // SQLite allows ADD COLUMN with REFERENCES as long as the default is NULL
        let _ = self.conn.execute(
            "ALTER TABLE cards ADD COLUMN is_tree INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE items ADD COLUMN parent_id TEXT REFERENCES items(id)",
            [],
        );
        // after the ALTERs, so old databases have the column when this runs
        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_items_parent ON items(parent_id)",
                [],
            )
            .context("create parent index")?;

        Ok(())
    }

    // ~~ Card creation

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

    /// the drafts may be a plain chain or a tree. Blank-answer steps are dropped
    /// (their children move up), and cards are always stored as tree cards
    /// (`is_tree = 1`) with `position` = DFS order, so a linear card is simply a
    /// tree in which every step has one child
    pub fn add_multi_card(
        &self,
        deck: &str,
        question: &str,
        steps: &[StepDraft],
        show_chain: bool,
        review_mode: &crate::models::ReviewMode,
    ) -> Result<String> {
        let drafts = tree::prune_blank(steps);
        if drafts.is_empty() {
            anyhow::bail!("multi-step cards need at least one step");
        }
        tree::check_branch_names(&drafts).map_err(|e| anyhow::anyhow!(e))?;

        let card_id = new_id();
        let now = Utc::now();
        let ts = now.to_rfc3339();

        let tx = self.conn.unchecked_transaction().context("begin add multi card")?;
        self.conn
            .execute_batch("PRAGMA defer_foreign_keys = ON;")
            .context("defer foreign keys")?;
        self.conn
            .execute(
                "INSERT INTO cards(id,deck,kind,question,reversible,show_chain,review_mode,is_tree,created_at,updated_at)
                VALUES(?1,?2,'multi',?3,0,?4,?5,1,?6,?7)",
                params![card_id, deck, question, show_chain as i32, review_mode.as_str(), ts, ts],
            )
            .context("insert multi card")?;
        self.write_step_drafts(&card_id, &drafts, &HashSet::new(), now)?;
        tx.commit().context("commit add multi card")?;
        Ok(card_id)
    }

    /// insert / update the step rows of one card from prepared drafts
    /// returns the ids of every step row that now belongs to the card
    /// must run inside a transaction with `defer_foreign_keys = ON`
    fn write_step_drafts(
        &self,
        card_id: &str,
        drafts: &[StepDraft],
        existing: &HashSet<String>,
        now: DateTime<Utc>,
    ) -> Result<HashSet<String>> {
        let depths = tree::draft_depths(drafts);

        // resolve every draft to a row id up front so parent links can be
        // written regardless of order, two drafts cant claim the same row
        let mut used: HashSet<String> = HashSet::new();
        let mut id_of: HashMap<u32, String> = HashMap::new();
        for d in drafts {
            let id = match &d.db_id {
                Some(id) if existing.contains(id) && !used.contains(id) => id.clone(),
                _ => new_id(),
            };
            used.insert(id.clone());
            id_of.insert(d.key, id);
        }

        for (i, d) in drafts.iter().enumerate() {
            let id = &id_of[&d.key];
            let parent = d.parent.and_then(|p| id_of.get(&p)).map(String::as_str);
            let pos = i as i32 + 1;
            // an unnamed step is labelled by its place in its path, which is
            // what review compares against to hide the redundant label
            let label = if d.name.trim().is_empty() {
                format!("Step {}", depths[i] + 1)
            } else {
                d.name.trim().to_string()
            };
            if existing.contains(id) {
                self.conn
                    .execute(
                        "UPDATE items
                         SET prompt=?1, answer=?2, image_path=?3, position=?4, parent_id=?5
                         WHERE id=?6 AND card_id=?7",
                        params![label, d.answer, d.image, pos, parent, id, card_id],
                    )
                    .context("update step item")?;
            } else {
                self.insert_item_full(
                    id, card_id, pos, parent, "step", &label, &d.answer, now, d.image.as_deref(),
                )?;
            }
        }
        Ok(used)
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
        self.insert_item_full(&new_id(), card_id, pos, None, kind, prompt, answer, now, image_path)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_item_full(
        &self,
        id: &str,
        card_id: &str,
        pos: i32,
        parent_id: Option<&str>,
        kind: &str,
        prompt: &str,
        answer: &str,
        now: DateTime<Utc>,
        image_path: Option<&str>,
    ) -> Result<()> {
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
                                   scaffold_state,scaffold_passes,weak_spans,scaffold_pass_date,parent_id)
                 VALUES(?1,?2,?3,?4,?5,?6,?7, 0.0,0.0,0,0,0.0,?8,0.0, 0,0,
                        'normal',0,'[]',NULL,?9)",
                params![id, card_id, pos, kind, prompt, answer, due, image_path, parent_id],
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
            for path in daily_paths(&card, &items, &today) {
                session.push(ReviewCard { card: card.clone(), items: path });
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
        //
        // each bucket entry is the cards whole group of paths kept together
        let mut sr_reviews: Vec<Vec<ReviewCard>> = Vec::new();
        let mut sr_new:     Vec<Vec<ReviewCard>> = Vec::new();

        for card in sr_cards {
            let items = self.load_items(&card.id)?;
            let paths = due_paths_for_card(&card, &items, now);
            if paths.is_empty() { continue; }

            let group: Vec<ReviewCard> = paths
                .into_iter()
                .map(|path| ReviewCard { card: card.clone(), items: path })
                .collect();

            if items.iter().all(|it| it.review_count == 0) {
                sr_new.push(group);
            } else {
                sr_reviews.push(group);
            }
        }

        // order the review bucket by *ascending retrievability* or
        // most likely to have been forgotten goes first
        // small random jitter is added to each score before sorting so
        // cards with almost identical urgency don't always come together
        // a card's score is its most urgent path
        //
        // new cards have no review history yet, so retrievability doesn't
        // apply to them yet, they are shuffled instead, mirroring anki
        let mut rng = rng();

        let mut scored: Vec<(f64, Vec<ReviewCard>)> = sr_reviews
            .into_iter()
            .map(|group| {
                let jitter = rng.random_range(-0.03..0.03);
                let urgency = group
                    .iter()
                    .map(|rc| target_retrievability(rc, now))
                    .fold(f64::INFINITY, f64::min);
                (urgency + jitter, group)
            })
            .collect();
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut sr_reviews: Vec<Vec<ReviewCard>> = scored.into_iter().map(|(_, g)| g).collect();

        sr_new.shuffle(&mut rng);

        // Apply caps (per card, not per path) then merge: reviews before new cards
        sr_reviews.truncate(limits.max_reviews);
        sr_new.truncate(limits.new_cards);

        session.extend(sr_reviews.into_iter().flatten());
        session.extend(sr_new.into_iter().flatten());

        Ok(session)
    }

    /// Load all cards in `filter` scope, ordered by creation time.
    fn load_cards_for_session(&self, filter: &str) -> Result<Vec<Card>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id,kind,deck,question,reversible,show_chain,created_at,updated_at,review_mode,is_tree
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
                let tree: i32    = row.get(9)?;
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
                    is_tree:     tree != 0,
                })
            })
            .context("query cards for session")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect cards for session")
    }

    // re-check a single card after a rating, returns a ReviewCard for every path
    // that now needs review (empty if nothing is due). A linear card yields at
    // most one, a branching card yields one per due branch. UI then dynamically
    // extends the running session queue
    pub fn review_card_if_due(&self, card_id: &str) -> Result<Vec<ReviewCard>> {
        let now   = Utc::now();
        let card  = self.load_card(card_id)?;
        let items = self.load_items(card_id)?;

        let paths = if card.review_mode.is_daily() {
            let today = now.format("%Y-%m-%d").to_string();
            daily_paths(&card, &items, &today)
        } else {
            due_paths_for_card(&card, &items, now)
        };

        Ok(paths
            .into_iter()
            .map(|path| ReviewCard { card: card.clone(), items: path })
            .collect())
    }

    pub fn load_items(&self, card_id: &str) -> Result<Vec<Item>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {ITEM_COLS} FROM items WHERE card_id=?1 ORDER BY position ASC"
            ))
            .context("prepare load_items")?;

        let rows = stmt
            .query_map(params![card_id], item_from_row)
            .context("query items")?;

        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("collect items")
    }

    pub fn load_card(&self, card_id: &str) -> Result<Card> {
        self.conn
            .query_row(
                "SELECT id,deck,kind,question,reversible,show_chain,created_at,updated_at,review_mode,is_tree
                FROM cards WHERE id=?1",
                params![card_id],
                |row| {
                    let kind: String = row.get(2)?;
                    let rev: i32     = row.get(4)?;
                    let sc: i32      = row.get(5)?;
                    let ca: String   = row.get(6)?;
                    let ua: String   = row.get(7)?;
                    let rm: String   = row.get(8)?;
                    let tree: i32    = row.get(9)?;
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
                        is_tree:     tree != 0,
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
                &format!("SELECT {ITEM_COLS} FROM items WHERE id=?1"),
                params![item_id],
                item_from_row,
            )
            .context("load item for review")?;

        // check whether this card is in daily mode / uses a step tree
        let (mode_str, tree_flag): (String, i32) = self
            .conn
            .query_row(
                "SELECT review_mode, is_tree FROM cards WHERE id=?1",
                params![&item.card_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or_else(|_| ("spaced_repetition".to_string(), 0));
        let is_daily = mode_str == "daily";
        let is_tree  = tree_flag != 0;

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

        // A re-exposure is a step being re-shown as *context* that is already
        // scheduled ahead. A step that is due right now must get a real rating
        // even if it was already rated earlier this session. That happens when a
        // later Again/Hard on an ancestor re-locked it (see the de-unlock below)
        // Treating that rating as a re-exposure would leave it due forever, and the
        // session would keep requeueing it, endlessly, even if every rating is a 3
        let chain_reexposure = chain_reexposure && item.due_at > now;

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

        // step de-unlock
        // when a step is rated Again(1) or Hard(2) it was not recalled well
        // enough. So every step BELOW it becomes immediately due and the user
        // must rebuild from this point on their next session
        // in a tree it is the steps descendants across all of its branches,
        // and not the siblings or ancestors (forgetting the SSD path doesn't reset HDD)
        // This only applies to SR cards (never daily mode) (obviously)
        if !is_daily && updated.kind == ItemKind::Step && confidence <= 2 {
            let items = self.load_items(&updated.card_id)?;
            let tree  = StepTree::new(is_tree, &items);
            if let Some(idx) = tree.index_of(&updated.id) {
                let below: Vec<&str> = tree
                    .descendants(idx)
                    .into_iter()
                    .map(|i| tree.item(i).id.as_str())
                    .collect();
                if !below.is_empty() {
                    let tx = self.conn.unchecked_transaction().context("begin de-unlock")?;
                    {
                        let mut upd = self
                            .conn
                            .prepare("UPDATE items SET due_at = ?1 WHERE id = ?2")
                            .context("prepare de-unlock")?;
                        for id in below {
                            upd.execute(params![now.to_rfc3339(), id])
                                .context("de-unlock subsequent steps")?;
                        }
                    }
                    tx.commit().context("commit de-unlock")?;
                }
            }
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

        // check every step tree up front so a bad file is rejected as a whole,
        // before anything is written
        for ic in &file.cards {
            if ic.card.kind == CardKind::Multi && ic.card.is_tree {
                tree::validate(&ic.items).with_context(|| {
                    format!("card {} has an invalid step tree", ic.card.id)
                })?;
            }
        }

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

            // one card = one transaction, with foreign keys checked at commit so
            // the order steps are written in cant trip the parent_id constraint
            let tx = self.conn.unchecked_transaction().context("begin import card")?;
            self.conn
                .execute_batch("PRAGMA defer_foreign_keys = ON;")
                .context("defer foreign keys")?;

            self.conn
                .execute(
                    "INSERT OR REPLACE INTO cards
                        (id, deck, kind, question, reversible, show_chain, review_mode, is_tree, created_at, updated_at)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![
                        ic.card.id,
                        ic.card.deck,
                        ic.card.kind.as_str(),
                        ic.card.question,
                        ic.card.reversible as i32,
                        ic.card.show_chain as i32,
                        ic.card.review_mode.as_str(),
                        ic.card.is_tree as i32,
                        ic.card.created_at.to_rfc3339(),
                        ic.card.updated_at.to_rfc3339(),
                    ],
                )
                .context("upsert card")?;

            if exists { summary.cards_replaced += 1; } else { summary.cards_imported += 1; }

            // restore tags. ic.card.tags is an empty vec if the JSON predates tags support
            self.set_card_tags(&ic.card.id, &ic.card.tags)?;

            // parents before children to keep organised
            let is_tree = ic.card.kind == CardKind::Multi && ic.card.is_tree;
            let order: Vec<usize> = if is_tree {
                StepTree::new(true, &ic.items).dfs_order()
            } else {
                (0..ic.items.len()).collect()
            };

            for idx in order {
                let item = &ic.items[idx];
                let weak_spans_json = serde_json::to_string(&item.weak_spans)
                    .unwrap_or_else(|_| "[]".to_string());
                // parent_id only means something on tree cards, a legacy card
                // carrying a stray one must not have it stored
                let parent_id = if is_tree { item.parent_id.as_deref() } else { None };
                self.conn
                    .execute(
                        "INSERT OR REPLACE INTO items
                            (id, card_id, position, kind, prompt, answer, due_at,
                            interval_days, ease, last_reviewed_at,
                            lapses, review_count, confidence_avg, image_path, difficulty,
                            consecutive_fails, consecutive_hards,
                            scaffold_state, scaffold_passes, weak_spans, scaffold_pass_date,
                            parent_id)
                        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
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
                            parent_id,
                        ],
                    )
                    .context("upsert item")?;
                summary.items_imported += 1;
            }

            tx.commit().context("commit import card")?;
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

    /// update an existing multi-step card's content in-place from editor drafts
    /// SRS state follows each step's **identity** (`StepDraft::db_id`), not its
    /// position, so steps can be inserted, reordered or re-parented without
    /// their schedules being attached to the wrong content
    pub fn update_multi_card(
        &self,
        card_id: &str,
        deck: &str,
        question: &str,
        steps: &[StepDraft],
        show_chain: bool,
        review_mode: &crate::models::ReviewMode,
    ) -> Result<()> {
        let drafts = tree::prune_blank(steps);
        if drafts.is_empty() {
            anyhow::bail!("multi-step cards need at least one step");
        }
        tree::check_branch_names(&drafts).map_err(|e| anyhow::anyhow!(e))?;

        let now = Utc::now();
        let ts  = now.to_rfc3339();

        let tx = self.conn.unchecked_transaction().context("begin update multi card")?;
        // parents and children are rewritten in one go, check the tree only at commit
        self.conn
            .execute_batch("PRAGMA defer_foreign_keys = ON;")
            .context("defer foreign keys")?;

        self.conn
            .execute(
                "UPDATE cards SET deck=?1, question=?2, show_chain=?3, review_mode=?4,
                                  is_tree=1, updated_at=?5 WHERE id=?6",
                params![deck, question, show_chain as i32, review_mode.as_str(), ts, card_id],
            )
            .context("update multi card")?;

        let existing: HashSet<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM items WHERE card_id=?1 AND kind='step'")
                .context("prepare existing steps")?;
            let rows = stmt
                .query_map(params![card_id], |r| r.get::<_, String>(0))
                .context("query existing steps")?;
            rows.collect::<rusqlite::Result<_>>().context("collect existing steps")?
        };

        let kept = self.write_step_drafts(card_id, &drafts, &existing, now)?;

        // remove steps that were deleted in the editor
        for id in existing.difference(&kept) {
            self.conn
                .execute("DELETE FROM items WHERE id=?1", params![id])
                .context("delete removed step")?;
        }

        tx.commit().context("commit update multi card")?;
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

// determine which items from a card should appear in a review session, as a
// list of *paths*. Each path becomes one `ReviewCard`, walked front to back,
// and its LAST item is the one being tested
fn due_paths_for_card(card: &Card, items: &[Item], now: DateTime<Utc>) -> Vec<Vec<Item>> {
    match card.kind {
        CardKind::Multi => StepTree::new(card.is_tree, items).due_paths(now),
        CardKind::Simple => {
            let due: Vec<Item> = items.iter().filter(|it| it.due_at <= now).cloned().collect();
            if due.is_empty() { vec![] } else { vec![due] }
        }
    }
}

// daily-mode paths. A daily card ignores schedulingt
fn daily_paths(card: &Card, items: &[Item], today: &str) -> Vec<Vec<Item>> {
    let not_today = |it: &Item| {
        it.last_reviewed_at
            .map(|t| t.format("%Y-%m-%d").to_string() != today)
            .unwrap_or(true)
    };
    match card.kind {
        CardKind::Simple => {
            if items.iter().any(not_today) { vec![items.to_vec()] } else { vec![] }
        }
        CardKind::Multi => {
            let tree = StepTree::new(card.is_tree, items);
            tree.leaf_paths()
                .into_iter()
                .filter(|p| p.iter().any(|&i| not_today(tree.item(i))))
                .map(|p| p.into_iter().map(|i| tree.item(i).clone()).collect())
                .collect()
        }
    }
}

/// Items that have never been reviewed by FSRS carry the same
/// stability == 0.0 bootstrap `scheduler::apply_confidence`
/// checks. There's no retrievability signal yet for those dividing by a
/// zero stability is undefined, so they're treated as maximally urgent (R = 0.0)
fn target_retrievability(rc: &ReviewCard, now: DateTime<Utc>) -> f64 {
    let Some(target) = rc.items.last() else {
        return 0.0; // due_items_for_card never returns an empty selection here
    };
    if target.stability <= 0.0 {
        return 0.0;
    }
    let days_elapsed = target
        .last_reviewed_at
        .map(|t| (now - t).num_days().max(0) as f64)
        .unwrap_or(0.0);
    scheduler::retrievability(days_elapsed, target.stability)
}

///  0 id | 1 card_id | 2 position | 3 kind | 4 prompt | 5 answer |
///  6 due_at | 7 interval_days | 8 ease(stability) |
///  9 last_reviewed_at | 10 lapses | 11 review_count |
///  12 confidence_avg | 13 image_path | 14 difficulty |
///  15 consecutive_fails | 16 consecutive_hards |
///  17 scaffold_state | 18 scaffold_passes | 19 weak_spans |
///  20 scaffold_pass_date | 21 parent_id
const ITEM_COLS: &str = "id, card_id, position, kind, prompt, answer, due_at,
                        interval_days, ease, last_reviewed_at, lapses, review_count,
                        confidence_avg, image_path, difficulty,
                        consecutive_fails, consecutive_hards,
                        scaffold_state, scaffold_passes, weak_spans, scaffold_pass_date,
                        parent_id";

fn item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
    let kind: String                       = row.get(3)?;
    let due: String                        = row.get(6)?;
    let last: Option<String>               = row.get(9)?;
    let image_path: Option<String>         = row.get(13)?;
    let scaffold_state: String             = row.get(17)?;
    let weak_spans: String                 = row.get(19)?;
    let scaffold_pass_date: Option<String> = row.get(20)?;
    Ok(Item {
        id:               row.get(0)?,
        card_id:          row.get(1)?,
        position:         row.get(2)?,
        parent_id:        row.get(21)?,
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


// ~~~ Tests ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    fn mem() -> Store {
        Store::open(":memory:").unwrap()
    }

    fn draft(key: u32, parent: Option<u32>, name: &str, answer: &str) -> StepDraft {
        StepDraft { key, db_id: None, parent, name: name.into(), answer: answer.into(), image: None }
    }

    /// t1 - t2 -+- a1 - a2      (branch names A / B)
    ///          +- b1
    fn fork_drafts() -> Vec<StepDraft> {
        vec![
            draft(0, None,    "",  "t1"),
            draft(1, Some(0), "",  "t2"),
            draft(2, Some(1), "A", "a1"),
            draft(3, Some(2), "",  "a2"),
            draft(4, Some(1), "B", "b1"),
        ]
    }

    /// two root paths, as in "SSD or HDD?":   s1 - s2     h1 - h2
    fn root_fork_drafts() -> Vec<StepDraft> {
        vec![
            draft(0, None,    "SSD", "s1"),
            draft(1, Some(0), "",    "s2"),
            draft(2, None,    "HDD", "h1"),
            draft(3, Some(2), "",    "h2"),
        ]
    }

    fn chain_drafts(answers: &[&str]) -> Vec<StepDraft> {
        answers
            .iter()
            .enumerate()
            .map(|(i, a)| draft(i as u32, if i == 0 { None } else { Some(i as u32 - 1) }, "", a))
            .collect()
    }

    fn add(store: &Store, drafts: &[StepDraft]) -> String {
        store
            .add_multi_card("Deck", "Q?", drafts, true, &ReviewMode::SpacedRepetition)
            .unwrap()
    }

    fn by_answer(store: &Store, card: &str, answer: &str) -> Item {
        store
            .load_items(card)
            .unwrap()
            .into_iter()
            .find(|i| i.answer == answer)
            .unwrap_or_else(|| panic!("no item with answer {answer}"))
    }

    fn rate(store: &Store, card: &str, answer: &str, confidence: u8) {
        let id = by_answer(store, card, answer).id;
        store.record_review(&id, confidence, Duration::from_secs(1), false).unwrap();
    }

    fn is_due(store: &Store, card: &str, answer: &str) -> bool {
        by_answer(store, card, answer).due_at <= Utc::now()
    }

    fn answers(path: &[Item]) -> Vec<&str> {
        path.iter().map(|i| i.answer.as_str()).collect()
    }

    fn fk_violations(store: &Store) -> usize {
        let mut stmt = store.conn.prepare("PRAGMA foreign_key_check").unwrap();
        stmt.query_map([], |_| Ok(())).unwrap().count()
    }

    /// make a card look like it predates branching
    fn make_legacy(store: &Store, card: &str) {
        store.conn.execute("UPDATE cards SET is_tree=0 WHERE id=?1", params![card]).unwrap();
        store.conn.execute("UPDATE items SET parent_id=NULL WHERE card_id=?1", params![card]).unwrap();
    }

    // ~~ linear behaviour is unchanged ~~

    // the rule before this feature, verbatim
    fn old_due_items_for_multi(items: &[Item], now: DateTime<Utc>) -> Vec<Item> {
        match items.iter().position(|it| it.due_at <= now) {
            Some(idx) => items[..=idx].to_vec(),
            None => vec![],
        }
    }

    #[test]
    fn legacy_chain_selection_matches_the_old_rule_for_every_due_pattern() {
        let store = mem();
        let card_id = add(&store, &chain_drafts(&["s1", "s2", "s3", "s4", "s5"]));
        make_legacy(&store, &card_id);
        let card = store.load_card(&card_id).unwrap();
        assert!(!card.is_tree);

        let now = Utc::now();
        for mask in 0u32..32 {
            for (i, it) in store.load_items(&card_id).unwrap().iter().enumerate() {
                let due = if mask & (1 << i) != 0 { now - ChronoDuration::minutes(5) }
                          else { now + ChronoDuration::days(2) };
                store.conn
                    .execute("UPDATE items SET due_at=?1 WHERE id=?2", params![due.to_rfc3339(), it.id])
                    .unwrap();
            }
            let items = store.load_items(&card_id).unwrap();
            let want = old_due_items_for_multi(&items, now);
            let got  = due_paths_for_card(&card, &items, now);
            if want.is_empty() {
                assert!(got.is_empty(), "mask {mask:05b}");
            } else {
                assert_eq!(got.len(), 1, "mask {mask:05b}");
                assert_eq!(answers(&got[0]), answers(&want), "mask {mask:05b}");
            }
        }
    }

    #[test]
    fn legacy_chain_deunlock_hits_exactly_the_later_steps() {
        let store = mem();
        let id = add(&store, &chain_drafts(&["s1", "s2", "s3", "s4"]));
        make_legacy(&store, &id);
        for a in ["s1", "s2", "s3", "s4"] { rate(&store, &id, a, 3); }
        assert!(["s1", "s2", "s3", "s4"].iter().all(|a| !is_due(&store, &id, a)));

        rate(&store, &id, "s2", 1);
        assert!(!is_due(&store, &id, "s1"));
        assert!(!is_due(&store, &id, "s2"), "the failed step itself is rescheduled");
        assert!(is_due(&store, &id, "s3"));
        assert!(is_due(&store, &id, "s4"));
    }

    // ~~ new: root fork from root question

    #[test]
    fn root_fork_surfaces_both_paths_then_unlocks_each_independently() {
        let store = mem();
        let id = add(&store, &root_fork_drafts());
        assert!(store.load_card(&id).unwrap().is_tree);

        let session = store.due_session(None, &SessionLimits::default()).unwrap();
        assert_eq!(session.len(), 2, "one review path per root");
        assert!(session.iter().all(|rc| rc.card.id == id && rc.items.len() == 1));
        let mut targets: Vec<&str> = session.iter().map(|rc| rc.items[0].answer.as_str()).collect();
        targets.sort();
        assert_eq!(targets, ["h1", "s1"]);

        // passing SSD step 1 unlocks SSD step 2 (with step 1 as context) and leaves HDD as it was
        rate(&store, &id, "s1", 3);
        let next = store.review_card_if_due(&id).unwrap();
        let paths: Vec<Vec<&str>> = next.iter().map(|rc| answers(&rc.items)).collect();
        assert_eq!(paths, vec![vec!["s1", "s2"], vec!["h1"]]);
    }

    #[test]
    fn failing_one_root_path_does_not_disturb_the_other() {
        let store = mem();
        let id = add(&store, &root_fork_drafts());
        for a in ["s1", "s2", "h1", "h2"] { rate(&store, &id, a, 3); }

        rate(&store, &id, "s1", 1); // forget the first SSD step
        assert!(is_due(&store, &id, "s2"), "below the failure");
        assert!(!is_due(&store, &id, "h1") && !is_due(&store, &id, "h2"), "other path untouched");
    }

    // ~~ new: fork in the middle creating a new branch

    #[test]
    fn deunlock_is_scoped_to_descendants() {
        let store = mem();
        let id = add(&store, &fork_drafts());
        for a in ["t1", "t2", "a1", "a2", "b1"] { rate(&store, &id, a, 3); }
        assert!(["t1", "t2", "a1", "a2", "b1"].iter().all(|a| !is_due(&store, &id, a)));

        // forgetting a leaf-side step only re-locks what is below it
        rate(&store, &id, "a1", 1);
        assert!(is_due(&store, &id, "a2"));
        assert!(!is_due(&store, &id, "b1"), "sibling branch untouched");
        assert!(!is_due(&store, &id, "t2") && !is_due(&store, &id, "t1"), "ancestors untouched");

        // forgetting the trunk re-locks BOTH branches
        rate(&store, &id, "t2", 2);
        assert!(is_due(&store, &id, "a1") && is_due(&store, &id, "a2") && is_due(&store, &id, "b1"));
        assert!(!is_due(&store, &id, "t1"));

        // and the next session targets the two children of t2, each after its trunk
        let next = store.review_card_if_due(&id).unwrap();
        let paths: Vec<Vec<&str>> = next.iter().map(|rc| answers(&rc.items)).collect();
        assert_eq!(paths, vec![vec!["t1", "t2", "a1"], vec!["t1", "t2", "b1"]]);
    }

    #[test]
    fn a_due_item_below_a_not_due_trunk_is_reached_through_the_trunk() {
        let store = mem();
        let id = add(&store, &fork_drafts());
        for a in ["t1", "t2", "a1", "a2", "b1"] { rate(&store, &id, a, 3); }
        store.conn
            .execute("UPDATE items SET due_at=?1 WHERE answer='a2'",
                     params![(Utc::now() - ChronoDuration::minutes(1)).to_rfc3339()])
            .unwrap();
        let next = store.review_card_if_due(&id).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(answers(&next[0].items), ["t1", "t2", "a1", "a2"]);
    }

    #[test]
    fn card_caps_count_cards_not_paths() {
        let store = mem();
        for _ in 0..3 { add(&store, &root_fork_drafts()); }
        let limits = SessionLimits { max_reviews: 200, new_cards: 2 };
        let session = store.due_session(None, &limits).unwrap();
        assert_eq!(session.len(), 4, "2 cards x 2 paths");
    }

    #[test]
    fn daily_tree_offers_each_path_and_skips_the_ones_done_today() {
        let store = mem();
        let id = store
            .add_multi_card("Deck", "Q?", &fork_drafts(), true, &ReviewMode::Daily)
            .unwrap();
        let session = store.due_session(None, &SessionLimits::default()).unwrap();
        let paths: Vec<Vec<&str>> = session.iter().map(|rc| answers(&rc.items)).collect();
        assert_eq!(paths, vec![vec!["t1", "t2", "a1", "a2"], vec!["t1", "t2", "b1"]]);

        // review the first path today (daily mode only stamps last_reviewed_at)
        for a in ["t1", "t2", "a1", "a2"] { rate(&store, &id, a, 3); }
        let again = store.review_card_if_due(&id).unwrap();
        let paths: Vec<Vec<&str>> = again.iter().map(|rc| answers(&rc.items)).collect();
        assert_eq!(paths, vec![vec!["t1", "t2", "b1"]], "only the branch not yet reviewed today");

        rate(&store, &id, "b1", 3);
        assert!(store.review_card_if_due(&id).unwrap().is_empty());
    }

    // ~~ editing keeps SRS state with the step, not the position

    #[test]
    fn inserting_a_step_mid_chain_keeps_the_other_steps_schedules() {
        let store = mem();
        let id = add(&store, &chain_drafts(&["s1", "s2", "s3"]));
        make_legacy(&store, &id);
        for a in ["s1", "s2", "s3"] { rate(&store, &id, a, 3); }
        let (s2_before, s3_before) = (by_answer(&store, &id, "s2"), by_answer(&store, &id, "s3"));

        // what the editor does: derive drafts from the (legacy) card, splice a step in, save
        let card  = store.load_card(&id).unwrap();
        let items = store.load_items(&id).unwrap();
        let mut drafts = tree::drafts_from_items(card.is_tree, &items);
        tree::insert_after(&mut drafts, 0, draft(0, None, "", "X"));
        store.update_multi_card(&id, "Deck", "Q?", &drafts, true, &ReviewMode::SpacedRepetition).unwrap();

        let items = store.load_items(&id).unwrap();
        assert_eq!(answers(&items), ["s1", "X", "s2", "s3"], "position follows the chain");
        let s2 = by_answer(&store, &id, "s2");
        let s3 = by_answer(&store, &id, "s3");
        assert_eq!((s2.id.as_str(), s2.stability, s2.due_at), (s2_before.id.as_str(), s2_before.stability, s2_before.due_at));
        assert_eq!((s3.id.as_str(), s3.stability, s3.due_at), (s3_before.id.as_str(), s3_before.stability, s3_before.due_at));
        let x = by_answer(&store, &id, "X");
        assert_eq!(x.review_count, 0);
        assert!(x.due_at <= Utc::now(), "a new step is immediately due");

        let by = |a: &str| items.iter().find(|i| i.answer == a).unwrap();
        assert_eq!(by("X").parent_id.as_deref(), Some(by("s1").id.as_str()));
        assert_eq!(by("s2").parent_id.as_deref(), Some(by("X").id.as_str()));
        assert!(store.load_card(&id).unwrap().is_tree, "converted on save");
        // labels are by depth: s2 is now the third step of its path
        assert_eq!(by("s2").prompt, "Step 3");
        assert_eq!(fk_violations(&store), 0);
    }

    #[test]
    fn removing_a_branch_deletes_only_that_subtree() {
        let store = mem();
        let id = add(&store, &fork_drafts());
        let card  = store.load_card(&id).unwrap();
        let items = store.load_items(&id).unwrap();
        let mut drafts = tree::drafts_from_items(card.is_tree, &items);
        let a1 = drafts.iter().position(|d| d.answer == "a1").unwrap();
        tree::remove_subtree(&mut drafts, a1);
        store.update_multi_card(&id, "Deck", "Q?", &drafts, true, &ReviewMode::SpacedRepetition).unwrap();

        let left = store.load_items(&id).unwrap();
        assert_eq!(answers(&left), ["t1", "t2", "b1"]);
        assert_eq!(fk_violations(&store), 0);
    }

    #[test]
    fn splice_removing_a_step_keeps_its_children_and_their_history() {
        let store = mem();
        let id = add(&store, &chain_drafts(&["s1", "s2", "s3"]));
        for a in ["s1", "s2", "s3"] { rate(&store, &id, a, 3); }
        let s3_before = by_answer(&store, &id, "s3");

        let card  = store.load_card(&id).unwrap();
        let items = store.load_items(&id).unwrap();
        let mut drafts = tree::drafts_from_items(card.is_tree, &items);
        tree::remove_splice(&mut drafts, 1); // drop s2
        store.update_multi_card(&id, "Deck", "Q?", &drafts, true, &ReviewMode::SpacedRepetition).unwrap();

        let items = store.load_items(&id).unwrap();
        assert_eq!(answers(&items), ["s1", "s3"]);
        let s3 = by_answer(&store, &id, "s3");
        assert_eq!((s3.stability, s3.review_count), (s3_before.stability, s3_before.review_count));
        assert_eq!(s3.parent_id.as_deref(), Some(by_answer(&store, &id, "s1").id.as_str()));
        assert_eq!(fk_violations(&store), 0);
    }

    #[test]
    fn saving_a_fork_without_branch_names_is_refused() {
        let store = mem();
        let mut d = fork_drafts();
        d[2].name.clear(); // the two children of t2 need names
        assert!(store.add_multi_card("Deck", "Q?", &d, true, &ReviewMode::SpacedRepetition).is_err());
        let id = add(&store, &fork_drafts());
        assert!(store.update_multi_card(&id, "Deck", "Q?", &d, true, &ReviewMode::SpacedRepetition).is_err());
    }

    // ~~ referential integrity

    #[test]
    fn deleting_a_step_that_still_has_children_is_refused() {
        let store = mem();
        let id = add(&store, &fork_drafts());
        let t1 = by_answer(&store, &id, "t1").id;
        assert!(store.conn.execute("DELETE FROM items WHERE id=?1", params![t1]).is_err());
    }

    #[test]
    fn deleting_a_tree_card_removes_everything() {
        let store = mem();
        let id = add(&store, &fork_drafts());
        store.delete_card(&id).unwrap();
        let n: i64 = store.conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    // ~~ export / import

    fn structure(store: &Store, card: &str) -> Vec<(String, Option<String>)> {
        let items = store.load_items(card).unwrap();
        let id_to_answer: HashMap<String, String> =
            items.iter().map(|i| (i.id.clone(), i.answer.clone())).collect();
        items
            .iter()
            .map(|i| (i.answer.clone(), i.parent_id.as_ref().map(|p| id_to_answer[p].clone())))
            .collect()
    }

    #[test]
    fn export_import_roundtrip_preserves_the_tree_even_if_children_come_first() {
        let src = mem();
        let id = add(&src, &fork_drafts());
        let mut json: serde_json::Value =
            serde_json::from_slice(&src.export_json(None, false).unwrap()).unwrap();
        json["cards"][0]["items"].as_array_mut().unwrap().reverse(); // children before parents

        let dst = mem();
        let summary = dst.import_json(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!((summary.cards_imported, summary.items_imported), (1, 5));
        assert!(dst.load_card(&id).unwrap().is_tree);
        let mut want = structure(&src, &id);
        let mut got  = structure(&dst, &id);
        want.sort();
        got.sort();
        assert_eq!(got, want);
        assert_eq!(fk_violations(&dst), 0);

        // importing the same file again replaces in place without error
        let again = dst.import_json(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(again.cards_replaced, 1);
        assert_eq!(fk_violations(&dst), 0);
        let mut got2 = structure(&dst, &id);
        got2.sort();
        assert_eq!(got2, want);
    }

    #[test]
    fn import_rejects_cycles_and_foreign_parents_without_writing_anything() {
        let src = mem();
        let id = add(&src, &fork_drafts());
        let good: serde_json::Value =
            serde_json::from_slice(&src.export_json(None, false).unwrap()).unwrap();
        let items = good["cards"][0]["items"].as_array().unwrap();
        let idof = |a: &str| items.iter().find(|i| i["answer"] == a).unwrap()["id"].clone();

        // cycle: t1's parent becomes a2 (which descends from t1)
        let mut cyc = good.clone();
        for it in cyc["cards"][0]["items"].as_array_mut().unwrap() {
            if it["answer"] == "t1" { it["parent_id"] = idof("a2"); }
        }
        // parent that isn't in the card
        let mut foreign = good.clone();
        for it in foreign["cards"][0]["items"].as_array_mut().unwrap() {
            if it["answer"] == "t1" { it["parent_id"] = "not-in-this-card".into(); }
        }

        for bad in [cyc, foreign] {
            let dst = mem();
            let err = dst.import_json(&serde_json::to_vec(&bad).unwrap());
            assert!(err.is_err());
            let n: i64 = dst.conn.query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0)).unwrap();
            assert_eq!(n, 0, "nothing written");
        }
        let _ = id;
    }

    #[test]
    fn import_of_a_pre_branching_export_is_a_linear_card() {
        let src = mem();
        let id = add(&src, &chain_drafts(&["s1", "s2", "s3"]));
        let mut json: serde_json::Value =
            serde_json::from_slice(&src.export_json(None, false).unwrap()).unwrap();
        // an old export has neither field
        json["cards"][0]["card"].as_object_mut().unwrap().remove("is_tree");
        for it in json["cards"][0]["items"].as_array_mut().unwrap() {
            it.as_object_mut().unwrap().remove("parent_id");
        }

        let dst = mem();
        dst.import_json(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(!dst.load_card(&id).unwrap().is_tree);
        let session = dst.due_session(None, &SessionLimits::default()).unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(answers(&session[0].items), ["s1"]);
        rate(&dst, &id, "s1", 3);
        let next = dst.review_card_if_due(&id).unwrap();
        assert_eq!(answers(&next[0].items), ["s1", "s2"]);
    }

    // ~~ upgrading a database created before this feature

    #[test]
    fn opening_a_pre_branching_database_migrates_in_place() {
        let path = std::env::temp_dir().join(format!("myoso-migrate-{}.db", new_id()));
        let path_str = path.to_str().unwrap().to_string();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(OLD_SCHEMA).unwrap();
            let now = Utc::now();
            let due = (now - ChronoDuration::seconds(5)).to_rfc3339();
            conn.execute(
                "INSERT INTO cards(id,deck,kind,question,created_at,updated_at)
                 VALUES('c1','Old','multi','Old question',?1,?1)",
                params![now.to_rfc3339()],
            ).unwrap();
            for (i, a) in ["one", "two", "three"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO items(id,card_id,position,kind,prompt,answer,due_at)
                     VALUES(?1,'c1',?2,'step',?3,?4,?5)",
                    params![format!("i{}", i + 1), i as i32 + 1, format!("Step {}", i + 1), a, due],
                ).unwrap();
            }
        }

        let store = Store::open(&path_str).unwrap();
        let card = store.load_card("c1").unwrap();
        assert!(!card.is_tree, "existing cards stay linear");
        let items = store.load_items("c1").unwrap();
        assert!(items.iter().all(|i| i.parent_id.is_none()));

        // opens again without complaint (ALTERs are idempotent)
        drop(store);
        let store = Store::open(&path_str).unwrap();

        // behaves like the chain it always was
        let session = store.due_session(None, &SessionLimits::default()).unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(answers(&session[0].items), ["one"]);
        store.record_review("i1", 3, Duration::from_secs(1), false).unwrap();
        let next = store.review_card_if_due("c1").unwrap();
        assert_eq!(answers(&next[0].items), ["one", "two"]);

        // and can be branched by editing it
        let items = store.load_items("c1").unwrap();
        let mut drafts = tree::drafts_from_items(false, &items);
        let root_key = drafts[0].key;
        tree::add_child(&mut drafts, Some(root_key), draft(0, None, "Alt", "one-b"));
        drafts[1].name = "Main".into();
        store.update_multi_card("c1", "Old", "Old question", &drafts, true, &ReviewMode::SpacedRepetition).unwrap();
        assert!(store.load_card("c1").unwrap().is_tree);
        assert_eq!(fk_violations(&store), 0);
        let _ = std::fs::remove_file(&path);
    }

    const OLD_SCHEMA: &str = r#"
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

    "#;
}
