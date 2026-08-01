// ~~~ ui/state.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// All state types:
//   Screen, ReviewState, AddCardState, ListCardsState, ListDecksState,
//   ExportState, ImportState, AppState

use std::collections::HashSet;
use std::time::Instant;

use ratatui::widgets::ListState;

use crate::db::Store;
use crate::models::{Card, CardKind, CardSummary, Item, ItemKind, ReviewCard, ReviewMode, Stats, SessionLimits};

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
        }
    }

    pub fn is_done(&self) -> bool {
        self.card_idx >= self.session.len()
    }

    pub fn reveal(&mut self) {
        self.phase = ReviewPhase::Revealed;
        self.answer_scroll = 0; // reset scroll on new reveal
    }

    // appends card_id to the end of the session if it now has due items
    // but only if it isn't already in the not-yet-reviewed portion (avoid duplicates)
    pub fn requeue_if_due(&mut self, store: &Store, card_id: &str) -> anyhow::Result<()> {
        let already_queued = self.session[self.card_idx..]
            .iter()
            .any(|rc| rc.card.id == card_id);
        if already_queued {
            return Ok(());
        }

        if let Some(review_card) = store.review_card_if_due(card_id)? {
            self.total_items += review_card.items.len();
            self.session.push(review_card);
        }

        Ok(())
    }

    pub fn rate(&mut self, store: &Store, confidence: u8) -> anyhow::Result<()> {
        if self.is_done() {
            return Ok(());
        }

        // snapshot of what we need before state mutation
        let item         = &self.session[self.card_idx].items[self.item_idx];
        let item_id      = item.id.clone();
        let is_step      = item.kind == ItemKind::Step;
        let is_last_item = self.item_idx == self.session[self.card_idx].items.len() - 1;
        let card_id      = self.session[self.card_idx].card.id.clone();

        // a step is a chain re-exposure when it has already been fully
        // scheduled once during this session. Only applies to steps, simple
        // forward/reverse items are never re-shown within a session
        let chain_reexposure = is_step && self.session_rated.contains(&item_id);

        store.record_review(&item_id, confidence, self.item_started_at.elapsed(), chain_reexposure)?;

        // mark this item as having received its first full update this session
        // later appearances (chain re-exposures) will skip rescheduling
        self.session_rated.insert(item_id.clone());

        self.done_items += 1;

        if is_step && confidence == 1 {
            // failure: skip remaining steps, jump to next card
            let skipped_items = self.session[self.card_idx].items.len() - self.item_idx - 1;
            self.total_items -= skipped_items;
            self.card_idx += 1;
            self.item_idx = 0;
            self.phase = ReviewPhase::Thinking;
            self.item_started_at = Instant::now();

            // db just set all subsequent steps' due_at = now, so re-add this
            // card at the tail of the session if it now has due items
            self.requeue_if_due(store, &card_id)?;
        } else {
            self.advance();

            // after completing the last step of a multi card, the next step in
            // the chain may already be due (all brand-new steps start past-due)
            if is_last_item && is_step {
                self.requeue_if_due(store, &card_id)?;
            }
        }

        // snapshot only after all re-queuing so we don't falsely end early
        if self.is_done() && self.finished_duration.is_none() {
            self.finished_duration = Some(self.started_at.elapsed());
        }

        Ok(())
    }

    pub fn advance(&mut self) {
        self.item_idx += 1;
        self.phase = ReviewPhase::Thinking;
        self.item_started_at = Instant::now();
        self.answer_scroll = 0; // reset scroll on advance
        if let Some(card) = self.session.get(self.card_idx) {
            if self.item_idx >= card.items.len() {
                self.card_idx += 1;
                self.item_idx = 0;
            }
        }
        // Snapshot the elapsed time the moment the last item is rated.
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
    // multi-only, each step is (optional_name, answer)
    pub answer_image_path:   Option<String>,
    pub step_image_path:     Option<String>,
    pub steps:               Vec<(String, String, Option<String>)>, // (name, answer, img_path)
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

    pub fn commit_step(&mut self) {
        let answer = self.step_buf.trim().to_string();
        let name   = self.step_name_buf.trim().to_string();
        let img = self.step_image_path.take();
        if !answer.is_empty() {
            if let Some(idx) = self.editing_step_idx {
                if idx < self.steps.len() {
                    self.steps[idx] = (name, answer, img);
                }
                self.editing_step_idx = None;
                self.focused = 5; // jump back to the list after editing
            } else {
                self.steps.push((name, answer, img));
                self.step_list_state.select(Some(self.steps.len() - 1));
                self.focused = 3; // keep the cursor in the step editor for the next entry
            }
            // resets after commit
            self.step_name_buf.clear();
            self.step_buf.clear();
        } else if let Some(idx) = self.editing_step_idx {
            // cancel edit if user submits blank answer
            if idx < self.steps.len() && self.steps[idx].1.is_empty() {
                self.steps.remove(idx);
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
            AddKind::Multi if self.steps.iter().all(|(_, a, _)| a.trim().is_empty()) =>
                Err("Add at least one step with content."),
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
                s.steps = items
                    .iter()
                    .filter(|i| i.kind == ItemKind::Step)
                    .enumerate()
                    .map(|(idx, i)| {
                        // treat auto-generated "Step N" labels as unnamed so the
                        // name field starts blank and the user isn't forced to clear it
                        let auto = format!("Step {}", idx + 1);
                        let name = if i.prompt == auto { String::new() } else { i.prompt.clone() };
                        (name, i.answer.clone(), i.image_path.clone())
                    })
                    .collect();
                if !s.steps.is_empty() {
                    s.step_list_state.select(Some(0));
                }
            }
        }
        s
    }

    /// Insert a blank step immediately after the currently selected step
    /// (or at the end if nothing is selected), then jump to the step-input
    /// field so the user can type the new step content straight away
    pub fn insert_step_after_selected(&mut self) {
        let insert_at = self
            .step_list_state
            .selected()
            .map(|i| i + 1)
            .unwrap_or(self.steps.len());
        self.steps.insert(insert_at, (String::new(), String::new(), None));
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
        }
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
