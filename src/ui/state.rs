// ~~~ ui/state.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// All state types:
//   Screen, ReviewState, AddCardState, ListCardsState, ListDecksState,
//   ExportState, ImportState, AppState

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use ratatui::{layout::Rect, widgets::ListState};

use crate::db::Store;
use crate::models::{
    Card, CardKind, CardSummary, Item, ItemKind, LeechStatus, ReviewCard, ReviewMode, Stats,
    SessionLimits, StepDraft,
};
use crate::tree;

// ~~~ Screens ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(PartialEq, Eq, Clone)]
pub enum Screen {
    MainMenu,
    Stats,
    Review,
    AddCard,
    ListDecks,
    ListCards,
    Export,
    Import,
}

// ~~~ Review state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewPhase {
    Thinking,
    Revealed,
}

// ~~~ leech intervention ~~~~~~~~~~~~~~~~~~~~~~~~~
//
// when a rating pushes a steps leech status to `Leech`, the review UI must
// block on a rename/split intervention prompt before the normal
// advance/chain-fail logic runs.
pub struct LeechPrompt {
    pub item_id:      String,
    pub card_id:      String,
    pub prompt_text:  String,
    pub answer_text:  String,
    pub is_step:      bool,
    pub is_last_item: bool,
    pub confidence:   u8,
}

// ~~~ weak-span marking ~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// fires after any confidence <= 2 rating (regardless of current scaffold
// state), offering a lightweight, entirely skippable way to tag the exact
// words that tripped the user up. Carries the same
// finish_rating() fields as LeechPrompt for the same reason, it can appear
// either directly from rate(), or chained after a leech prompt
pub struct WeakSpanPrompt {
    pub item_id:      String,
    pub card_id:      String,
    pub is_step:      bool,
    pub is_last_item: bool,
    pub confidence:   u8,
    // the answer, tokenized on whitespace in original order
    pub words:        Vec<String>,
    // indices into `words` currently toggled on, building up one phrase
    pub selected:      HashSet<usize>,
    // list-navigation cursor
    pub cursor:        usize,
    // true once at least one phrase has been committed this prompt, purely
    // for the "marked!" confirmation flash in the footer
    pub committed_any: bool,
}

impl WeakSpanPrompt {
    fn new(item_id: String, card_id: String, is_step: bool, is_last_item: bool, confidence: u8, answer: &str) -> Self {
        Self {
            item_id,
            card_id,
            is_step,
            is_last_item,
            confidence,
            words: answer.split_whitespace().map(str::to_string).collect(),
            selected: HashSet::new(),
            cursor: 0,
            committed_any: false,
        }
    }
}

// ~~~ Feynman scratchpad ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// never touches the DB or import/export
// persists while stepping through a multi-step chain, and is wiped on next card
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewFocus {
    Card,
    Scratchpad,
}

pub struct ReviewState {
    pub session:           Vec<ReviewCard>,
    pub card_idx:          usize,
    pub item_idx:          usize,
    pub phase:             ReviewPhase,
    pub started_at:        Instant,
    pub item_started_at:   Instant,
    pub total_items:       usize,
    pub done_items:        usize,
    pub finished_duration: Option<std::time::Duration>,
    pub answer_scroll:     u16,
    // contains Item IDs that have already received a full SRS for curr session
    // any later rating of the same item (aka chain re-exposure) will only
    // update analytics, leaving the schedule unchanged
    pub session_rated:     HashSet<String>,
    pub leech_prompt:      Option<LeechPrompt>,
    // either directly or chained after a leech prompt resolves
    pub weak_span_prompt:  Option<WeakSpanPrompt>,
    pub scratchpad:        String, // feynman technique
    pub scratchpad_open:   bool,
    pub focus:             ReviewFocus,
}

impl ReviewState {
    pub fn new(session: Vec<ReviewCard>) -> Self {
        let total = session.iter().map(|rc| rc.items.len()).sum();
        Self {
            session,
            card_idx: 0,
            item_idx: 0,
            phase: ReviewPhase::Thinking,
            started_at: Instant::now(),
            item_started_at: Instant::now(),
            total_items: total,
            done_items: 0,
            finished_duration: None,
            answer_scroll: 0,
            session_rated: HashSet::new(),
            leech_prompt: None,
            weak_span_prompt: None,
            scratchpad: String::new(),
            scratchpad_open: false,
            focus: ReviewFocus::Card,
        }
    }

    pub fn is_done(&self) -> bool {
        self.card_idx >= self.session.len()
    }

    pub fn reveal(&mut self) {
        self.phase = ReviewPhase::Revealed;
        self.answer_scroll = 0; // reset scroll on new reveal
    }

    // appends a review path to the end of the session for every branch of
    // `card_id` that now has something due. A linear card has at most one path
    // a branching card can have several (one per due branch)
    //
    // a path is identified by its target, the last item, so a card can have
    // several paths queued at once, while the same path is never queued twice
    // (only the not yet reviewed part of the session is checked, so a path
    // that was just finished can come back if it is due again)
    pub fn requeue_if_due(&mut self, store: &Store, card_id: &str) -> anyhow::Result<()> {
        for review_card in store.review_card_if_due(card_id)? {
            let target = review_card.items.last().map(|it| it.id.as_str());
            let already_queued = self
                .session
                .get(self.card_idx..)
                .unwrap_or(&[])
                .iter()
                .any(|rc| {
                    rc.card.id == card_id && rc.items.last().map(|it| it.id.as_str()) == target
                });
            if already_queued {
                continue;
            }
            self.total_items += review_card.items.len();
            self.session.push(review_card);
        }

        Ok(())
    }

    /// bring the running session back in line with a card that was just
    /// edited from inside the review (the [e] key). Steps may have been
    /// added, deleted or moved, and a queued path that still names a deleted
    /// step would fail the moment it is rated, so every not yet finished path
    /// of the card is rebuilt from the database
    ///  * steps that still exist get their fresh text/state, in the same order
    ///  * steps that no longer exist are dropped from the paths
    ///  * paths whose target step is gone (or that are left empty) are removed,
    ///     and the cursor stays on the step on
    ///     screen (or the next surviving one if that step was deleted)
    /// anything newly due (like a step that was just added) is queued afterwards
    pub fn refresh_card(&mut self, store: &Store, card_id: &str) -> anyhow::Result<()> {
        let card = store.load_card(card_id)?;
        let fresh: HashMap<String, Item> = store
            .load_items(card_id)?
            .into_iter()
            .map(|it| (it.id.clone(), it))
            .collect();

        let mut i = self.card_idx;
        while i < self.session.len() {
            if self.session[i].card.id != card_id {
                i += 1;
                continue;
            }

            let is_current = i == self.card_idx;
            let old_items = std::mem::take(&mut self.session[i].items);
            // survivors among the steps already passed in the on-screen path
            // decide where the cursor lands
            let mut survivors_before_cursor = 0usize;
            let mut kept: Vec<Item> = Vec::with_capacity(old_items.len());
            for (k, it) in old_items.iter().enumerate() {
                if let Some(f) = fresh.get(&it.id) {
                    if is_current && k < self.item_idx {
                        survivors_before_cursor += 1;
                    }
                    kept.push(f.clone());
                }
            }

            let cursor_item_survived = is_current
                && old_items
                    .get(self.item_idx)
                    .map_or(false, |it| fresh.contains_key(&it.id));

            // a path exists to test its LAST step, so if that step was deleted the
            // path goes, even if some of the steps leading to it survive
            let target_gone = old_items.last().map_or(true, |it| !fresh.contains_key(&it.id));

            if target_gone || kept.is_empty() || (is_current && survivors_before_cursor >= kept.len()) {
                // nothing left to review in this path
                self.session.remove(i);
                if is_current {
                    self.item_idx = 0;
                    self.phase = ReviewPhase::Thinking;
                    self.item_started_at = Instant::now();
                    self.answer_scroll = 0;
                    self.clear_scratchpad();
                }
                continue; // `i` now names the next entry
            }

            self.session[i].card = card.clone();
            self.session[i].items = kept;
            if is_current {
                self.item_idx = survivors_before_cursor;
                if !cursor_item_survived {
                    // the step on screen is gone, so show the next one fresh
                    self.phase = ReviewPhase::Thinking;
                    self.item_started_at = Instant::now();
                    self.answer_scroll = 0;
                }
            }
            i += 1;
        }

        // keep the progress bar consistent, done + everything still ahead
        let remaining: usize = self
            .session
            .get(self.card_idx..)
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .map(|(n, rc)| if n == 0 { rc.items.len().saturating_sub(self.item_idx) } else { rc.items.len() })
            .sum();
        self.total_items = self.done_items + remaining;

        if card.kind == CardKind::Multi {
            self.requeue_if_due(store, card_id)?;
        }
        if self.is_done() && self.finished_duration.is_none() {
            self.finished_duration = Some(self.started_at.elapsed());
        }
        Ok(())
    }

