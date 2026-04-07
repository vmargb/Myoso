// ~~~ ui.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// All screens:
//   MainMenu -> Review | AddCard | ListCards | Stats | (Export) | Quit
//

use std::io;
use std::time::Instant;

use chrono::Utc;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};

use crate::db::Store;
use crate::models::{CardKind, CardSummary, ItemKind, ReviewCard, Stats};

// ~~~ Screens ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(PartialEq, Eq, Clone)]
enum Screen {
    MainMenu,
    Stats,
    Review,
    AddCard,
    ListDecks,
    ListCards,
}

// ~~~ Review state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewPhase {
    Thinking,
    Revealed,
}

struct ReviewState {
    session:           Vec<ReviewCard>,
    card_idx:          usize,
    item_idx:          usize,
    phase:             ReviewPhase,
    started_at:        Instant,
    item_started_at:   Instant,
    total_items:       usize,
    done_items:        usize,
    finished_duration: Option<std::time::Duration>,
}

impl ReviewState {
    fn new(session: Vec<ReviewCard>) -> Self {
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
        }
    }

    fn is_done(&self) -> bool {
        self.card_idx >= self.session.len()
    }

    fn reveal(&mut self) {
        self.phase = ReviewPhase::Revealed;
    }

    fn rate(&mut self, store: &Store, confidence: u8) -> anyhow::Result<()> {
        if self.is_done() {
            return Ok(());
        }
        let item_id = self.session[self.card_idx].items[self.item_idx].id.clone();
        store.record_review(&item_id, confidence, self.item_started_at.elapsed())?;
        self.done_items += 1;
        self.advance();
        Ok(())
    }

    fn advance(&mut self) {
        self.item_idx += 1;
        self.phase = ReviewPhase::Thinking;
        self.item_started_at = Instant::now();
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

    fn progress(&self) -> f64 {
        if self.total_items == 0 {
            1.0
        } else {
            self.done_items as f64 / self.total_items as f64
        }
    }
}

// ~~~ AddCard state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddKind {
    Simple,
    Multi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddPhase {
    PickType,
    FillForm,
}

struct AddCardState {
    phase:         AddPhase,
    kind:          AddKind,
    // shared
    deck:          String,
    question:      String,
    // simple-only
    answer:        String,
    reversible:    bool,
    // multi-only
    steps:         Vec<String>,
    step_buf:      String,
    // navigation
    focused:       usize,
    error:         Option<String>,
    // deck suggestions
    decks:         Vec<String>,
    deck_list_idx: Option<usize>,
}

impl AddCardState {
    fn new(decks: Vec<String>) -> Self {
        Self {
            phase:         AddPhase::PickType,
            kind:          AddKind::Simple,
            deck:          String::new(),
            question:      String::new(),
            answer:        String::new(),
            reversible:    false,
            steps:         Vec::new(),
            step_buf:      String::new(),
            focused:       0,
            error:         None,
            decks,
            deck_list_idx: None,
        }
    }

    /// Decks whose name contains the currently-typed text (case-insensitive).
    fn filtered_decks(&self) -> Vec<&str> {
        let q = self.deck.trim().to_lowercase();
        self.decks.iter()
            .map(|d| d.as_str())
            .filter(|d| q.is_empty() || d.to_lowercase().contains(&q))
            .collect()
    }

    /// Return the currently highlighted suggestion as an owned String.
    fn selected_suggestion(&self) -> Option<String> {
        self.deck_list_idx
            .and_then(|i| self.filtered_decks().get(i).map(|s| s.to_string()))
    }

    /// Field layout
    ///   Simple: deck(0)  question(1)  answer(2)  reversible(3)  [Save](4)
    ///   Multi:  deck(0)  question(1)  step_input(2)              [Save](3)
    fn field_count(&self) -> usize {
        match self.kind {
            AddKind::Simple => 5,
            AddKind::Multi  => 4,
        }
    }

    fn next_field(&mut self) {
        self.focused = (self.focused + 1) % self.field_count();
    }

    fn prev_field(&mut self) {
        let n = self.field_count();
        self.focused = if self.focused == 0 { n - 1 } else { self.focused - 1 };
    }

    fn is_save(&self)       -> bool { self.focused == self.field_count() - 1 }
    fn is_reversible(&self) -> bool { self.kind == AddKind::Simple && self.focused == 3 }
    fn is_step_input(&self) -> bool { self.kind == AddKind::Multi  && self.focused == 2 }

    fn active_buf_mut(&mut self) -> Option<&mut String> {
        match self.kind {
            AddKind::Simple => match self.focused {
                0 => Some(&mut self.deck),
                1 => Some(&mut self.question),
                2 => Some(&mut self.answer),
                _ => None,
            },
            AddKind::Multi => match self.focused {
                0 => Some(&mut self.deck),
                1 => Some(&mut self.question),
                2 => Some(&mut self.step_buf),
                _ => None,
            },
        }
    }