    pub fn rate(&mut self, store: &Store, confidence: u8) -> anyhow::Result<()> {
        // ignore rating input while a blocking leech intervention, or the
        if self.is_done() || self.leech_prompt.is_some() || self.weak_span_prompt.is_some() {
            return Ok(());
        }

        // snapshot of what we need before state mutation
        let item         = &self.session[self.card_idx].items[self.item_idx];
        let item_id      = item.id.clone();
        let prompt_text  = item.prompt.clone();
        let answer_text  = item.answer.clone();
        let is_step      = item.kind == ItemKind::Step;
        let is_last_item = self.item_idx == self.session[self.card_idx].items.len() - 1;
        let card_id      = self.session[self.card_idx].card.id.clone();

        // a step is a chain re-exposure when it has already been fully
        // scheduled once during this session. Only applies to steps, simple
        // forward/reverse items are never re-shown within a session
        let chain_reexposure = is_step && self.session_rated.contains(&item_id);

        let leech_status = store.record_review(
            &item_id, confidence, self.item_started_at.elapsed(), chain_reexposure,
        )?;

        // mark this item as having received its first full update this session
        // later appearances (chain re-exposures) will skip rescheduling
        self.session_rated.insert(item_id.clone());

        self.done_items += 1;

        match leech_status {
            LeechStatus::Leech => {
                // The step that triggered the session must always be a
                // full-weight test
                self.leech_prompt = Some(LeechPrompt {
                    item_id, card_id, prompt_text, answer_text, is_step, is_last_item, confidence,
                });
                return Ok(());
            }
            LeechStatus::None => {}
        }

        // Weak-span marking is only offered as a follow-on to an actual
        // leech trigger (see resolve_leech_prompt below), never on a plain
        // Again/Hard rating by itself — it's a "you've now got a confirmed
        // leech, want to pin down the exact words?" tool, not a prompt on
        // every low rating.
        self.finish_rating(store, &card_id, confidence, is_step, is_last_item)
    }

    /// Resolve a pending leech intervention.
    /// `mark_blind_spot == true`: chain into the weak-span prompt so the
    /// `mark_blind_spot == false`: "Ignore": just finish the rating as normal.
    pub fn resolve_leech_prompt(&mut self, store: &Store, mark_blind_spot: bool) -> anyhow::Result<()> {
        if let Some(p) = self.leech_prompt.take() {
            if mark_blind_spot {
                self.weak_span_prompt = Some(WeakSpanPrompt::new(
                    p.item_id, p.card_id, p.is_step, p.is_last_item, p.confidence, &p.answer_text,
                ));
                return Ok(());
            }
            self.finish_rating(store, &p.card_id, p.confidence, p.is_step, p.is_last_item)?;
        }
        Ok(())
    }

    fn finish_rating(
        &mut self,
        store: &Store,
        card_id: &str,
        confidence: u8,
        is_step: bool,
        is_last_item: bool,
    ) -> anyhow::Result<()> {
        if is_step && confidence == 1 {
            // failure: skip remaining steps, jump to next card
            let skipped_items = self.session[self.card_idx].items.len() - self.item_idx - 1;
            self.total_items -= skipped_items;
            self.card_idx += 1;
            self.item_idx = 0;
            self.phase = ReviewPhase::Thinking;
            self.item_started_at = Instant::now();
            self.answer_scroll = 0;
            self.clear_scratchpad();

            // db just set all subsequent steps' due_at = now, so re-add this
            // card at the tail of the session if it now has due items
            self.requeue_if_due(store, card_id)?;
        } else {
            self.advance();

            // after completing the last step of a multi card, the next step in
            // the chain may already be due (all brand-new steps start past-due)
            if is_last_item && is_step {
                self.requeue_if_due(store, card_id)?;
            }
        }

        // snapshot only after all re-queuing so we don't falsely end early
        if self.is_done() && self.finished_duration.is_none() {
            self.finished_duration = Some(self.started_at.elapsed());
        }

        Ok(())
    }

    // ~~ weak-span marking interaction (Phase 2) ~~~~~~~~~~~

    pub fn weak_span_move_cursor(&mut self, delta: i32) {
        if let Some(p) = self.weak_span_prompt.as_mut() {
            if p.words.is_empty() {
                return;
            }
            let len = p.words.len() as i32;
            let new = (p.cursor as i32 + delta).rem_euclid(len);
            p.cursor = new as usize;
        }
    }

    pub fn weak_span_toggle_cursor(&mut self) {
        if let Some(p) = self.weak_span_prompt.as_mut() {
            if p.words.is_empty() {
                return;
            }
            if !p.selected.remove(&p.cursor) {
                p.selected.insert(p.cursor);
            }
        }
    }

    /// Commit the currently-toggled words as one phrase
    pub fn weak_span_commit(&mut self, store: &Store) -> anyhow::Result<()> {
        if let Some(p) = self.weak_span_prompt.as_mut() {
            if p.selected.is_empty() {
                return Ok(());
            }
            let mut idxs: Vec<usize> = p.selected.iter().copied().collect();
            idxs.sort_unstable();
            let phrase = idxs.iter().map(|&i| p.words[i].as_str())
                .collect::<Vec<_>>().join(" ");

            store.add_weak_span(&p.item_id, &phrase)?;
            p.selected.clear();
            p.committed_any = true;
        }
        Ok(())
    }

    /// Finish the weak-span prompt
    pub fn resolve_weak_span_prompt(&mut self, store: &Store) -> anyhow::Result<()> {
        if let Some(p) = self.weak_span_prompt.take() {
            self.finish_rating(store, &p.card_id, p.confidence, p.is_step, p.is_last_item)?;
        }
        Ok(())
    }

    pub fn advance(&mut self) {
        let prev_card_idx = self.card_idx;
        self.item_idx += 1;
        self.phase = ReviewPhase::Thinking;
        self.item_started_at = Instant::now();
        if let Some(card) = self.session.get(self.card_idx) {
            if self.item_idx >= card.items.len() {
                self.card_idx += 1;
                self.item_idx = 0;
            }
        }
        if self.card_idx != prev_card_idx {
            self.answer_scroll = 0; // fresh card: don't carry over old scroll
            self.clear_scratchpad();
        }
        // snapshot the elapsed time the moment the last item is rated
        if self.is_done() && self.finished_duration.is_none() {
            self.finished_duration = Some(self.started_at.elapsed());
        }
    }

    pub fn progress(&self) -> f64 {
        if self.total_items == 0 {
            1.0
        } else {
            self.done_items as f64 / self.total_items as f64
        }
    }

    // ~~ Feynman scratchpad ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

    fn clear_scratchpad(&mut self) {
        self.scratchpad.clear();
        self.scratchpad_open = false;
        self.focus = ReviewFocus::Card;
    }

    /// `[f]` only fires while focus is on the card, but if focus is on the
    /// scratchpad itself, 'f' is just a character being typed into it
    pub fn toggle_scratchpad(&mut self) {
        if self.focus != ReviewFocus::Card {
            return;
        }
        if self.scratchpad_open {
            self.scratchpad_open = false; // hide but keep content
        } else {
            self.scratchpad_open = true;
            self.focus = ReviewFocus::Scratchpad; // jump straight into typing
        }
    }

    /// `[Tab]` while the pad is open, swap between typing in the pad and
    /// controlling the card (reveal/rate/scroll), without closing it
    pub fn cycle_review_focus(&mut self) {
        if !self.scratchpad_open {
            return;
        }
        self.focus = match self.focus {
            ReviewFocus::Card       => ReviewFocus::Scratchpad,
            ReviewFocus::Scratchpad => ReviewFocus::Card,
        };
    }

    pub fn scratchpad_push_char(&mut self, c: char) {
        if self.focus == ReviewFocus::Scratchpad {
            self.scratchpad.push(c);
        }
    }

    pub fn scratchpad_newline(&mut self) {
        if self.focus == ReviewFocus::Scratchpad {
            self.scratchpad.push('\n');
        }
    }

    pub fn scratchpad_backspace(&mut self) {
        if self.focus == ReviewFocus::Scratchpad {
            self.scratchpad.pop();
        }
    }
}

// ~~~ AddCard state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddKind {
    Simple,
    Multi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddPhase {
    PickType,
    FillForm,
}

pub struct AddCardState {
    pub phase:               AddPhase,
    pub kind:                AddKind,
    // When Some, we are editing an existing card rather than creating one.
    pub editing_card_id:     Option<String>,
    // shared
    pub deck:                String,
    // simple only
    pub simple_question:     String,
    pub multi_question:      String,
    pub answer:              String,
    pub reversible:          bool,
    pub show_chain:          bool, // multi only: show preceding step answers during review
    // multi-only, the steps form a tree (a plain chain is the common case) kept as
    // a flat list in DFS order, each with its parent's key, StepDraft / tree.rs
    // the list cursor (step_list_state) indexes into it one row per step
    pub answer_image_path:   Option<String>,
    pub step_image_path:     Option<String>,
    pub steps:               Vec<StepDraft>,
    // where the NEXT new step attaches, set by [b] / [r] on the steps list
    //   None              default: extend the selected step, or the end of the chain
    //   Some(Some(key)) [b]   a child of that step (a new branch if it has children)
    //   Some(None) [r]        a new top-level path, attached to the question itself
    pub pending_parent:      Option<Option<u32>>,
    pub step_name_buf:       String,
    pub step_buf:            String,
    pub step_list_state:     ListState,
    pub editing_step_idx:    Option<usize>,
    // navigation
    pub focused:             usize,
    pub error:               Option<String>,
    // deck suggestions
    pub decks:               Vec<String>,
    pub deck_list_idx:       Option<usize>,
    // review mode
    pub review_mode:         ReviewMode,
    // set when launched from inside a review session so save returns to Review
    pub editing_from_review: bool,
    pub tags_buf:            String,
}

impl AddCardState {
    pub fn new(decks: Vec<String>) -> Self {
        Self {
            phase:               AddPhase::PickType,
            kind:                AddKind::Simple,
            editing_card_id:     None,
            deck:                String::new(),
            simple_question:     String::new(),
            multi_question:      String::new(),
            answer:              String::new(),
            reversible:          false,
            show_chain:          true,
            answer_image_path:   None,
            step_image_path:     None,
            steps:               Vec::new(),
            pending_parent:      None,
            step_name_buf:       String::new(),
            step_buf:            String::new(),
            step_list_state:     ListState::default(),
            editing_step_idx:    None,
            focused:             0,
            error:               None,
            decks,
            deck_list_idx:       None,
            review_mode:         ReviewMode::SpacedRepetition,
            editing_from_review: false,
            tags_buf:            String::new(),
        }
    }

    /// Decks whose name contains the currently-typed text (case-insensitive)
    pub fn filtered_decks(&self) -> Vec<&str> {
        let q = self.deck.trim().to_lowercase();
        self.decks.iter()
            .map(|d| d.as_str())
            .filter(|d| q.is_empty() || d.to_lowercase().contains(&q))
            .collect()
    }

    /// return the currently highlighted suggestion as an owned String
    pub fn selected_suggestion(&self) -> Option<String> {
        self.deck_list_idx
            .and_then(|i| self.filtered_decks().get(i).map(|s| s.to_string()))
    }