    fn push_char(&mut self, c: char) {
        if let Some(b) = self.active_buf_mut() { b.push(c); }
    }

    fn pop_char(&mut self) {
        if let Some(b) = self.active_buf_mut() { b.pop(); }
    }

    fn commit_step(&mut self) {
        let step = self.step_buf.trim().to_string();
        if !step.is_empty() {
            self.steps.push(step);
            self.step_buf.clear();
        }
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.deck.trim().is_empty()     { return Err("Deck name cannot be empty."); }
        if self.question.trim().is_empty() { return Err("Question cannot be empty."); }
        match self.kind {
            AddKind::Simple if self.answer.trim().is_empty() =>
                Err("Answer cannot be empty."),
            AddKind::Multi if self.steps.is_empty() =>
                Err("Add at least one step (type in the step field, then press Enter)."),
            _ => Ok(()),
        }
    }
}

// ~~~ ListCards state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

struct ListCardsState {
    cards:          Vec<CardSummary>,
    list_state:     ListState,
    deck:           Option<String>,
    confirm_delete: bool,
}

impl ListCardsState {
    fn new(cards: Vec<CardSummary>, deck: Option<String>) -> Self {
        let mut ls = ListState::default();
        if !cards.is_empty() { ls.select(Some(0)); }
        Self { cards, list_state: ls, deck, confirm_delete: false }
    }

    fn next(&mut self) {
        if self.cards.is_empty() { return; }
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % self.cards.len());
        self.list_state.select(Some(i));
    }

    fn prev(&mut self) {
        if self.cards.is_empty() { return; }
        let n = self.cards.len();
        let i = self.list_state.selected()
            .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
        self.list_state.select(Some(i));
    }
}

// ~~~ ListDecks state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

struct ListDecksState {
    decks:          Vec<String>,
    list_state:     ListState,
    confirm_delete: bool,
}

impl ListDecksState {
    fn new(decks: Vec<String>) -> Self {
        let mut ls = ListState::default();
        if !decks.is_empty() { ls.select(Some(0)); }
        Self { decks, list_state: ls, confirm_delete: false }
    }

    fn next(&mut self) {
        if self.decks.is_empty() { return; }
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % self.decks.len());
        self.list_state.select(Some(i));
    }

    fn prev(&mut self) {
        if self.decks.is_empty() { return; }
        let n = self.decks.len();
        let i = self.list_state.selected()
            .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
        self.list_state.select(Some(i));
    }

    fn selected_deck(&self) -> Option<&str> {
        self.list_state.selected().and_then(|i| self.decks.get(i).map(|s| s.as_str()))
    }
}

// ~~~ Application state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

const MENU_ITEMS: usize = 7;

struct AppState<'a> {
    store:       &'a Store,
    screen:      Screen,
    prev_screen: Screen,
    menu_state:  ListState,
    stats:       Option<Stats>,
    review:      Option<ReviewState>,
    add_card:    Option<AddCardState>,
    list_decks:  Option<ListDecksState>,
    list_cards:  Option<ListCardsState>,
    flash:       Option<String>,
    should_quit: bool,
}

impl<'a> AppState<'a> {
    fn new(store: &'a Store) -> Self {
        let mut ms = ListState::default();
        ms.select(Some(0));
        Self {
            store,
            screen: Screen::MainMenu,
            prev_screen: Screen::MainMenu,
            menu_state: ms,
            stats: None,
            review: None,
            add_card: None,
            list_decks: None,
            list_cards: None,
            flash: None,
            should_quit: false,
        }
    }

    /// Navigate to a new screen, remembering where we came from.
    fn go_to(&mut self, screen: Screen) {
        self.prev_screen = self.screen.clone();
        self.screen = screen;
    }

    /// Return to the previous screen (one level only).
    fn go_back(&mut self) {
        let dest = self.prev_screen.clone();
        self.prev_screen = Screen::MainMenu;
        self.screen = dest;
    }

    fn menu_down(&mut self) {
        let i = self.menu_state.selected()
            .map_or(0, |i| (i + 1) % MENU_ITEMS);
        self.menu_state.select(Some(i));
    }

    fn menu_up(&mut self) {
        let i = self.menu_state.selected().map_or(0, |i| {
            if i == 0 { MENU_ITEMS - 1 } else { i - 1 }
        });
        self.menu_state.select(Some(i));
    }