    /// Parse tags_buf into a normalised Vec<String>
    pub fn parsed_tags(&self) -> Vec<String> {
        self.tags_buf
            .split(|c: char| c == ',' || c == ' ')
            .map(|t| t.trim_start_matches('#').trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// Field layout
    ///   Simple: deck(0)  question(1)  answer(2)  reversible(3)  daily(4)  tags(5)  [Save](6)
    ///   Multi:  deck(0)  question(1)  step_name(2)  step_answer(3)  [Add step](4)
    ///           steps_list(5)  show_chain(6)  daily(7)  tags(8)  [Save](9)
    pub fn field_count(&self) -> usize {
        match self.kind {
            AddKind::Simple => 7,
            AddKind::Multi  => 10,
        }
    }

    pub fn next_field(&mut self) {
        self.focused = (self.focused + 1) % self.field_count();
    }

    pub fn prev_field(&mut self) {
        let n = self.field_count();
        self.focused = if self.focused == 0 { n - 1 } else { self.focused - 1 };
    }

    pub fn is_save(&self)            -> bool { self.focused == self.field_count() - 1 }
    pub fn is_reversible(&self)      -> bool { self.kind == AddKind::Simple && self.focused == 3 }
    pub fn is_daily_toggle(&self)    -> bool {
        match self.kind {
            AddKind::Simple => self.focused == 4,
            AddKind::Multi  => self.focused == 7,
        }
    }
    pub fn is_step_name(&self)       -> bool { self.kind == AddKind::Multi && self.focused == 2 }
    pub fn is_add_step_button(&self) -> bool { self.kind == AddKind::Multi && self.focused == 4 }
    pub fn is_steps_list(&self)      -> bool { self.kind == AddKind::Multi && self.focused == 5 }
    pub fn is_show_chain(&self)      -> bool { self.kind == AddKind::Multi && self.focused == 6 }

    pub fn active_buf_mut(&mut self) -> Option<&mut String> {
        match self.kind {
            AddKind::Simple => match self.focused {
                0 => Some(&mut self.deck),
                1 => Some(&mut self.simple_question),
                2 => Some(&mut self.answer),
                5 => Some(&mut self.tags_buf),
                _ => None,
            },
            AddKind::Multi => match self.focused {
                0 => Some(&mut self.deck),
                1 => Some(&mut self.multi_question),
                2 => Some(&mut self.step_name_buf),
                3 => Some(&mut self.step_buf),
                8 => Some(&mut self.tags_buf),
                _ => None, // 4 is add button, 5 is steps list, 6 is save
            },
        }
    }

    pub fn push_char(&mut self, c: char) {
        if let Some(b) = self.active_buf_mut() { b.push(c); }
    }

    pub fn pop_char(&mut self) {
        if let Some(b) = self.active_buf_mut() { b.pop(); }
    }

    // ~~ step tree editing

    /// index of the highlighted step, if it is a valid row
    pub fn selected_step(&self) -> Option<usize> {
        self.step_list_state.selected().filter(|&i| i < self.steps.len())
    }

    /// where a new step goes when the user hasn't asked for a branch, after the
    /// selected step if it is a leaf (extending its chain), otherwise at the end
    /// of the outline. The last step in DFS order is always a leaf, so for a
    /// plain chain this is simply "append"
    fn default_parent(&self) -> Option<u32> {
        match self.selected_step() {
            Some(i) if tree::child_count(&self.steps, Some(self.steps[i].key)) == 0 => {
                Some(self.steps[i].key)
            }
            _ => self.steps.last().map(|d| d.key),
        }
    }

    /// [b] on the steps listm the next step becomes a child of the selected
    /// step. If that step already has children this starts a new branch
    pub fn begin_branch(&mut self) {
        if let Some(i) = self.selected_step() {
            self.pending_parent = Some(Some(self.steps[i].key));
            self.editing_step_idx = None;
            self.step_name_buf.clear();
            self.step_buf.clear();
            self.step_image_path = None;
            self.focused = 2; // Step Name
        }
    }

    /// [r] on the steps list, the next step starts a new top-level path,
    /// an alternative answer to the question itself
    pub fn begin_root_path(&mut self) {
        self.pending_parent = Some(None);
        self.editing_step_idx = None;
        self.step_name_buf.clear();
        self.step_buf.clear();
        self.step_image_path = None;
        self.focused = 2;
    }

    /// remove the selected step. `with_branch == false` removes just that step
    /// and its children move up (nothing else is lost), `true` removes the step
    /// and everything below it
    pub fn delete_selected_step(&mut self, with_branch: bool) {
        let Some(i) = self.selected_step() else { return };
        if with_branch {
            tree::remove_subtree(&mut self.steps, i);
        } else {
            tree::remove_splice(&mut self.steps, i);
        }
        let n = self.steps.len();
        self.step_list_state.select(if n == 0 { None } else { Some(i.min(n - 1)) });
        // indices moved, so a half-finished edit or pending target would point at the wrong step
        self.editing_step_idx = None;
        self.pending_parent = None;
        self.step_name_buf.clear();
        self.step_buf.clear();
    }

    /// short description of where the next new step will attach, for the
    /// step Name box. `None` when there is nothing to say (first step, or editing)
    pub fn attach_hint(&self) -> Option<String> {
        if self.editing_step_idx.is_some() || self.steps.is_empty() {
            return None;
        }
        let parent = match self.pending_parent {
            Some(p) => p,
            None => self.default_parent(),
        };
        let Some(key) = parent else {
            return Some("new top-level path".to_string());
        };
        let i = self.steps.iter().position(|d| d.key == key)?;
        let step_no = tree::outline(&self.steps)[i].step_no;
        let d = &self.steps[i];
        let label = if d.name.trim().is_empty() {
            d.answer.lines().next().unwrap_or("").trim().to_string()
        } else {
            d.name.trim().to_string()
        };
        let label: String = if label.chars().count() > 22 {
            label.chars().take(21).collect::<String>() + "…"
        } else {
            label
        };
        let has_kids = tree::child_count(&self.steps, Some(key)) > 0;
        Some(format!(
            "{} {}. {}",
            if has_kids { "new branch under" } else { "after" },
            step_no,
            label
        ))
    }

    pub fn commit_step(&mut self) {
        let answer = self.step_buf.trim().to_string();
        let name   = self.step_name_buf.trim().to_string();
        let img = self.step_image_path.take();
        if !answer.is_empty() {
            if let Some(idx) = self.editing_step_idx {
                if idx < self.steps.len() {
                    // edit in place, identity and position in the tree are kept
                    self.steps[idx].name   = name;
                    self.steps[idx].answer = answer;
                    self.steps[idx].image  = img;
                }
                self.editing_step_idx = None;
                self.focused = 5; // jump back to the list after editing
            } else {
                let parent = match self.pending_parent.take() {
                    Some(p) => p,
                    None    => self.default_parent(),
                };
                let at = tree::add_child(
                    &mut self.steps,
                    parent,
                    StepDraft { name, answer, image: img, ..StepDraft::default() },
                );
                self.step_list_state.select(Some(at));
                self.focused = 3; // keep the cursor in the step editor for the next entry
            }
            // resets after commit
            self.step_name_buf.clear();
            self.step_buf.clear();
        } else if let Some(idx) = self.editing_step_idx {
            // cancel edit if user submits blank answer
            if idx < self.steps.len() && self.steps[idx].answer.is_empty() {
                // a step that was spliced in and never filled, take it back out
                tree::remove_splice(&mut self.steps, idx);
                let n = self.steps.len();
                let sel = if n == 0 { None } else { Some(idx.min(n - 1)) };
                self.step_list_state.select(sel);
            }
            self.editing_step_idx = None;
            self.step_name_buf.clear();
            self.step_buf.clear();
            self.focused = 5;
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.deck.trim().is_empty() { return Err("Deck name cannot be empty."); }
        let q = match self.kind { // get question type for kind
            AddKind::Simple => &self.simple_question,
            AddKind::Multi  => &self.multi_question,
        };
        if q.trim().is_empty() { return Err("Question cannot be empty."); }
        match self.kind {
            AddKind::Simple if self.answer.trim().is_empty() =>
                Err("Answer cannot be empty."),
            AddKind::Multi if self.steps.is_empty() =>
                Err("Add at least one step (type in the step field, then press the Add step button)."),
            AddKind::Multi if self.steps.iter().all(|d| d.answer.trim().is_empty()) =>
                Err("Add at least one step with content."),
            // sibling branches must be nameable in review
            AddKind::Multi => tree::check_branch_names(&tree::prune_blank(&self.steps)),
            _ => Ok(()),
        }
    }

    /// Rebuild an AddCardState pre-populated from an existing card for editing.
    /// Jumps straight to FillForm and skips the PickType screen.
    pub fn for_edit(card: &Card, items: &[Item], decks: Vec<String>, tags: Vec<String>) -> Self {
        let mut s = Self::new(decks);
        s.editing_card_id = Some(card.id.clone());
        s.phase    = AddPhase::FillForm;
        s.deck     = card.deck.clone();
        s.reversible = card.reversible;
        s.review_mode = card.review_mode.clone();
        s.tags_buf = tags.join(" ");
        match card.kind {
            CardKind::Simple => { // handle question & answer for simple kind
                s.kind = AddKind::Simple;
                s.simple_question = card.question.clone();
                if let Some(item) = items.iter().find(|i| i.kind == ItemKind::Forward) {
                    s.answer = item.answer.clone();
                    s.answer_image_path = item.image_path.clone();
                }
            }
            CardKind::Multi => { // handle question & answer for multi kind
                s.kind  = AddKind::Multi;
                s.multi_question = card.question.clone();
                s.show_chain = card.show_chain;
                let step_items: Vec<Item> = items
                    .iter()
                    .filter(|i| i.kind == ItemKind::Step)
                    .cloned()
                    .collect();
                // the same call handles a legacy chain (parents derived from
                // position) and a tree card, auto-generated "Step N" labels become
                // blank names so the name field starts empty
                s.steps = tree::drafts_from_items(card.is_tree, &step_items);
                if !s.steps.is_empty() {
                    s.step_list_state.select(Some(0));
                }
            }
        }
        s
    }

    /// insert a blank step immediately after the currently selected step, it
    /// takes that step's place in the chain and adopts its children, or at the
    /// end if nothing is selected, then jump to the step-input field so the user
    /// can type the new step content straight away
    pub fn insert_step_after_selected(&mut self) {
        let insert_at = match self.selected_step() {
            Some(i) => tree::insert_after(&mut self.steps, i, StepDraft::default()),
            None => {
                let parent = self.default_parent();
                tree::add_child(&mut self.steps, parent, StepDraft::default())
            }
        };
        self.pending_parent = None;
        self.editing_step_idx = Some(insert_at);
        self.step_list_state.select(Some(insert_at));
        self.step_name_buf.clear();
        self.step_buf.clear();
        self.focused = 2; // jump to Step Name field first
    }

    pub fn is_multiline_field(&self) -> bool {
        match self.kind {
            AddKind::Simple => matches!(self.focused, 1 | 2), // both question or answer
            AddKind::Multi => matches!(self.focused, 1 | 3),  // question or step answer (multi)
        }
    }
}

// ~~~ SearchScope ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// switch between different search scopes: question, answer and both

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    #[default]
    Questions,  // question + deck (default behaviour)
    Answers,    // item answers / steps only
    Both,       // question + deck + answers
}

impl SearchScope {
    pub fn label(self) -> &'static str {
        match self {
            SearchScope::Questions => "Q",
            SearchScope::Answers   => "A",
            SearchScope::Both      => "Q+A",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SearchScope::Questions => SearchScope::Answers,
            SearchScope::Answers   => SearchScope::Both,
            SearchScope::Both      => SearchScope::Questions,
        }
    }
}

// ~~~ ListCards state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub struct ListCardsState {
    pub cards:             Vec<CardSummary>,
    pub list_state:        ListState,
    pub deck:              Option<String>,
    pub confirm_delete:    bool,
    // search
    pub search_query:      String,
    pub search_active:     bool,
    pub search_scope:      SearchScope,
    // tag filter
    pub tag_filter:        Vec<String>,
    pub tag_picker_active: bool,
    pub tag_picker_tags:   Vec<String>,
    pub tag_picker_state:  ListState,
}