    fn menu_select(&mut self) -> anyhow::Result<()> {
        self.flash = None;
        match self.menu_state.selected() {
            Some(0) => {
                let session = self.store.due_session(None)?;
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
                let bytes = self.store.export_json()?;
                std::fs::write("export.json", &bytes)?;
                self.flash = Some("✓ Exported to export.json".into());
            }
            _ => self.should_quit = true,
        }
        Ok(())
    }
}

// ~~~ Entry point ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub fn run_tui(store: &Store) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let mut app = AppState::new(store);
    let res = event_loop(&mut terminal, &mut app);
    // Always restore the terminal, even on error.
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    res
}

// ~~~ Event loop ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> anyhow::Result<()> {
    loop {
        if app.should_quit { break; }

        terminal.draw(|f| {
            match app.screen {
                Screen::MainMenu  => render_menu(f, app),
                Screen::Stats     => render_stats(f, app),
                Screen::Review    => render_review(f, app),
                Screen::AddCard   => render_add_card(f, app),
                Screen::ListDecks => render_list_decks(f, app),
                Screen::ListCards => render_list_cards(f, app),
            }
        })?;

        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }
                match app.screen {
                    Screen::MainMenu  => on_menu(app, key.code)?,
                    Screen::Stats     => on_stats(app, key.code),
                    Screen::Review    => on_review(app, key.code)?,
                    Screen::AddCard   => on_add_card(app, key.code)?,
                    Screen::ListDecks => on_list_decks(app, key.code)?,
                    Screen::ListCards => on_list_cards(app, key.code)?,
                }
            }
        }
    }
    Ok(())
}

// ~~~ Key handlers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn on_menu(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    match code {
        KeyCode::Char('q') | KeyCode::Esc       => app.should_quit = true,
        KeyCode::Down      | KeyCode::Char('j') => app.menu_down(),
        KeyCode::Up        | KeyCode::Char('k') => app.menu_up(),
        KeyCode::Enter                          => app.menu_select()?,
        _ => {}
    }
    Ok(())
}

fn on_stats(app: &mut AppState, code: KeyCode) {
    if matches!(code, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter) {
        app.go_back();
    }
}

fn on_review(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    // Copy &'a Store pointer into local.  It's a Copy type so this ends
    // the borrow on app immediately, leaving app.review free to borrow
    // mutably in the match arms below
    let store   = app.store;
    let is_done = app.review.as_ref().map_or(true, |r| r.is_done());
    let phase   = app.review.as_ref().map(|r| r.phase);

    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.go_back();
        }
        KeyCode::Enter | KeyCode::Char(' ') if !is_done => {
            if let Some(r) = app.review.as_mut() { r.reveal(); }
        }
        KeyCode::Char(c @ '1'..='5') if !is_done && phase == Some(ReviewPhase::Revealed) => {
            if let Some(r) = app.review.as_mut() {
                r.rate(store, c as u8 - b'0')?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn on_add_card(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    if app.add_card.is_none() { return Ok(()); }

    let phase         = app.add_card.as_ref().unwrap().phase;
    let is_save       = app.add_card.as_ref().unwrap().is_save();
    let is_rev        = app.add_card.as_ref().unwrap().is_reversible();
    let is_step       = app.add_card.as_ref().unwrap().is_step_input();
    let is_deck_field = app.add_card.as_ref().unwrap().focused == 0;
    let has_deck_sel  = app.add_card.as_ref().unwrap().deck_list_idx.is_some();
    let sugg_count    = app.add_card.as_ref().unwrap().filtered_decks().len();

    // ~~ Pick-type phase ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if phase == AddPhase::PickType {
        match code {
            KeyCode::Esc => app.go_back(),
            KeyCode::Left  | KeyCode::Char('h') | KeyCode::Char('1') => {
                app.add_card.as_mut().unwrap().kind = AddKind::Simple;
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('2') => {
                app.add_card.as_mut().unwrap().kind = AddKind::Multi;
            }
            KeyCode::Enter => {
                app.add_card.as_mut().unwrap().phase = AddPhase::FillForm;
            }
            _ => {}
        }
        return Ok(());
    }

    // ~~ Fill-form phase ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    match code {
        KeyCode::Esc => app.go_back(),

        // Deck suggestion navigation
        KeyCode::Down if is_deck_field => {
            if sugg_count > 0 {
                let s = app.add_card.as_mut().unwrap();
                s.deck_list_idx = Some(match s.deck_list_idx {
                    None    => 0,
                    Some(i) => (i + 1).min(sugg_count - 1),
                });
            }
        }
        KeyCode::Up if is_deck_field => {
            let s = app.add_card.as_mut().unwrap();
            s.deck_list_idx = match s.deck_list_idx {
                None | Some(0) => None,
                Some(i)        => Some(i - 1),
            };
        }

        KeyCode::Tab => {
            let s = app.add_card.as_mut().unwrap();
            s.deck_list_idx = None;
            s.next_field();
        }
        KeyCode::BackTab => {
            let s = app.add_card.as_mut().unwrap();
            s.deck_list_idx = None;
            s.prev_field();
        }

        KeyCode::Char(' ') if is_rev => {
            let s = app.add_card.as_mut().unwrap();
            s.reversible = !s.reversible;
        }

        // Select highlighted deck suggestion
        KeyCode::Enter if is_deck_field && has_deck_sel => {
            let selected = app.add_card.as_ref().unwrap().selected_suggestion();
            if let Some(d) = selected {
                let s = app.add_card.as_mut().unwrap();
                s.deck = d;
                s.deck_list_idx = None;
                s.next_field();
            }
        }

        KeyCode::Enter if is_save => {
            match save_new_card(app) {
                Ok(()) => {
                    app.flash    = Some("✓ Card saved!".into());
                    app.add_card = None;
                    app.go_back();
                }
                Err(e) => {
                    if let Some(s) = app.add_card.as_mut() {
                        s.error = Some(e.to_string());
                    }
                }
            }
        }

        KeyCode::Enter if is_step => {
            app.add_card.as_mut().unwrap().commit_step();
        }

        KeyCode::Enter => {
            app.add_card.as_mut().unwrap().next_field();
        }

        KeyCode::Backspace => {
            let s = app.add_card.as_mut().unwrap();
            if s.focused == 0 { s.deck_list_idx = None; }
            s.pop_char();
        }

        KeyCode::Char(c) if !is_rev && !is_save => {
            let s = app.add_card.as_mut().unwrap();
            if s.focused == 0 { s.deck_list_idx = None; }
            s.push_char(c);
        }

        _ => {}
    }
    Ok(())
}

fn on_list_cards(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    // If a confirm dialog is showing, handle it exclusively
    if app.list_cards.as_ref().map_or(false, |lc| lc.confirm_delete) {
        if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            let card_id = app.list_cards.as_ref()
                .and_then(|lc| lc.list_state.selected().and_then(|i| lc.cards.get(i)))
                .map(|c| c.card_id.clone());
            if let Some(id) = card_id {
                app.store.delete_card(&id)?;
                let deck = app.list_cards.as_ref().and_then(|lc| lc.deck.clone());
                let cards = app.store.list_cards(deck.as_deref())?;
                app.list_cards = Some(ListCardsState::new(cards, deck));
            }
        } else {
            if let Some(lc) = app.list_cards.as_mut() { lc.confirm_delete = false; }
        }
        return Ok(());
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.go_back(),
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(lc) = app.list_cards.as_mut() { lc.next(); }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(lc) = app.list_cards.as_mut() { lc.prev(); }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(lc) = app.list_cards.as_mut() {
                if !lc.cards.is_empty() && lc.list_state.selected().is_some() {
                    lc.confirm_delete = true;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn on_list_decks(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    // Confirm dialog takes exclusive input
    if app.list_decks.as_ref().map_or(false, |ld| ld.confirm_delete) {
        if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            let deck = app.list_decks.as_ref()
                .and_then(|ld| ld.selected_deck().map(|s| s.to_string()));
            if let Some(deck) = deck {
                app.store.delete_deck(&deck)?;
                let decks = app.store.list_decks()?;
                app.list_decks = Some(ListDecksState::new(decks));
            }
        } else {
            if let Some(ld) = app.list_decks.as_mut() { ld.confirm_delete = false; }
        }
        return Ok(());
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.go_back();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ld) = app.list_decks.as_mut() { ld.next(); }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ld) = app.list_decks.as_mut() { ld.prev(); }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(ld) = app.list_decks.as_mut() {
                if !ld.decks.is_empty() && ld.list_state.selected().is_some() {
                    ld.confirm_delete = true;
                }
            }
        }
        // [Enter] or [r]: start a review session for the selected deck
        KeyCode::Enter | KeyCode::Char('r') => {
            let deck = app.list_decks.as_ref()
                .and_then(|ld| ld.selected_deck().map(|s| s.to_string()));
            if let Some(deck) = deck {
                let session = app.store.due_session(Some(&deck))?;
                app.review = Some(ReviewState::new(session));
                app.go_to(Screen::Review);
            }
        }
        // [l]: list cards for the selected deck
        KeyCode::Char('l') => {
            let deck = app.list_decks.as_ref()
                .and_then(|ld| ld.selected_deck().map(|s| s.to_string()));
            if let Some(deck) = deck {
                let cards = app.store.list_cards(Some(&deck))?;
                app.list_cards = Some(ListCardsState::new(cards, Some(deck)));
                app.go_to(Screen::ListCards);
            }
        }
        _ => {}
    }
    Ok(())
}