impl ListCardsState {
    pub fn new(cards: Vec<CardSummary>, deck: Option<String>) -> Self {
        let mut ls = ListState::default();
        if !cards.is_empty() { ls.select(Some(0)); }
        Self {
            cards,
            list_state:        ls,
            deck,
            confirm_delete:    false,
            search_query:      String::new(),
            search_active:     false,
            search_scope:      SearchScope::default(),
            tag_filter:        Vec::new(),
            tag_picker_active: false,
            tag_picker_tags:   Vec::new(),
            tag_picker_state:  ListState::default(),
        }
    }

    pub fn filtered_cards(&self) -> Vec<&CardSummary> {
        let q = self.search_query.to_lowercase();
        self.cards.iter().filter(|c| {
            let text_ok = q.is_empty() || match self.search_scope {
                SearchScope::Questions => {
                    c.question.to_lowercase().contains(&q)
                        || c.deck.to_lowercase().contains(&q)
                }
                SearchScope::Answers => {
                    c.answers_text.to_lowercase().contains(&q)
                }
                SearchScope::Both => {
                    c.question.to_lowercase().contains(&q)
                        || c.deck.to_lowercase().contains(&q)
                        || c.answers_text.to_lowercase().contains(&q)
                }
            };
            let tag_ok = self.tag_filter.is_empty()
                || self.tag_filter.iter().all(|ft| c.tags.iter().any(|ct| ct == ft));
            text_ok && tag_ok
        }).collect()
    }

    pub fn next(&mut self) {
        let n = self.filtered_cards().len();
        if n == 0 { return; }
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % n);
        self.list_state.select(Some(i));
    }

    pub fn prev(&mut self) {
        let n = self.filtered_cards().len();
        if n == 0 { return; }
        let i = self.list_state.selected()
            .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
        self.list_state.select(Some(i));
    }
}

// ~~~ ListDecks state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub struct ListDecksState {
    pub decks:          Vec<String>,
    pub list_state:     ListState,
    pub confirm_delete: bool,
    // search
    pub search_query:   String,
    pub search_active:  bool,
}

impl ListDecksState {
    pub fn new(decks: Vec<String>) -> Self {
        let mut ls = ListState::default();
        if !decks.is_empty() { ls.select(Some(0)); }
        Self {
            decks,
            list_state:     ls,
            confirm_delete: false,
            search_query:   String::new(),
            search_active:  false,
        }
    }

    pub fn filtered_decks(&self) -> Vec<&str> {
        let q = self.search_query.to_lowercase();
        self.decks.iter()
            .map(String::as_str)
            .filter(|d| q.is_empty() || d.to_lowercase().contains(&q))
            .collect()
    }

    pub fn next(&mut self) {
        let n = self.filtered_decks().len();
        if n == 0 { return; }
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % n);
        self.list_state.select(Some(i));
    }

    pub fn prev(&mut self) {
        let n = self.filtered_decks().len();
        if n == 0 { return; }
        let i = self.list_state.selected()
            .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
        self.list_state.select(Some(i));
    }

    pub fn selected_deck(&self) -> Option<&str> {
        let idx = self.list_state.selected()?;
        let q   = self.search_query.to_lowercase();
        self.decks.iter()
            .filter(|d| q.is_empty() || d.to_lowercase().contains(&q))
            .nth(idx)
            .map(String::as_str)
    }
}

// ~~~ Export state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFocus {
    DeckList,
    ResetToggle,
    PathField,
    ConfirmBtn,
}

pub struct ExportState {
    pub decks:          Vec<String>,
    pub list_state:     ListState,
    pub reset_metadata: bool,
    pub focus:          ExportFocus,
    pub path:           String,
    pub path_edited:    bool,
    pub status:         Option<String>,
}

impl ExportState {
    pub fn new(decks: Vec<String>) -> Self {
        let mut ls = ListState::default();
        ls.select(Some(0));
        Self {
            decks,
            list_state:     ls,
            reset_metadata: false,
            focus:          ExportFocus::DeckList,
            path:           "export.json".to_string(),
            path_edited:    false,
            status:         None,
        }
    }

    pub fn selected_deck(&self) -> Option<&str> {
        self.list_state
            .selected()
            .and_then(|i| if i == 0 { None } else { self.decks.get(i - 1).map(|s| s.as_str()) })
    }

    pub fn list_len(&self) -> usize { self.decks.len() + 1 }

    pub fn list_next(&mut self) {
        let n = self.list_len();
        let i = self.list_state.selected().map_or(0, |i| (i + 1).min(n - 1));
        self.list_state.select(Some(i));
        self.refresh_default_path();
    }

    pub fn list_prev(&mut self) {
        let i = self.list_state.selected().map_or(0, |i| i.saturating_sub(1));
        self.list_state.select(Some(i));
        self.refresh_default_path();
    }