// ~~~ Save helper ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn save_new_card(app: &mut AppState) -> anyhow::Result<()> {
    // Copy &'a Store pointer copy, lifetime 'a is independent of `app`,
    // so holding `s = &app.add_card` simultaneously is fine.
    let store = app.store;

    let s = app.add_card.as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing add-card state"))?;
    s.validate().map_err(|e| anyhow::anyhow!(e))?;

    match s.kind {
        AddKind::Simple => store.add_simple_card(
            s.deck.trim(),
            s.question.trim(),
            s.answer.trim(),
            s.reversible,
        )?,
        AddKind::Multi => store.add_multi_card(
            s.deck.trim(),
            s.question.trim(),
            &s.steps,
        )?,
    }
    Ok(())
}

// ~~~ Layout / style helpers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Centre a rect that is `pct_x` percent wide and exactly `height` rows tall.
fn centered_rect(pct_x: u16, height: u16, r: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(r.height.saturating_sub(height) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vert[1])[1]
}

/// Bordered block whose border thickens and turns cyan when focused.
fn field_block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .title(title)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        })
}

/// Append a block-cursor glyph when the field is active.
fn with_cursor(s: &str, focused: bool) -> String {
    if focused { format!("{}\u{258C}", s) } else { s.to_owned() }
}

/// White when focused, dark-grey otherwise.
fn text_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

// ~~~ Renderers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn render_menu(f: &mut Frame, app: &mut AppState) {
    let size = f.area();
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Length(3),
            Constraint::Length(MENU_ITEMS as u16 + 2),
            Constraint::Min(1),
        ])
        .split(size);

    // Horizontal centering helper (40 % column)
    let centre = |area: Rect| {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ])
            .split(area)[1]
    };

    // Title
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Myoso",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Step-by-step flashcards for the terminal",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .alignment(Alignment::Center),
        v[1],
    );

    // Menu list
    let items = [
        " >  Start Review Session",
        " +  Add Card",
        " *  Browse Decks",
        " =  All Cards",
        " @  Statistics",
        " ~  Export to JSON",
        " x  Quit",
    ]
    .iter()
    .map(|s| ListItem::new(*s))
    .collect::<Vec<_>>();

    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Menu "),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> "),
        centre(v[2]),
        &mut app.menu_state,
    );

    // Flash message (e.g. "Card saved!" or "Exported")
    if let Some(ref msg) = app.flash {
        f.render_widget(
            Paragraph::new(Span::styled(msg.as_str(), Style::default().fg(Color::Green)))
                .alignment(Alignment::Center),
            centre(v[3]),
        );
    }
}