    /// keeps the path in sync with the selected deck unless the user has
    /// already typed a custom path
    pub fn refresh_default_path(&mut self) {
        if !self.path_edited {
            self.path = match self.selected_deck() {
                None    => "export.json".to_string(),
                Some(d) => {
                    // Replace the :: separator first so Math::Calc -> Math-Calc,
                    // then fix remaining non-alphanumeric chars to underscores
                    let slug: String = d.replace("::", "-").chars()
                        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
                        .collect();
                    format!("export_{slug}.json")
                }
            };
        }
    }
}

// ~~~ Import state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFocus {
    PathField,
    ConfirmBtn,
}

pub struct ImportState {
    pub path:   String,
    pub focus:  ImportFocus,
    pub status: Option<String>,
}

impl ImportState {
    pub fn new() -> Self {
        Self {
            path:   String::new(),
            focus:  ImportFocus::PathField,
            status: None,
        }
    }
}

// ~~~ Mouse click targets ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// Rect -> ClickTarget mapping for every clickable widget
// as they draw each frame in render.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    MenuList,
    PickSimple,
    PickMulti,
    /// click-to-focus a text field / row by its `focused` index
    AddCardField(usize),
    /// click-to-focus + immediately toggle (reversible / show_chain / daily)
    AddCardToggle(usize),
    /// click-to-focus + immediately activate (Add step / Save)
    AddCardButton(usize),
    StepsList,
    DeckSuggestions,
    ReviewCard,
    ReviewRate(u8),
    LeechMarkBlindSpot,
    LeechIgnore,
    DecksList,
    CardsList,
    TagPickerList,
    ExportDeckList,
    ExportToggle,
    ExportPathField,
    ExportConfirmBtn,
    ImportPathField,
    ImportConfirmBtn,
}

// ~~~ Application state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub const MENU_ITEMS: usize = 8;

pub struct AppState<'a> {
    pub store:       &'a Store,
    pub screen:      Screen,
    pub prev_screen: Screen,
    pub menu_state:  ListState,
    pub stats:       Option<Stats>,
    pub review:      Option<ReviewState>,
    pub add_card:    Option<AddCardState>,
    pub list_decks:  Option<ListDecksState>,
    pub list_cards:  Option<ListCardsState>,
    pub flash:       Option<String>,
    pub should_quit: bool,
    pub export:      Option<ExportState>,
    pub import:      Option<ImportState>,
    // rebuilt every frame by the renderers, checked on mouse clicks
    pub click_regions: Vec<(Rect, ClickTarget)>,
}

impl<'a> AppState<'a> {
    pub fn new(store: &'a Store) -> Self {
        let mut ms = ListState::default();
        ms.select(Some(0));
        Self {
            store,
            screen:      Screen::MainMenu,
            prev_screen: Screen::MainMenu,
            menu_state:  ms,
            stats:       None,
            review:      None,
            add_card:    None,
            list_decks:  None,
            list_cards:  None,
            flash:       None,
            should_quit: false,
            export:      None,
            import:      None,
            click_regions: Vec::new(),
        }
    }

    /// Find the topmost (most-recently-registered) click region containing
    /// (col, row), returning both its Rect (so callers can do their own
    /// row-within-list arithmetic) and the target it maps to
    pub fn hit_test(&self, col: u16, row: u16) -> Option<(Rect, ClickTarget)> {
        self.click_regions
            .iter()
            .rev()
            .find(|(r, _)| {
                col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
            })
            .copied()
    }

    /// Navigate to a new screen, remembering where we came from
    pub fn go_to(&mut self, screen: Screen) {
        self.prev_screen = self.screen.clone();
        self.screen = screen;
    }

    /// Return to the previous screen (one level only)
    pub fn go_back(&mut self) {
        let dest = self.prev_screen.clone();
        self.prev_screen = Screen::MainMenu;
        self.screen = dest;
    }

    pub fn menu_down(&mut self) {
        let i = self.menu_state.selected()
            .map_or(0, |i| (i + 1) % MENU_ITEMS);
        self.menu_state.select(Some(i));
    }

    pub fn menu_up(&mut self) {
        let i = self.menu_state.selected().map_or(0, |i| {
            if i == 0 { MENU_ITEMS - 1 } else { i - 1 }
        });
        self.menu_state.select(Some(i));
    }

    pub fn menu_select(&mut self) -> anyhow::Result<()> {
        self.flash = None;
        match self.menu_state.selected() {
            Some(0) => {
                let session = self.store.due_session(None, &SessionLimits::default())?;
                self.review = Some(ReviewState::new(session));
                self.go_to(Screen::Review);
            }
            Some(1) => {
                let decks = self.store.list_decks().unwrap_or_default();
                self.add_card = Some(AddCardState::new(decks));
                self.go_to(Screen::AddCard);
            }
            Some(2) => {
                let decks = self.store.list_decks()?;
                self.list_decks = Some(ListDecksState::new(decks));
                self.go_to(Screen::ListDecks);
            }
            Some(3) => {
                let cards = self.store.list_cards(None)?;
                self.list_cards = Some(ListCardsState::new(cards, None));
                self.go_to(Screen::ListCards);
            }
            Some(4) => {
                self.stats = Some(self.store.stats()?);
                self.go_to(Screen::Stats);
            }
            Some(5) => {
                let decks = self.store.list_decks().unwrap_or_default();
                self.export = Some(ExportState::new(decks));
                self.go_to(Screen::Export);
            }
            Some(6) => {
                self.import = Some(ImportState::new());
                self.go_to(Screen::Import);
            }
            _ => self.should_quit = true,
        }
        Ok(())
    }
}