// ~~ Stats ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn render_stats(f: &mut Frame, app: &AppState) {
    let size = f.area();
    let area = centered_rect(50, 10, size);
    if let Some(ref st) = app.stats {
        let body = vec![
            Line::from(vec![
                Span::raw("  Total cards : "),
                Span::styled(st.cards.to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Total items : "),
                Span::styled(st.items.to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Due now     : "),
                Span::styled(st.due_items.to_string(),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Reviews log : "),
                Span::styled(st.review_logs.to_string(),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  [Enter] / [Esc] to return",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        f.render_widget(
            Paragraph::new(body).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Statistics "),
            ),
            area,
        );
    }
}

// ~~ Review ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn render_review(f: &mut Frame, app: &AppState) {
    let size = f.area();
    let rs = match app.review.as_ref() { Some(r) => r, None => return };

    if rs.is_done() {
        if rs.total_items == 0 {
            render_nothing_due(f, size);
        } else {
            render_review_done(f, rs, size);
        }
        return;
    }

    let rc   = &rs.session[rs.card_idx];
    let item = &rc.items[rs.item_idx];

    // Three-row layout: progress bar | content | key-hint footer
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(size);

    // ~~ Progress bar ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let bar_title = format!(
        " {}/{} items  •  card {}/{} ",
        rs.done_items + 1, rs.total_items,
        rs.card_idx + 1,   rs.session.len(),
    );
    f.render_widget(
        Gauge::default()
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(bar_title))
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
            .ratio(rs.progress()),
        v[0],
    );

    // ~~ Content (question / answer) ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(v[1]);

    let prompt_label = match item.kind {
        ItemKind::Reverse => format!(" {} | {} | A->Q ", rc.card.deck, rc.card.kind),
        _                 => format!(" {} | {} ", rc.card.deck, rc.card.kind),
    };
    f.render_widget(
        Paragraph::new(item.prompt.as_str())
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(prompt_label)
                .title_style(Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        content[0],
    );

    // Build answer/step panel lines
    let mut lines: Vec<Line> = Vec::new();

    if rc.card.kind == CardKind::Multi {
        // Show already-answered steps as dim context
        for (i, prev) in rc.items[..rs.item_idx].iter().enumerate() {
            lines.push(Line::from(Span::styled(
                format!("  Step {} ✓", i + 1),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
            )));
            for l in prev.answer.lines() {
                lines.push(Line::from(Span::styled(
                    format!("    {l}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("  Step {} — what comes next?", rs.item_idx + 1),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
    } else {
        let hint = match item.kind {
            ItemKind::Forward => "  Answer",
            ItemKind::Reverse => "  Question  (reverse card)",
            ItemKind::Step    => "  Step",
        };
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));

    match rs.phase {
        ReviewPhase::Thinking => {
            lines.push(Line::from(Span::styled(
                "  [ Space / Enter to reveal ]",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            )));
        }
        ReviewPhase::Revealed => {
            for l in item.answer.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {l}"),
                    Style::default().fg(Color::Green),
                )));
            }
        }
    }

    let ans_title = if rs.phase == ReviewPhase::Thinking {
        " Answer (hidden) "
    } else {
        " Answer "
    };
    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(ans_title))
            .wrap(Wrap { trim: false }),
        content[1],
    );

    // ~~ Key-hint footer ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let footer = if rs.phase == ReviewPhase::Thinking {
        Line::from(vec![
            Span::styled(" [Space] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Reveal    "),
            Span::styled(" [q] ", Style::default().fg(Color::DarkGray)),
            Span::raw("Quit session"),
        ])
    } else {
        Line::from(vec![
            Span::styled(" [1] ", Style::default().fg(Color::Red)),
            Span::raw("Again  "),
            Span::styled(" [2] ", Style::default().fg(Color::Yellow)),
            Span::raw("Hard  "),
            Span::styled(" [3] ", Style::default().fg(Color::Green)),
            Span::raw("Good  "),
            Span::styled(" [4] ", Style::default().fg(Color::Cyan)),
            Span::raw("Great  "),
            Span::styled(" [5] ", Style::default().fg(Color::Blue)),
            Span::raw("Easy"),
        ])
    };
    f.render_widget(
        Paragraph::new(footer)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded))
            .alignment(Alignment::Center),
        v[2],
    );
}

fn render_nothing_due(f: &mut Frame, size: Rect) {
    let area = centered_rect(50, 7, size);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                " Nothing due right now!",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(" All caught up, come back later."),
            Line::from(""),
            Line::from(Span::styled(
                " [q] / [Esc] to return",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Review ")),
        area,
    );
}

fn render_review_done(f: &mut Frame, rs: &ReviewState, size: Rect) {
    let area    = centered_rect(50, 9, size);
    let elapsed = rs.finished_duration.unwrap_or_else(|| rs.started_at.elapsed());
    let (m, s)  = (elapsed.as_secs() / 60, elapsed.as_secs() % 60);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                " Session complete!",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("  Items reviewed : "),
                Span::styled(rs.done_items.to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Time taken     : "),
                Span::styled(format!("{m:02}:{s:02}"),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  [q] / [Esc] to return",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Done ")),
        area,
    );
}

// ~~ Add Card ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn render_add_card(f: &mut Frame, app: &AppState) {
    let size = f.area();
    let s = match app.add_card.as_ref() { Some(s) => s, None => return };
    match s.phase {
        AddPhase::PickType => render_pick_type(f, s, size),
        AddPhase::FillForm => match s.kind {
            AddKind::Simple => render_simple_form(f, s, size),
            AddKind::Multi  => render_multi_form(f, s, size),
        },
    }
}

fn render_pick_type(f: &mut Frame, s: &AddCardState, size: Rect) {
    let area = centered_rect(60, 14, size);
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // heading
            Constraint::Length(5),  // Simple option
            Constraint::Length(5),  // Multi option
            Constraint::Min(2),     // help line
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            "Choose card type",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        v[0],
    );

    for (slot, label, desc, kind) in [
        (1usize, "[1] Simple",     "  Q → A  (optionally reversible)", AddKind::Simple),
        (2,      "[2] Multi-step", "  One question, N ordered answer steps", AddKind::Multi),
    ] {
        let selected = s.kind == kind;
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    label,
                    if selected {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                )),
                Line::from(Span::styled(desc, Style::default().fg(Color::DarkGray))),
            ])
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if selected {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                })),
            v[slot],
        );
    }

    f.render_widget(
        Paragraph::new(Span::styled(
            "[1/2] or [←/→] to pick  •  [Enter] confirm  •  [Esc] cancel",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        v[3],
    );
}

fn render_simple_form(f: &mut Frame, s: &AddCardState, size: Rect) {
    let filtered = s.filtered_decks();
    let sugg_h: u16 = if s.focused == 0 && !filtered.is_empty() {
        (filtered.len() as u16 + 2).min(6)
    } else { 0 };

    let total_h = 1 + 3 + sugg_h + 4 + 4 + 3 + 3 + 1;
    let area = centered_rect(72, total_h, size);
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),       // heading
            Constraint::Length(3),       // deck input
            Constraint::Length(sugg_h),  // deck suggestions
            Constraint::Length(4),       // question
            Constraint::Length(4),       // answer
            Constraint::Length(3),       // reversible
            Constraint::Length(3),       // save button
            Constraint::Min(1),          // error / help
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            "Add Simple Card",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        v[0],
    );

    f.render_widget(
        Paragraph::new(with_cursor(&s.deck, s.focused == 0))
            .block(field_block(" Deck ", s.focused == 0))
            .style(text_style(s.focused == 0)),
        v[1],
    );

    if sugg_h > 0 {
        let items: Vec<ListItem> = filtered.iter().enumerate().map(|(i, d)| {
            let selected = s.deck_list_idx == Some(i);
            ListItem::new(Line::from(Span::styled(
                format!("  {d}"),
                if selected {
                    Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            )))
        }).collect();
        f.render_widget(
            List::new(items).block(
                Block::default().borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Existing decks  [↓/↑] select  [Enter] confirm "),
            ),
            v[2],
        );
    }

    f.render_widget(
        Paragraph::new(with_cursor(&s.question, s.focused == 1))
            .block(field_block(" Question ", s.focused == 1))
            .wrap(Wrap { trim: false })
            .style(text_style(s.focused == 1)),
        v[3],
    );

    f.render_widget(
        Paragraph::new(with_cursor(&s.answer, s.focused == 2))
            .block(field_block(" Answer ", s.focused == 2))
            .wrap(Wrap { trim: false })
            .style(text_style(s.focused == 2)),
        v[4],
    );

    let rev_text = if s.reversible {
        "  y  Also create A -> Q reverse card"
    } else {
        "  n  One-way only (Q -> A)"
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            rev_text,
            if s.focused == 3 {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ))
        .block(field_block(" Reversible  [Space to toggle] ", s.focused == 3)),
        v[5],
    );

    render_save_btn(f, s.focused == 4, v[6]);
    render_form_hint(f, s.error.as_deref(), v[7]);
}

fn render_multi_form(f: &mut Frame, s: &AddCardState, size: Rect) {
    let filtered = s.filtered_decks();
    let sugg_h: u16 = if s.focused == 0 && !filtered.is_empty() {
        (filtered.len() as u16 + 2).min(6)
    } else { 0 };

    let steps_h = ((s.steps.len() as u16) + 2).clamp(4, 6);
    let total_h = 1 + 3 + sugg_h + 4 + steps_h + 3 + 3 + 1;
    let area = centered_rect(72, total_h, size);
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(sugg_h),
            Constraint::Length(4),
            Constraint::Length(steps_h),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            "Add Multi-step Card",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        v[0],
    );

    f.render_widget(
        Paragraph::new(with_cursor(&s.deck, s.focused == 0))
            .block(field_block(" Deck ", s.focused == 0))
            .style(text_style(s.focused == 0)),
        v[1],
    );

    if sugg_h > 0 {
        let items: Vec<ListItem> = filtered.iter().enumerate().map(|(i, d)| {
            let selected = s.deck_list_idx == Some(i);
            ListItem::new(Line::from(Span::styled(
                format!("  {d}"),
                if selected {
                    Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            )))
        }).collect();
        f.render_widget(
            List::new(items).block(
                Block::default().borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Existing decks  [down/up] select  [Enter] confirm "),
            ),
            v[2],
        );
    }

    f.render_widget(
        Paragraph::new(with_cursor(&s.question, s.focused == 1))
            .block(field_block(" Question ", s.focused == 1))
            .wrap(Wrap { trim: false })
            .style(text_style(s.focused == 1)),
        v[3],
    );

    let step_lines: Vec<Line> = if s.steps.is_empty() {
        vec![Line::from(Span::styled(
            "  (no steps yet)",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        s.steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                Line::from(Span::styled(
                    format!("  {}. {}", i + 1, step),
                    Style::default().fg(Color::Green),
                ))
            })
            .collect()
    };
    f.render_widget(
        Paragraph::new(step_lines)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" Steps ({}) ", s.steps.len()))
                .border_style(Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: false }),
        v[4],
    );

    f.render_widget(
        Paragraph::new(with_cursor(&s.step_buf, s.focused == 2))
            .block(field_block(" New Step  [Enter to add] ", s.focused == 2))
            .wrap(Wrap { trim: false })
            .style(text_style(s.focused == 2)),
        v[5],
    );

    render_save_btn(f, s.focused == 3, v[6]);
    render_form_hint(f, s.error.as_deref(), v[7]);
}