// ~~~ Tests 

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Store {
        Store::open(":memory:").unwrap()
    }

    fn draft(key: u32, parent: Option<u32>, name: &str, answer: &str) -> StepDraft {
        StepDraft { key, db_id: None, parent, name: name.into(), answer: answer.into(), image: None }
    }

    /// t1 - t2 -+- a1 - a2   (branches "A" / "B")
    ///          +- b1
    fn fork() -> Vec<StepDraft> {
        vec![
            draft(0, None, "", "t1"),
            draft(1, Some(0), "", "t2"),
            draft(2, Some(1), "A", "a1"),
            draft(3, Some(2), "", "a2"),
            draft(4, Some(1), "B", "b1"),
        ]
    }

    fn root_fork() -> Vec<StepDraft> {
        vec![
            draft(0, None, "SSD", "s1"),
            draft(1, Some(0), "", "s2"),
            draft(2, None, "HDD", "h1"),
            draft(3, Some(2), "", "h2"),
        ]
    }

    fn chain(answers: &[&str]) -> Vec<StepDraft> {
        answers.iter().enumerate()
            .map(|(i, a)| draft(i as u32, if i == 0 { None } else { Some(i as u32 - 1) }, "", a))
            .collect()
    }

    fn add(store: &Store, d: &[StepDraft]) -> String {
        store.add_multi_card("Deck", "Q?", d, true, &ReviewMode::SpacedRepetition).unwrap()
    }

    fn item(store: &Store, card: &str, answer: &str) -> Item {
        store.load_items(card).unwrap().into_iter().find(|i| i.answer == answer).unwrap()
    }

    fn queued_paths(rs: &ReviewState) -> Vec<Vec<String>> {
        rs.session[rs.card_idx..]
            .iter()
            .map(|rc| rc.items.iter().map(|i| i.answer.clone()).collect())
            .collect()
    }

    fn run_to_end(rs: &mut ReviewState, store: &Store, confidence: u8) {
        for _ in 0..100 {
            if rs.is_done() { return; }
            rs.rate(store, confidence).unwrap();
        }
        panic!("session never finished");
    }

    // ~~ review flow ~~

    #[test]
    fn a_root_fork_session_walks_both_paths_without_inflating_the_trunk() {
        let store = mem();
        let id = add(&store, &root_fork());
        let mut rs = ReviewState::new(store.due_session(None, &SessionLimits::default()).unwrap());
        assert_eq!(queued_paths(&rs), vec![vec!["s1"], vec!["h1"]]);

        run_to_end(&mut rs, &store, 3);

        for a in ["s1", "s2", "h1", "h2"] {
            let it = item(&store, &id, a);
            assert!(it.due_at > chrono::Utc::now(), "{a} should be scheduled ahead");
            assert!(rs.session_rated.contains(&it.id), "{a} was rated");
        }
        // s1 / h1 were shown twice (alone, then as context for step 2), but the second
        // showing is a re-exposure, still a first-Good interval, not a compounded one
        let s1 = item(&store, &id, "s1");
        assert_eq!(s1.review_count, 2);
        assert_eq!(s1.interval_days, item(&store, &id, "s2").interval_days);
        // progress bookkeeping ends consistent
        assert_eq!(rs.done_items, rs.total_items);
        assert!(store.due_session(None, &SessionLimits::default()).unwrap().is_empty());
    }

    #[test]
    fn failing_a_trunk_step_queues_both_branches_for_rebuild() {
        let store = mem();
        let id = add(&store, &fork());
        for a in ["t1", "t2", "a1", "a2", "b1"] {
            store.record_review(&item(&store, &id, a).id, 3, std::time::Duration::from_secs(1), false).unwrap();
        }
        // make the trunk due again and fail it
        store.record_review(&item(&store, &id, "t2").id, 1, std::time::Duration::from_secs(1), false).unwrap();
        // t2 was rescheduled ahead but everything under it is now due
        let mut rs = ReviewState::new(store.review_card_if_due(&id).unwrap());
        assert_eq!(queued_paths(&rs), vec![vec!["t1", "t2", "a1"], vec!["t1", "t2", "b1"]]);

        // fail t1 on screen mid-path, the rest of THIS path is skipped, and the card is requeued
        rs.rate(&store, 3).unwrap(); // t1 (re-exposure of a well-known step)
        rs.rate(&store, 3).unwrap(); // t2 (re-exposure)
        rs.rate(&store, 1).unwrap(); // a1: fail
        let queued = queued_paths(&rs);
        assert!(queued.contains(&vec!["t1".into(), "t2".into(), "b1".into()]), "b1 path still queued: {queued:?}");
        assert!(rs.total_items >= rs.done_items);
        run_to_end(&mut rs, &store, 3);
        assert!(rs.is_done());
    }

    #[test]
    fn requeue_does_not_duplicate_a_queued_path_but_keeps_sibling_paths() {
        let store = mem();
        add(&store, &root_fork());
        let mut rs = ReviewState::new(store.due_session(None, &SessionLimits::default()).unwrap());
        rs.rate(&store, 3).unwrap(); // s1
        // [h1] still queued, [s1,s2] appended
        assert_eq!(queued_paths(&rs), vec![vec!["h1"], vec!["s1", "s2"]]);
        rs.rate(&store, 3).unwrap(); // h1
        assert_eq!(queued_paths(&rs), vec![vec!["s1", "s2"], vec!["h1", "h2"]]);
    }

    #[test]
    fn linear_card_still_produces_one_growing_path() {
        let store = mem();
        add(&store, &chain(&["s1", "s2", "s3"]));
        let mut rs = ReviewState::new(store.due_session(None, &SessionLimits::default()).unwrap());
        assert_eq!(queued_paths(&rs), vec![vec!["s1"]]);
        rs.rate(&store, 3).unwrap();
        assert_eq!(queued_paths(&rs), vec![vec!["s1", "s2"]]);
        rs.rate(&store, 3).unwrap();
        rs.rate(&store, 3).unwrap();
        assert_eq!(queued_paths(&rs), vec![vec!["s1", "s2", "s3"]]);
        run_to_end(&mut rs, &store, 3);
        assert!(rs.is_done());
    }

    // A single Hard on a step that is only being re-shown as context used to
    // re-lock the steps below it, including ones already rated this session
    // Those could never become un-due again (a re-exposure rating doesn't
    // reschedule), so the session kept requeueing them forever, even with 3s only
    fn assert_finishes_after_one_hard_on_context(draft: Vec<StepDraft>, hard_on: usize) {
        let store = mem();
        add(&store, &draft);
        let mut rs = ReviewState::new(store.due_session(None, &SessionLimits::default()).unwrap());
        let mut n = 0;
        while !rs.is_done() {
            assert!(n < 300, "session never finishes: queued now = {:?}", queued_paths(&rs));
            // the hard_on-th rating overall is a Hard; every other one is a 3
            let c = if n == hard_on { 2 } else { 3 };
            rs.rate(&store, c).unwrap();
            n += 1;
        }
    }

    #[test]
    fn one_hard_on_a_context_step_cannot_make_a_linear_session_endless() {
        // ratings: s1 | s1 s2 | s1(HARD, context) ...
        assert_finishes_after_one_hard_on_context(chain(&["s1", "s2", "s3"]), 3);
    }

    #[test]
    fn one_hard_on_a_context_step_cannot_make_a_tree_session_endless() {
        assert_finishes_after_one_hard_on_context(fork(), 3);
        assert_finishes_after_one_hard_on_context(fork(), 6);
        assert_finishes_after_one_hard_on_context(root_fork(), 3);
    }

    // ~~ editing a card from inside a session ~~

    #[test]
    fn refresh_card_survives_deleted_steps_mid_review() {
        let store = mem();
        let id = add(&store, &fork());
        for a in ["t1", "t2"] {
            store.record_review(&item(&store, &id, a).id, 3, std::time::Duration::from_secs(1), false).unwrap();
        }
        let mut rs = ReviewState::new(store.review_card_if_due(&id).unwrap());
        assert_eq!(queued_paths(&rs), vec![vec!["t1", "t2", "a1"], vec!["t1", "t2", "b1"]]);
        rs.item_idx = 1; // looking at t2

        let edit = |mutate: &dyn Fn(&mut Vec<StepDraft>)| {
            let card = store.load_card(&id).unwrap();
            let items = store.load_items(&id).unwrap();
            let mut d = crate::tree::drafts_from_items(card.is_tree, &items);
            mutate(&mut d);
            store.update_multi_card(&id, "Deck", "Q?", &d, true, &ReviewMode::SpacedRepetition).unwrap();
        };

        // delete branch B: its queued path goes, the on-screen path is untouched
        edit(&|d| {
            let i = d.iter().position(|x| x.answer == "b1").unwrap();
            crate::tree::remove_subtree(d, i);
        });
        rs.refresh_card(&store, &id).unwrap();
        assert_eq!(queued_paths(&rs), vec![vec!["t1", "t2", "a1"]]);
        assert_eq!(rs.item_idx, 1);

        // delete the step ON SCREEN (t2), the cursor lands on the next surviving step
        edit(&|d| {
            let i = d.iter().position(|x| x.answer == "t2").unwrap();
            crate::tree::remove_splice(d, i);
        });
        rs.refresh_card(&store, &id).unwrap();
        assert_eq!(queued_paths(&rs), vec![vec!["t1", "a1"]]);
        assert_eq!(rs.item_idx, 1);
        assert_eq!(rs.session[0].items[rs.item_idx].answer, "a1");
        assert_eq!(rs.phase, ReviewPhase::Thinking);

        // and the session can be finished without touching a deleted id
        run_to_end(&mut rs, &store, 3);
    }

    #[test]
    fn refresh_card_ends_the_session_cleanly_when_the_target_is_deleted() {
        let store = mem();
        let id = add(&store, &chain(&["s1", "s2"]));
        let mut rs = ReviewState::new(store.due_session(None, &SessionLimits::default()).unwrap());
        assert_eq!(queued_paths(&rs), vec![vec!["s1"]]);

        let card = store.load_card(&id).unwrap();
        let items = store.load_items(&id).unwrap();
        let mut d = crate::tree::drafts_from_items(card.is_tree, &items);
        d.remove(0); // delete s1 (splice): s2 becomes the only step
        d[0].parent = None;
        store.update_multi_card(&id, "Deck", "Q?", &d, true, &ReviewMode::SpacedRepetition).unwrap();

        rs.refresh_card(&store, &id).unwrap();
        // the old path's target is gone, and s2 is newly due, so it is queued instead
        assert_eq!(queued_paths(&rs), vec![vec!["s2"]]);
        run_to_end(&mut rs, &store, 3);
    }

    // ~~ the editor

    fn editor_for(store: &Store, id: &str) -> AddCardState {
        let card = store.load_card(id).unwrap();
        let items = store.load_items(id).unwrap();
        AddCardState::for_edit(&card, &items, vec![], vec![])
    }

    fn step_answers(s: &AddCardState) -> Vec<&str> {
        s.steps.iter().map(|d| d.answer.as_str()).collect()
    }

    fn type_step(s: &mut AddCardState, name: &str, answer: &str) {
        s.step_name_buf = name.into();
        s.step_buf = answer.into();
        s.commit_step();
    }

    #[test]
    fn typing_steps_one_after_another_builds_a_chain_even_on_an_existing_card() {
        let store = mem();
        let id = add(&store, &chain(&["s1", "s2", "s3"]));
        let mut s = editor_for(&store, &id);
        assert_eq!(s.step_list_state.selected(), Some(0)); // for_edit selects the first step
        type_step(&mut s, "", "s4"); // must APPEND, not fork under s1
        type_step(&mut s, "", "s5");
        assert_eq!(step_answers(&s), ["s1", "s2", "s3", "s4", "s5"]);
        assert!(crate::tree::is_linear(&s.steps));
        assert!(s.validate().is_ok());
    }

    #[test]
    fn branch_key_forks_under_the_selected_step_and_names_are_required() {
        let store = mem();
        let id = add(&store, &chain(&["s1", "s2", "s3"]));
        let mut s = editor_for(&store, &id);
        s.step_list_state.select(Some(0));
        s.begin_branch();
        assert_eq!(s.focused, 2);
        assert_eq!(s.attach_hint().as_deref(), Some("new branch under 1. s1"));
        type_step(&mut s, "", "alt");

        assert_eq!(step_answers(&s), ["s1", "s2", "s3", "alt"]);
        assert_eq!(s.steps[3].parent, Some(s.steps[0].key));
        assert!(s.validate().is_err(), "two children of s1, neither named");

        s.steps[1].name = "Main".into();
        s.steps[3].name = "Alt".into();
        assert!(s.validate().is_ok());

        // saving it produces a real fork
        store.update_multi_card(&id, "Deck", "Q?", &s.steps, true, &ReviewMode::SpacedRepetition).unwrap();
        let items = store.load_items(&id).unwrap();
        let s1 = items.iter().find(|i| i.answer == "s1").unwrap();
        assert_eq!(items.iter().filter(|i| i.parent_id.as_deref() == Some(s1.id.as_str())).count(), 2);
    }

    #[test]
    fn root_path_key_adds_an_alternative_answer_to_the_question() {
        let store = mem();
        let id = add(&store, &chain(&["s1", "s2"]));
        let mut s = editor_for(&store, &id);
        s.begin_root_path();
        assert_eq!(s.attach_hint().as_deref(), Some("new top-level path"));
        type_step(&mut s, "Other way", "o1");
        assert_eq!(crate::tree::child_count(&s.steps, None), 2);
        assert!(s.validate().is_err(), "the two root paths need names");
        s.steps[0].name = "First way".into();
        assert!(s.validate().is_ok());
        // the following step continues the NEW path (selection moved to it)
        type_step(&mut s, "", "o2");
        assert_eq!(s.steps.last().unwrap().parent, Some(s.steps[s.steps.len() - 2].key));
    }

    #[test]
    fn navigating_the_list_cancels_a_pending_branch_target() {
        let store = mem();
        let id = add(&store, &chain(&["s1", "s2"]));
        let mut s = editor_for(&store, &id);
        s.begin_branch();
        assert!(s.pending_parent.is_some());
        s.pending_parent = None; // what Up/Down do in the key handler
        type_step(&mut s, "", "s3");
        assert!(crate::tree::is_linear(&s.steps));
    }

    #[test]
    fn delete_keys_remove_one_step_or_a_whole_branch() {
        let store = mem();
        let id = add(&store, &fork());
        let mut s = editor_for(&store, &id);
        let a1 = s.steps.iter().position(|d| d.answer == "a1").unwrap();

        s.step_list_state.select(Some(a1));
        s.delete_selected_step(false); // just a1: a2 moves up under t2
        assert_eq!(step_answers(&s).len(), 4);
        let a2 = s.steps.iter().find(|d| d.answer == "a2").unwrap();
        let t2 = s.steps.iter().find(|d| d.answer == "t2").unwrap();
        assert_eq!(a2.parent, Some(t2.key));

        let t2_idx = s.steps.iter().position(|d| d.answer == "t2").unwrap();
        s.step_list_state.select(Some(t2_idx));
        s.delete_selected_step(true); // t2 and everything below it
        assert_eq!(step_answers(&s), ["t1"]);
        assert_eq!(s.step_list_state.selected(), Some(0));
    }

    #[test]
    fn insert_then_cancel_restores_the_original_chain() {
        let store = mem();
        let id = add(&store, &chain(&["s1", "s2", "s3"]));
        let mut s = editor_for(&store, &id);
        s.step_list_state.select(Some(0));
        s.insert_step_after_selected();
        assert_eq!(s.steps.len(), 4);
        assert!(s.editing_step_idx.is_some());
        s.commit_step(); // blank answer = cancel
        assert_eq!(step_answers(&s), ["s1", "s2", "s3"]);
        assert!(crate::tree::is_linear(&s.steps));
        assert_eq!(s.steps[1].parent, Some(s.steps[0].key), "s2 re-attached to s1");
    }

    #[test]
    fn a_saved_tree_reopens_in_the_editor_in_the_same_shape() {
        let store = mem();
        let id = add(&store, &fork());
        let s = editor_for(&store, &id);
        assert_eq!(step_answers(&s), ["t1", "t2", "a1", "a2", "b1"]);
        let rows = crate::tree::outline(&s.steps);
        assert_eq!(rows[2].branch, Some(false));
        assert_eq!(rows[4].branch, Some(true));
        assert_eq!(s.steps[2].name, "A");
        assert!(s.steps[0].name.is_empty(), "auto label is not shown as a name");
    }
}