fn render_save_btn(f: &mut Frame, focused: bool, area: Rect) {
    f.render_widget(
        Paragraph::new(Span::styled(
            "  ►  Save Card",
            if focused {
                Style::default()
                    .bg(Color::Green)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ))
        .block(field_block("", focused)),
        area,
    );
}

fn render_form_hint(f: &mut Frame, error: Option<&str>, area: Rect) {
    let line = if let Some(e) = error {
        Line::from(Span::styled(
            format!("  ✗ {e}"),
            Style::default().fg(Color::Red),
        ))
    } else {
        Line::from(Span::styled(
            "  [Tab] next  │  [Shift+Tab] prev  │  [Enter] on Save to confirm  │  [Esc] cancel",
            Style::default().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}

fn render_confirm_dialog(f: &mut Frame, msg: &str, size: Rect) {
    let area = centered_rect(54, 7, size);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                msg,
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  [y] Yes, delete    [any other key] Cancel",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Thick)
                .border_style(Style::default().fg(Color::Red))
                .title(" Confirm Delete "),
        ),
        area,
    );
}

// ~~ List Cards ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn render_list_cards(f: &mut Frame, app: &mut AppState) {
    let size = f.area();
    let lc = match app.list_cards.as_mut() { Some(lc) => lc, None => return };

    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(size);

    let now         = Utc::now();
    // Grab count before building `items` so the borrow ends before we need
    // `&mut lc.list_state` for `render_stateful_widget`.
    let card_count  = lc.cards.len();

    let items: Vec<ListItem> = if lc.cards.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No cards yet — use Add Card to create one.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        lc.cards
            .iter()
            .map(|c| {
                // All content is owned (format! / collect) so no borrow escapes
                // into the ListItem — `lc.cards` can be released before we
                // mutably borrow `lc.list_state` below.
                let due_tag = if c.due_at <= now {
                    Span::styled("DUE ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("    ", Style::default())
                };
                let kind_tag = Span::styled(
                    format!("[{:6}] ", c.kind.as_str()),
                    Style::default().fg(Color::Cyan),
                );
                let deck_tag = Span::styled(
                    format!("{:<14}", c.deck),
                    Style::default().fg(Color::Yellow),
                );
                let q_tag = Span::styled(
                    c.question.chars().take(50).collect::<String>(),
                    Style::default().fg(Color::White),
                );
                let n_tag = Span::styled(
                    format!(
                        " ({} item{})",
                        c.item_count,
                        if c.item_count == 1 { "" } else { "s" }
                    ),
                    Style::default().fg(Color::DarkGray),
                );
                ListItem::new(Line::from(vec![
                    Span::raw(" "),
                    due_tag,
                    kind_tag,
                    deck_tag,
                    q_tag,
                    n_tag,
                ]))
            })
            .collect()
    };
    // Immutable borrow of lc.cards ended so &mut lc.list_state is now safe

    f.render_stateful_widget(
        List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(if let Some(d) = &lc.deck {
                    format!(" Cards in '{}' ({card_count}) ", d)
                } else {
                    format!(" All Cards ({card_count}) ")
                }))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        v[0],
        &mut lc.list_state,
    );

    f.render_widget(
        Paragraph::new(Span::styled(
            " [j/k] navigate  |  [d] delete card  |  [Esc] back",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        v[1],
    );

    if lc.confirm_delete {
        render_confirm_dialog(f, "  Delete this card and all its review history?", size);
    }
}

// ~~ List Decks ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn render_list_decks(f: &mut Frame, app: &mut AppState) {
    let size = f.area();
    let ld = match app.list_decks.as_mut() { Some(ld) => ld, None => return };

    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(size);

    let deck_count = ld.decks.len();

    let items: Vec<ListItem> = if ld.decks.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No decks yet -- add a card to create one.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        ld.decks
            .iter()
            .map(|d| {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("[D]  {}", d),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                ]))
            })
            .collect()
    };

    f.render_stateful_widget(
        List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" Decks ({deck_count}) ")))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        v[0],
        &mut ld.list_state,
    );

    f.render_widget(
        Paragraph::new(Span::styled(
            " [j/k] navigate  |  [Enter/r] review  |  [l] list cards  |  [d] delete deck  |  [Esc] back",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        v[1],
    );

    if ld.confirm_delete {
        render_confirm_dialog(f, "  Delete this deck and ALL its cards?", size);
    }
}
