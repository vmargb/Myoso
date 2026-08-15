// ~~~ ui/mod.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// entry point from `run_tui', event loop, and all keyboard handlers
// state types in state.rs, rendering in render.rs

pub mod state;
mod render;

use std::io;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rfd::FileDialog;

use crate::db::Store;
use crate::models::{CardKind, ItemKind, ReviewMode, SessionLimits};

use state::{
    AddCardState, AddKind, AddPhase, AppState, ClickTarget, ExportFocus, ImportFocus,
    ListCardsState, ListDecksState, MENU_ITEMS, ReviewPhase, ReviewState, Screen, SearchScope,
};
use render::row_to_list_index;

// ~~~ Entry point ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub fn run_tui(store: &Store) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let mut app = AppState::new(store);
    let res = event_loop(&mut terminal, &mut app);
    // always restore the terminal even on error
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture);
    res
}

// ~~~ Event loop ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> anyhow::Result<()> {
    loop {
        if app.should_quit { break; }

        // click regions are rebuilt fresh every frame by the renderers, since
        // layouts and screen coordinates can change between draws
        let mut clicks: Vec<(ratatui::layout::Rect, ClickTarget)> = Vec::new();
        terminal.draw(|f| {
            match app.screen {
                Screen::MainMenu  => render::render_menu(f, app, &mut clicks),
                Screen::Stats     => render::render_stats(f, app),
                Screen::Review    => render::render_review(f, app, &mut clicks),
                Screen::AddCard   => render::render_add_card(f, app, &mut clicks),
                Screen::ListDecks => render::render_list_decks(f, app, &mut clicks),
                Screen::ListCards => render::render_list_cards(f, app, &mut clicks),
                Screen::Export    => render::render_export(f, app, &mut clicks),
                Screen::Import    => render::render_import(f, app, &mut clicks),
            }
        })?;
        app.click_regions = clicks;

        if event::poll(std::time::Duration::from_millis(200))? {
            match event::read()? {
                Event::Mouse(mouse) => {
                    on_mouse(app, mouse)?;
                    continue;
                }
                Event::Key(key) => {
                if key.kind != KeyEventKind::Press { continue; }

                // Ctrl+E on any multiline field opens external editor
                if key.code == KeyCode::Char('e') // EDITOR
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                    && app.screen == Screen::AddCard
                {
                    if app.add_card.as_ref().map_or(false, |s| s.is_multiline_field()) {
                        let content = app.add_card.as_ref().and_then(|s| {
                            match (s.kind, s.focused) {
                                (AddKind::Simple, 1) => Some(s.simple_question.clone()),
                                (AddKind::Simple, 2) => Some(s.answer.clone()),
                                (AddKind::Multi,  1) => Some(s.multi_question.clone()),
                                (AddKind::Multi,  3) => Some(s.step_buf.clone()),
                                _ => None,
                            }
                        });
                        if let Some(text) = content {
                            let edited = open_in_editor(terminal, &text)?;
                            if let Some(s) = app.add_card.as_mut() {
                                match (s.kind, s.focused) {
                                    (AddKind::Simple, 1) => s.simple_question = edited,
                                    (AddKind::Simple, 2) => s.answer = edited,
                                    (AddKind::Multi,  1) => s.multi_question = edited,
                                    (AddKind::Multi,  3) => s.step_buf = edited,
                                    _ => {}
                                }
                            }
                        }
                    }
                    continue;
                }
                if key.code == KeyCode::Char('o')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                    && app.screen == Screen::AddCard
                {
                    if let Some(s) = app.add_card.as_mut() {
                        let eligible = matches!(
                            (s.kind, s.focused),
                            (AddKind::Simple, 2) | (AddKind::Multi, 3)
                        );
                        if eligible {
                            let picked = FileDialog::new()
                                .add_filter("Images", &["png","jpg","jpeg","gif","webp","bmp","svg"])
                                .pick_file()
                                .map(|p| p.to_string_lossy().to_string());
                            if picked.is_some() {
                                match (s.kind, s.focused) {
                                    (AddKind::Simple, 2) => s.answer_image_path = picked,
                                    (AddKind::Multi,  3) => s.step_image_path   = picked,
                                    _ => {}
                                }
                            }
                        }
                    }
                    continue;
                }
                match app.screen {
                    Screen::MainMenu  => on_menu(app, key.code)?,
                    Screen::Stats     => on_stats(app, key.code),
                    Screen::Review    => on_review(app, key.code)?,
                    Screen::AddCard   => on_add_card(app, key)?,
                    Screen::ListDecks => on_list_decks(app, key.code)?,
                    Screen::ListCards => on_list_cards(app, key)?,
                    Screen::Export    => on_export(app, key.code)?,
                    Screen::Import    => on_import(app, key.code)?,
                }
                }
                _ => {}
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
    let store   = app.store;
    let is_done = app.review.as_ref().map_or(true, |r| r.is_done());
    let phase   = app.review.as_ref().map(|r| r.phase);

    // weak-step handling, blocking leech-intervention prompt takes
    // over the review screens keys entirely until resolved
    let leech_is_step = app.review.as_ref()
        .and_then(|r| r.leech_prompt.as_ref())
        .map(|p| p.is_step);
    if let Some(is_step) = leech_is_step {
        match code {
            KeyCode::Char('1') => launch_leech_editor(app, false)?,
            // split only makes sense for multi-card steps so chain isn't broken
            KeyCode::Char('2') if is_step => launch_leech_editor(app, true)?,
            KeyCode::Char('3') => {
                let item_id = app.review.as_ref()
                    .and_then(|r| r.leech_prompt.as_ref())
                    .map(|p| p.item_id.clone());
                if let Some(id) = item_id {
                    store.enter_scaffold_mode(&id)?;
                }
                if let Some(rs) = app.review.as_mut() {
                    rs.resolve_leech_prompt(store)?;
                }
            }
            KeyCode::Char('c') | KeyCode::Esc | KeyCode::Enter => {
                if let Some(rs) = app.review.as_mut() {
                    rs.resolve_leech_prompt(store)?;
                }
            }
            _ => {}
        }
        return Ok(());
    }

    // weak-step handling
    if app.review.as_ref().and_then(|r| r.weak_span_prompt.as_ref()).is_some() {
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(rs) = app.review.as_mut() { rs.weak_span_move_cursor(1); }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(rs) = app.review.as_mut() { rs.weak_span_move_cursor(-1); }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(rs) = app.review.as_mut() { rs.weak_span_move_cursor(-1); }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(rs) = app.review.as_mut() { rs.weak_span_move_cursor(1); }
            }
            KeyCode::Char(' ') => {
                if let Some(rs) = app.review.as_mut() { rs.weak_span_toggle_cursor(); }
            }
            KeyCode::Enter => {
                if let Some(rs) = app.review.as_mut() { rs.weak_span_commit(store)?; }
            }
            KeyCode::Esc => {
                if let Some(rs) = app.review.as_mut() { rs.resolve_weak_span_prompt(store)?; }
            }
            _ => {}
        }
        return Ok(());
    }

    // scaffolded item has no reveal gate
    // cloze-blanked answer is already on screen) and only accepts pass/fail
    // ([1]/[3]) : [2]/[4] are visibly disabled in the footer and ignored here
    let is_scaffolded = app.review.as_ref()
        .filter(|r| !r.is_done())
        .map(|r| r.session[r.card_idx].items[r.item_idx].scaffold_state.is_scaffolded())
        .unwrap_or(false);
    if is_scaffolded && phase == Some(ReviewPhase::Revealed) {
        match code {
            KeyCode::Char(c @ ('1' | '3')) => {
                if let Some(r) = app.review.as_mut() {
                    r.rate(store, c as u8 - b'0')?;
                }
                return Ok(());
            }
            KeyCode::Char('2') | KeyCode::Char('4') => return Ok(()), // disabled while scaffolded
            _ => {}
        }
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.go_back();
        }
        KeyCode::Enter | KeyCode::Char(' ') if !is_done => {
            if let Some(r) = app.review.as_mut() { r.reveal(); }
        }
        KeyCode::Char(c @ '1'..='4') if !is_done && phase == Some(ReviewPhase::Revealed) => {
            if let Some(r) = app.review.as_mut() {
                r.rate(store, c as u8 - b'0')?;
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(rs) = app.review.as_mut() {
                rs.answer_scroll = rs.answer_scroll.saturating_add(1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(rs) = app.review.as_mut() {
                rs.answer_scroll = rs.answer_scroll.saturating_sub(1);
            }
        }
        KeyCode::Char('o') if phase == Some(ReviewPhase::Revealed) => {
            if let Some(rs) = app.review.as_ref() {
                if !rs.is_done() {
                    let item = &rs.session[rs.card_idx].items[rs.item_idx];
                    if let Some(path) = &item.image_path {
                        let _ = open::that(path);
                    }
                }
            }
        }
        KeyCode::Char('e') if !is_done => {
            let card_id = app.review.as_ref()
                .and_then(|rs| rs.session.get(rs.card_idx))
                .map(|rc| rc.card.id.clone());
            if let Some(id) = card_id {
                let card  = app.store.load_card(&id)?;
                let items = app.store.load_items(&id)?;
                let tags  = app.store.get_card_tags(&id)?;
                let decks = app.store.list_decks().unwrap_or_default();
                let mut edit_state = AddCardState::for_edit(&card, &items, decks, tags);
                edit_state.editing_from_review = true;
                app.add_card = Some(edit_state);
                app.go_to(Screen::AddCard);
            }
        }
        _ => {}
    }
    Ok(())
}

fn on_add_card(app: &mut AppState, key: KeyEvent) -> anyhow::Result<()> {
    if app.add_card.is_none() { return Ok(()); }

    let phase         = app.add_card.as_ref().unwrap().phase;
    let is_save       = app.add_card.as_ref().unwrap().is_save();
    let is_rev        = app.add_card.as_ref().unwrap().is_reversible();
    let is_step_name  = app.add_card.as_ref().unwrap().is_step_name();
    let is_add_step   = app.add_card.as_ref().unwrap().is_add_step_button();
    let is_steps_list = app.add_card.as_ref().unwrap().is_steps_list();
    let is_deck_field = app.add_card.as_ref().unwrap().focused == 0;
    let has_deck_sel  = app.add_card.as_ref().unwrap().deck_list_idx.is_some();
    let sugg_count    = app.add_card.as_ref().unwrap().filtered_decks().len();
    let is_editing    = app.add_card.as_ref().unwrap().editing_card_id.is_some();
    let is_show_chain = app.add_card.as_ref().unwrap().is_show_chain();
    let is_daily      = app.add_card.as_ref().unwrap().is_daily_toggle();

    // ~~ Pick-type phase ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if phase == AddPhase::PickType {
        match key.code {
            KeyCode::Esc => app.go_back(),
            KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('1') => {
                app.add_card.as_mut().unwrap().kind = AddKind::Simple;
            }
            KeyCode::Down | KeyCode::Char('l') | KeyCode::Char('2') => {
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
    match key.code {
        // Esc: go back to PickType when creating, go_back when editing
        KeyCode::Esc => {
            if is_editing {
                let from_review = app.add_card.as_ref()
                    .map_or(false, |s| s.editing_from_review);
                app.add_card = None;
                if from_review {
                    app.screen = Screen::Review;
                    // cancelling out of a leech-prompt deep-link
                    // the intervention is handle it or explicitly move on
                    let store = app.store;
                    if let Some(rs) = app.review.as_mut() {
                        rs.resolve_leech_prompt(store)?;
                    }
                } else {
                    app.go_back();
                }
            } else {
                let s = app.add_card.as_mut().unwrap();
                s.phase   = AddPhase::PickType;
                s.focused = 0;
                s.error   = None;
            }
        }

        // deck suggestion navigation
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

        // steps actions, navigation, editing
        KeyCode::Char('i') if is_steps_list => {
            app.add_card.as_mut().unwrap().insert_step_after_selected();
        }
        KeyCode::Down if is_steps_list => {
            let s = app.add_card.as_mut().unwrap();
            let n = s.steps.len();
            if n > 0 {
                let i = s.step_list_state.selected().map_or(0, |i| (i + 1).min(n - 1));
                s.step_list_state.select(Some(i));
            }
        }
        KeyCode::Up if is_steps_list => {
            let s = app.add_card.as_mut().unwrap();
            let n = s.steps.len();
            if n > 0 {
                let i = s.step_list_state.selected().map_or(n.saturating_sub(1), |i| i.saturating_sub(1));
                s.step_list_state.select(Some(i));
            }
        }
        KeyCode::Char('d') | KeyCode::Delete if is_steps_list => {
            let s = app.add_card.as_mut().unwrap();
            if let Some(i) = s.step_list_state.selected() {
                if i < s.steps.len() {
                    s.steps.remove(i);
                    let n = s.steps.len();
                    if n == 0 {
                        s.step_list_state.select(None);
                    } else if i >= n {
                        s.step_list_state.select(Some(n - 1));
                    }
                }
            }
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

        KeyCode::Char(' ') if is_show_chain => {
            let s = app.add_card.as_mut().unwrap();
            s.show_chain = !s.show_chain;
        }

        KeyCode::Char(' ') if is_daily => {
            let s = app.add_card.as_mut().unwrap();
            s.review_mode = if s.review_mode.is_daily() {
                ReviewMode::SpacedRepetition
            } else {
                ReviewMode::Daily
            };
        }

        // === edit selected step (Enter on steps list) ===
        KeyCode::Enter if is_steps_list => {
            let s = app.add_card.as_mut().unwrap();
            if let Some(idx) = s.step_list_state.selected() {
                if idx < s.steps.len() {
                    let (name, answer, img) = s.steps[idx].clone();
                    s.step_name_buf    = name;
                    s.step_buf         = answer;
                    s.step_image_path  = img;
                    s.editing_step_idx = Some(idx);
                    s.focused          = 2; // jump to Step Name field
                }
            }
        }

        // === deck suggestion selection ===
        KeyCode::Enter if is_deck_field && has_deck_sel => {
            let selected = app.add_card.as_ref().unwrap().selected_suggestion();
            if let Some(d) = selected {
                let s = app.add_card.as_mut().unwrap();
                s.deck = d;
                s.deck_list_idx = None;
                s.next_field();
            }
        }

        // === commit step button ===
        KeyCode::Enter if is_add_step => {
            let s = app.add_card.as_mut().unwrap();
            s.commit_step();
            if s.focused == 4 {
                // Empty step: treat this like a no-op and return to the editor.
                s.focused = 3;
            }
        }

        // === normal Enter = newline only in multi-line fields ===
        KeyCode::Enter if app.add_card.as_ref().unwrap().is_multiline_field() => {
            let s = app.add_card.as_mut().unwrap();
            s.error = None;
            if let Some(b) = s.active_buf_mut() {
                b.push('\n');
            }
        }

        // === other enter cases (save, step name, next field) ===
        KeyCode::Enter if is_save => {
            match save_new_card(app) {
                Ok(()) => {
                    let is_edit = app.add_card.as_ref()
                        .map_or(false, |s| s.editing_card_id.is_some());
                    app.flash = Some(if is_edit {
                        "✓ Card updated!".into()
                    } else {
                        "✓ Card saved!".into()
                    });
                    if is_edit {
                        let editing_from_review = app.add_card.as_ref()
                            .map_or(false, |s| s.editing_from_review);
                        let edited_card_id = app.add_card.as_ref()
                            .and_then(|s| s.editing_card_id.clone());
                        app.add_card = None;

                        if editing_from_review {
                            // return to the review session and patch the live card
                            app.screen = Screen::Review;
                            if let (Some(rs), Some(id)) = (app.review.as_mut(), edited_card_id) {
                                // reload the card from DB and update whichever slot in
                                // the session matches, so review_mode are current
                                if let Ok(fresh_card) = app.store.load_card(&id) {
                                    for rc in rs.session.iter_mut() {
                                        if rc.card.id == id {
                                            rc.card = fresh_card.clone();
                                        }
                                    }
                                }
                                // Rename/Split resolves the leech prompt that
                                // sent us here, resuming the rating that was paused on it
                                rs.resolve_leech_prompt(app.store)?;
                            }
                        } else {
                            app.go_back();
                            if app.screen == Screen::ListCards {
                                let deck = app.list_cards.as_ref().and_then(|lc| lc.deck.clone());
                                let sel  = app.list_cards.as_ref()
                                    .and_then(|lc| lc.list_state.selected());
                                let tag_filter = app.list_cards.as_ref()
                                    .map(|lc| lc.tag_filter.clone()).unwrap_or_default();
                                if let Ok(cards) = app.store.list_cards(deck.as_deref()) {
                                    let mut new_lc = ListCardsState::new(cards, deck);
                                    new_lc.tag_filter = tag_filter;
                                    if let Some(i) = sel {
                                        new_lc.list_state.select(Some(
                                            i.min(new_lc.cards.len().saturating_sub(1))
                                        ));
                                    }
                                    app.list_cards = Some(new_lc);
                                }
                            }
                        }
                    } else {
                        let saved_kind = app.add_card.as_ref().unwrap().kind;
                        let decks = app.store.list_decks().unwrap_or_default();
                        let mut fresh = AddCardState::new(decks);
                        fresh.kind = saved_kind;
                        app.add_card = Some(fresh);
                    }
                }
                Err(e) => {
                    if let Some(s) = app.add_card.as_mut() {
                        s.error = Some(e.to_string());
                    }
                }
            }
        }
        KeyCode::Enter if is_step_name => {
            app.add_card.as_mut().unwrap().next_field();
        }
        KeyCode::Enter => {
            app.add_card.as_mut().unwrap().next_field();
        }

        KeyCode::Backspace => {
            let s = app.add_card.as_mut().unwrap();
            if s.focused == 0 { s.deck_list_idx = None; }
            s.error = None;
            s.pop_char();
        }

        KeyCode::Char(c) if !is_rev && !is_show_chain && !is_daily && !is_save => {
            let s = app.add_card.as_mut().unwrap();
            if s.focused == 0 { s.deck_list_idx = None; }
            s.error = None;
            s.push_char(c);
        }

        _ => {}
    }
    Ok(())
}

fn on_list_cards(app: &mut AppState, key: KeyEvent) -> anyhow::Result<()> {
    let code = key.code;

    // ~~ tag picker overlay ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if app.list_cards.as_ref().map_or(false, |lc| lc.tag_picker_active) {
        match code {
            KeyCode::Esc | KeyCode::Enter => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.tag_picker_active = false;
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(lc) = app.list_cards.as_mut() {
                    let n = lc.tag_picker_tags.len();
                    if n > 0 {
                        let i = lc.tag_picker_state.selected()
                            .map_or(0, |i| (i + 1).min(n - 1));
                        lc.tag_picker_state.select(Some(i));
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(lc) = app.list_cards.as_mut() {
                    let n = lc.tag_picker_tags.len();
                    if n > 0 {
                        let i = lc.tag_picker_state.selected()
                            .map_or(0, |i| i.saturating_sub(1));
                        lc.tag_picker_state.select(Some(i));
                    }
                }
            }
            KeyCode::Char(' ') => {
                if let Some(lc) = app.list_cards.as_mut() {
                    if let Some(i) = lc.tag_picker_state.selected() {
                        if let Some(tag) = lc.tag_picker_tags.get(i).cloned() {
                            if lc.tag_filter.contains(&tag) {
                                lc.tag_filter.retain(|t| t != &tag);
                            } else {
                                lc.tag_filter.push(tag);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        return Ok(());
    }

    // ~~ search mode ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if app.list_cards.as_ref().map_or(false, |lc| lc.search_active) {
        match code {
            // enter: leave search edit mode but keep the filter
            KeyCode::Enter => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_active = false;
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }

            // esc: clear the query and leave search edit mode
            KeyCode::Esc => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_query.clear();
                    lc.search_active = false;
                    lc.search_scope = SearchScope::default();
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }

            KeyCode::Backspace => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_query.pop();
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }

            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.next();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.prev();
                }
            }

            // tab cycles the search scope Q -> A -> Q+A -> Q etc...
            KeyCode::Tab => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_scope = lc.search_scope.next();
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }

            KeyCode::Char(c) => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_query.push(c);
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }

            _ => {}
        }
        return Ok(());
    }

    // ~~ search mode ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if app.list_cards.as_ref().map_or(false, |lc| lc.search_active) {
        match code {
            KeyCode::Esc => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_query.clear();
                    lc.search_active = false;
                    lc.search_scope = SearchScope::default();
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            KeyCode::Backspace => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_query.pop();
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            // type into the search query instead for j/k (bug fix)
            KeyCode::Down => {
                if let Some(lc) = app.list_cards.as_mut() { lc.next(); }
            }
            KeyCode::Up => {
                if let Some(lc) = app.list_cards.as_mut() { lc.prev(); }
            }
            // tab cycles the search scope Q -> A -> Q+A -> Q etc...
            KeyCode::Tab => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_scope = lc.search_scope.next();
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            KeyCode::Char(c) => {
                if let Some(lc) = app.list_cards.as_mut() {
                    lc.search_query.push(c);
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            _ => {}
        }
        return Ok(());
    }
    // ~~ confirm delete ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if app.list_cards.as_ref().map_or(false, |lc| lc.confirm_delete) {
        if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            let card_id = app.list_cards.as_ref().and_then(|lc| {
                let filtered = lc.filtered_cards();
                lc.list_state.selected()
                    .and_then(|i| filtered.get(i).map(|c| c.card_id.clone()))
            });
            if let Some(id) = card_id {
                app.store.delete_card(&id)?;
                let deck       = app.list_cards.as_ref().and_then(|lc| lc.deck.clone());
                let tag_filter = app.list_cards.as_ref()
                    .map(|lc| lc.tag_filter.clone()).unwrap_or_default();
                let cards = app.store.list_cards(deck.as_deref())?;
                let mut new_lc = ListCardsState::new(cards, deck);
                new_lc.tag_filter = tag_filter;
                app.list_cards = Some(new_lc);
            }
        } else {
            if let Some(lc) = app.list_cards.as_mut() { lc.confirm_delete = false; }
        }
        return Ok(());
    }

    // ~~ normal mode ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    match code {
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(lc) = app.list_cards.as_mut() { lc.next(); }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(lc) = app.list_cards.as_mut() { lc.prev(); }
        }

        // activate text search (keep existing query so it can be edited)
        KeyCode::Char('/') => {
            if let Some(lc) = app.list_cards.as_mut() {
                lc.search_active = true;
                let n = lc.filtered_cards().len();
                lc.list_state.select(if n > 0 { Some(0) } else { None });
            }
        }

        // esc clears an active filter first, otherwise it goes back
        KeyCode::Esc => {
            if let Some(lc) = app.list_cards.as_mut() {
                if !lc.search_query.is_empty() {
                    lc.search_query.clear();
                    lc.search_scope = SearchScope::default();
                    let n = lc.filtered_cards().len();
                    lc.list_state.select(if n > 0 { Some(0) } else { None });
                } else {
                    app.go_back();
                }
            }
        }

        KeyCode::Char('q') => app.go_back(),

        // open tag-filter picker
        KeyCode::Char('#') => {
            let all_tags = app.store.list_all_tags().unwrap_or_default();
            if let Some(lc) = app.list_cards.as_mut() {
                if !all_tags.is_empty() {
                    lc.tag_picker_tags = all_tags;
                    if lc.tag_picker_state.selected().is_none() {
                        lc.tag_picker_state.select(Some(0));
                    }
                    lc.tag_picker_active = true;
                }
            }
        }

        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(lc) = app.list_cards.as_mut() {
                let fc_len = lc.filtered_cards().len();
                if fc_len > 0 && lc.list_state.selected().is_some() {
                    lc.confirm_delete = true;
                }
            }
        }

        KeyCode::Char('e') => {
            let card_id = app.list_cards.as_ref().and_then(|lc| {
                let filtered = lc.filtered_cards();
                lc.list_state.selected()
                    .and_then(|i| filtered.get(i).map(|c| c.card_id.clone()))
            });
            if let Some(id) = card_id {
                let card  = app.store.load_card(&id)?;
                let items = app.store.load_items(&id)?;
                let tags  = app.store.get_card_tags(&id)?;
                let decks = app.store.list_decks().unwrap_or_default();
                app.add_card = Some(AddCardState::for_edit(&card, &items, decks, tags));
                app.go_to(Screen::AddCard);
            }
        }

        KeyCode::Char('m') => {
            let result = app.list_cards.as_ref().and_then(|lc| {
                let filtered = lc.filtered_cards();
                lc.list_state.selected()
                    .and_then(|i| filtered.get(i).copied())
                    .map(|c| (c.card_id.clone(), c.review_mode.clone()))
            });
            if let Some((id, mode)) = result {
                let new_mode = if mode.is_daily() {
                    ReviewMode::SpacedRepetition
                } else {
                    ReviewMode::Daily
                };
                app.store.set_card_mode(&id, &new_mode)?;
                let deck       = app.list_cards.as_ref().and_then(|lc| lc.deck.clone());
                let sel        = app.list_cards.as_ref()
                    .and_then(|lc| lc.list_state.selected());
                let tag_filter = app.list_cards.as_ref()
                    .map(|lc| lc.tag_filter.clone()).unwrap_or_default();
                let cards = app.store.list_cards(deck.as_deref())?;
                let mut new_lc = ListCardsState::new(cards, deck);
                new_lc.tag_filter = tag_filter;
                if let Some(i) = sel {
                    let fc_len = new_lc.filtered_cards().len();
                    new_lc.list_state.select(Some(i.min(fc_len.saturating_sub(1))));
                }
                app.list_cards = Some(new_lc);
                app.flash = Some(format!(
                    "✓ Switched to {}",
                    if new_mode.is_daily() { "Daily" } else { "Spaced Repetition" }
                ));
            }
        }

        _ => {}
    }
    Ok(())
}

fn on_list_decks(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    // ~~ search mode ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if app.list_decks.as_ref().map_or(false, |ld| ld.search_active) {
        match code {
            KeyCode::Esc => {
                if let Some(ld) = app.list_decks.as_mut() {
                    ld.search_query.clear();
                    ld.search_active = false;
                    let n = ld.filtered_decks().len();
                    ld.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            KeyCode::Backspace => {
                if let Some(ld) = app.list_decks.as_mut() {
                    ld.search_query.pop();
                    let n = ld.filtered_decks().len();
                    ld.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ld) = app.list_decks.as_mut() { ld.next(); }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ld) = app.list_decks.as_mut() { ld.prev(); }
            }
            KeyCode::Enter | KeyCode::Char('r') => {
                let deck = app.list_decks.as_ref()
                    .and_then(|ld| ld.selected_deck().map(|s| s.to_string()));
                if let Some(deck) = deck {
                    let session = app.store.due_session(Some(&deck), &SessionLimits::default())?;
                    app.review = Some(ReviewState::new(session));
                    app.go_to(Screen::Review);
                }
            }
            KeyCode::Char(c) => {
                if let Some(ld) = app.list_decks.as_mut() {
                    ld.search_query.push(c);
                    let n = ld.filtered_decks().len();
                    ld.list_state.select(if n > 0 { Some(0) } else { None });
                }
            }
            _ => {}
        }
        return Ok(());
    }

    // ~~ confirm delete ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
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

    // ~~ normal mode ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
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
        KeyCode::Char('/') => {
            if let Some(ld) = app.list_decks.as_mut() {
                ld.search_active = true;
                ld.search_query.clear();
                let n = ld.filtered_decks().len();
                ld.list_state.select(if n > 0 { Some(0) } else { None });
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(ld) = app.list_decks.as_mut() {
                if !ld.filtered_decks().is_empty() && ld.list_state.selected().is_some() {
                    ld.confirm_delete = true;
                }
            }
        }
        KeyCode::Enter | KeyCode::Char('r') => {
            let deck = app.list_decks.as_ref()
                .and_then(|ld| ld.selected_deck().map(|s| s.to_string()));
            if let Some(deck) = deck {
                let session = app.store.due_session(Some(&deck), &SessionLimits::default())?;
                app.review = Some(ReviewState::new(session));
                app.go_to(Screen::Review);
            }
        }
        KeyCode::Char('l') => {
            let deck = app.list_decks.as_ref()
                .and_then(|ld| ld.selected_deck().map(|s| s.to_string()));
            if let Some(deck) = deck {
                let cards = app.store.list_cards(Some(&deck))?;
                app.list_cards = Some(ListCardsState::new(cards, Some(deck)));
                app.go_to(Screen::ListCards);
            }
        }
        KeyCode::Char('M') => {
            let deck = app.list_decks.as_ref()
                .and_then(|ld| ld.selected_deck().map(|s| s.to_string()));
            if let Some(deck) = deck {
                let cards = app.store.list_cards(Some(&deck))?;
                let daily_count = cards.iter().filter(|c| c.review_mode.is_daily()).count();
                let new_mode = if daily_count * 2 >= cards.len() {
                    ReviewMode::SpacedRepetition
                } else {
                    ReviewMode::Daily
                };
                app.store.set_deck_mode(&deck, &new_mode)?;
                app.flash = Some(format!(
                    "✓ Deck '{}' → {}",
                    deck,
                    if new_mode.is_daily() { "Daily" } else { "Spaced Repetition" }
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn on_export(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    let ex = match app.export.as_mut() { Some(e) => e, None => return Ok(()) };

    // [b] browse open native save dialog regardless of focus
    if code == KeyCode::Char('b') {
        let default = ex.path.clone();
        // temporarily leave raw mode so the OS dialog renders cleanly.
        let _ = disable_raw_mode();
        let picked = FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name(&default)
            .save_file();
        let _ = enable_raw_mode();
        if let Some(p) = picked {
            ex.path        = p.to_string_lossy().into_owned();
            ex.path_edited = true;
            ex.status      = None;
        }
        return Ok(());
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => { app.go_back(); return Ok(()); }

        KeyCode::Tab | KeyCode::BackTab => {
            let ex = app.export.as_mut().unwrap();
            ex.focus = match (ex.focus, code) {
                (ExportFocus::DeckList,    KeyCode::Tab)    => ExportFocus::ResetToggle,
                (ExportFocus::ResetToggle, KeyCode::Tab)    => ExportFocus::PathField,
                (ExportFocus::PathField,   KeyCode::Tab)    => ExportFocus::ConfirmBtn,
                (ExportFocus::ConfirmBtn,  KeyCode::Tab)    => ExportFocus::DeckList,
                (ExportFocus::DeckList,    _)               => ExportFocus::ConfirmBtn,
                (ExportFocus::ResetToggle, _)               => ExportFocus::DeckList,
                (ExportFocus::PathField,   _)               => ExportFocus::ResetToggle,
                (ExportFocus::ConfirmBtn,  _)               => ExportFocus::PathField,
            };
        }

        KeyCode::Down | KeyCode::Char('j') if ex.focus == ExportFocus::DeckList => {
            app.export.as_mut().unwrap().list_next();
        }
        KeyCode::Up | KeyCode::Char('k') if ex.focus == ExportFocus::DeckList => {
            app.export.as_mut().unwrap().list_prev();
        }

        KeyCode::Char(' ') if ex.focus == ExportFocus::ResetToggle => {
            let ex = app.export.as_mut().unwrap();
            ex.reset_metadata = !ex.reset_metadata;
        }

        // path field text editing
        KeyCode::Char(c) if ex.focus == ExportFocus::PathField => {
            let ex = app.export.as_mut().unwrap();
            ex.path.push(c);
            ex.path_edited = true;
            ex.status = None;
        }
        KeyCode::Backspace if ex.focus == ExportFocus::PathField => {
            let ex = app.export.as_mut().unwrap();
            ex.path.pop();
            ex.path_edited = !ex.path.is_empty();
            ex.status = None;
        }

        KeyCode::Enter if ex.focus == ExportFocus::ConfirmBtn => {
            let ex    = app.export.as_ref().unwrap();
            let deck  = ex.selected_deck().map(|s| s.to_string());
            let reset = ex.reset_metadata;
            let path  = ex.path.trim().to_string();
            let path  = if path.is_empty() { "export.json".to_string() } else { path };

            match app.store.export_json(deck.as_deref(), reset) {
                Ok(bytes) => {
                    match std::fs::write(&path, &bytes) {
                        Ok(()) => {
                            let label = deck.as_deref().unwrap_or("all decks");
                            app.export.as_mut().unwrap().status =
                                Some(format!("✓  Exported {label}  →  {path}"));
                        }
                        Err(e) => {
                            app.export.as_mut().unwrap().status =
                                Some(format!("✗  {e}"));
                        }
                    }
                }
                Err(e) => {
                    app.export.as_mut().unwrap().status = Some(format!("✗  {e}"));
                }
            }
        }

        _ => {}
    }
    Ok(())
}

fn on_import(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    let im = match app.import.as_mut() { Some(i) => i, None => return Ok(()) };

    // [b] browse open native open-file dialog regardless of focus
    if code == KeyCode::Char('b') {
        let _ = disable_raw_mode();
        let picked = FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file();
        let _ = enable_raw_mode();
        if let Some(p) = picked {
            im.path   = p.to_string_lossy().into_owned();
            im.status = None;
        }
        return Ok(());
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => { app.go_back(); return Ok(()); }

        KeyCode::Tab | KeyCode::BackTab => {
            let im = app.import.as_mut().unwrap();
            im.focus = match im.focus {
                ImportFocus::PathField  => ImportFocus::ConfirmBtn,
                ImportFocus::ConfirmBtn => ImportFocus::PathField,
            };
        }

        KeyCode::Char(c) if im.focus == ImportFocus::PathField => {
            let im = app.import.as_mut().unwrap();
            im.path.push(c);
            im.status = None;
        }
        KeyCode::Backspace if im.focus == ImportFocus::PathField => {
            let im = app.import.as_mut().unwrap();
            im.path.pop();
            im.status = None;
        }

        KeyCode::Enter if im.focus == ImportFocus::ConfirmBtn => {
            let path = app.import.as_ref().unwrap().path.trim().to_string();
            match std::fs::read(&path) {
                Err(e) => {
                    app.import.as_mut().unwrap().status = Some(format!("✗  {e}"));
                }
                Ok(bytes) => match app.store.import_json(&bytes) {
                    Err(e) => {
                        app.import.as_mut().unwrap().status = Some(format!("✗  {e}"));
                    }
                    Ok(summary) => {
                        app.import.as_mut().unwrap().status = Some(format!(
                            "✓  {} card(s) added, {} replaced, {} item(s) total",
                            summary.cards_imported,
                            summary.cards_replaced,
                            summary.items_imported,
                        ));
                    }
                },
            }
        }

        _ => {}
    }
    Ok(())
}

// ~~~ Mouse handlers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// clicks are hit-tested against the regions the last frames renderers registered
// state::ClickTarget / render::Clicks

fn on_mouse(app: &mut AppState, mouse: MouseEvent) -> anyhow::Result<()> {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => on_click(app, mouse.column, mouse.row)?,
        MouseEventKind::ScrollDown => dispatch_scroll(app, KeyCode::Down)?,
        MouseEventKind::ScrollUp   => dispatch_scroll(app, KeyCode::Up)?,
        _ => {}
    }
    Ok(())
}

/// mouse wheel: reuse whatever the Down/Up arrow already does
/// screen scroll the review answer, move a list selection.
fn dispatch_scroll(app: &mut AppState, code: KeyCode) -> anyhow::Result<()> {
    match app.screen {
        Screen::MainMenu  => on_menu(app, code)?,
        Screen::Stats     => {}
        Screen::Review    => on_review(app, code)?,
        Screen::AddCard   => on_add_card(app, KeyEvent::new(code, KeyModifiers::NONE))?,
        Screen::ListDecks => on_list_decks(app, code)?,
        Screen::ListCards => on_list_cards(app, KeyEvent::new(code, KeyModifiers::NONE))?,
        Screen::Export    => on_export(app, code)?,
        Screen::Import    => on_import(app, code)?,
    }
    Ok(())
}

fn on_click(app: &mut AppState, col: u16, row: u16) -> anyhow::Result<()> {
    let (area, target) = match app.hit_test(col, row) {
        Some(v) => v,
        None => return Ok(()),
    };

    match target {
        ClickTarget::MenuList => {
            if let Some(i) = row_to_list_index(area, app.menu_state.offset(), row) {
                if i < MENU_ITEMS {
                    app.menu_state.select(Some(i));
                    app.menu_select()?;
                }
            }
        }

        ClickTarget::PickSimple => {
            if let Some(s) = app.add_card.as_mut() {
                s.kind = AddKind::Simple;
                s.phase = AddPhase::FillForm;
            }
        }
        ClickTarget::PickMulti => {
            if let Some(s) = app.add_card.as_mut() {
                s.kind = AddKind::Multi;
                s.phase = AddPhase::FillForm;
            }
        }

        ClickTarget::AddCardField(idx) => {
            if let Some(s) = app.add_card.as_mut() {
                s.focused = idx;
                if idx != 0 { s.deck_list_idx = None; }
            }
        }
        ClickTarget::AddCardToggle(idx) => {
            if let Some(s) = app.add_card.as_mut() { s.focused = idx; }
            on_add_card(app, KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))?;
        }
        ClickTarget::AddCardButton(idx) => {
            if let Some(s) = app.add_card.as_mut() { s.focused = idx; }
            on_add_card(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
        }
        ClickTarget::StepsList => {
            let mut hit = false;
            if let Some(s) = app.add_card.as_mut() {
                let offset = s.step_list_state.offset();
                if let Some(i) = row_to_list_index(area, offset, row) {
                    if i < s.steps.len() {
                        s.focused = 5;
                        s.step_list_state.select(Some(i));
                        hit = true;
                    }
                }
            }
            // clicking a step also opens it for editing, same as pressing enter
            if hit {
                on_add_card(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
            }
        }
        ClickTarget::DeckSuggestions => {
            let mut confirm = false;
            if let Some(s) = app.add_card.as_mut() {
                let n = s.filtered_decks().len();
                if let Some(i) = row_to_list_index(area, 0, row) {
                    if i < n {
                        s.focused = 0;
                        s.deck_list_idx = Some(i);
                        confirm = true;
                    }
                }
            }
            if confirm {
                on_add_card(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
            }
        }

        ClickTarget::ReviewCard => {
            on_review(app, KeyCode::Char(' '))?;
        }
        ClickTarget::ReviewRate(n) => {
            on_review(app, KeyCode::Char((b'0' + n) as char))?;
        }
        ClickTarget::LeechRename => {
            on_review(app, KeyCode::Char('1'))?;
        }
        ClickTarget::LeechSplit => {
            on_review(app, KeyCode::Char('2'))?;
        }
        ClickTarget::LeechScaffold => {
            on_review(app, KeyCode::Char('3'))?;
        }
        ClickTarget::LeechContinue => {
            on_review(app, KeyCode::Char('c'))?;
        }

        ClickTarget::DecksList => {
            if let Some(ld) = app.list_decks.as_mut() {
                if let Some(i) = row_to_list_index(area, ld.list_state.offset(), row) {
                    if i < ld.filtered_decks().len() { ld.list_state.select(Some(i)); }
                }
            }
        }
        ClickTarget::CardsList => {
            if let Some(lc) = app.list_cards.as_mut() {
                if let Some(i) = row_to_list_index(area, lc.list_state.offset(), row) {
                    if i < lc.filtered_cards().len() { lc.list_state.select(Some(i)); }
                }
            }
        }
        ClickTarget::TagPickerList => {
            if let Some(lc) = app.list_cards.as_mut() {
                if let Some(i) = row_to_list_index(area, lc.tag_picker_state.offset(), row) {
                    if let Some(tag) = lc.tag_picker_tags.get(i).cloned() {
                        lc.tag_picker_state.select(Some(i));
                        if lc.tag_filter.contains(&tag) {
                            lc.tag_filter.retain(|t| t != &tag);
                        } else {
                            lc.tag_filter.push(tag);
                        }
                    }
                }
            }
        }

        ClickTarget::ExportDeckList => {
            if let Some(ex) = app.export.as_mut() {
                if let Some(i) = row_to_list_index(area, ex.list_state.offset(), row) {
                    if i < ex.list_len() {
                        ex.focus = ExportFocus::DeckList;
                        ex.list_state.select(Some(i));
                        ex.refresh_default_path();
                    }
                }
            }
        }
        ClickTarget::ExportToggle => {
            if let Some(ex) = app.export.as_mut() {
                ex.focus = ExportFocus::ResetToggle;
                ex.reset_metadata = !ex.reset_metadata;
            }
        }
        ClickTarget::ExportPathField => {
            if let Some(ex) = app.export.as_mut() { ex.focus = ExportFocus::PathField; }
        }
        ClickTarget::ExportConfirmBtn => {
            if let Some(ex) = app.export.as_mut() { ex.focus = ExportFocus::ConfirmBtn; }
            on_export(app, KeyCode::Enter)?;
        }

        ClickTarget::ImportPathField => {
            if let Some(im) = app.import.as_mut() { im.focus = ImportFocus::PathField; }
        }
        ClickTarget::ImportConfirmBtn => {
            if let Some(im) = app.import.as_mut() { im.focus = ImportFocus::ConfirmBtn; }
            on_import(app, KeyCode::Enter)?;
        }
    }
    Ok(())
}

// ~~~ weak-step handling: leech intervention ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// step editor No new editor logic both reuse `AddCardState` exactly
/// as the normal "edit step" flow does.
fn launch_leech_editor(app: &mut AppState, split: bool) -> anyhow::Result<()> {
    let (item_id, card_id) = match app.review.as_ref().and_then(|r| r.leech_prompt.as_ref()) {
        Some(p) => (p.item_id.clone(), p.card_id.clone()),
        None => return Ok(()),
    };

    let card  = app.store.load_card(&card_id)?;
    let items = app.store.load_items(&card_id)?;
    let tags  = app.store.get_card_tags(&card_id)?;
    let decks = app.store.list_decks().unwrap_or_default();
    let mut edit_state = AddCardState::for_edit(&card, &items, decks, tags);
    edit_state.editing_from_review = true;

    if card.kind == CardKind::Multi {
        if let Some(idx) = items
            .iter()
            .filter(|i| i.kind == ItemKind::Step)
            .position(|i| i.id == item_id)
        {
            edit_state.step_list_state.select(Some(idx));
            if split {
                edit_state.insert_step_after_selected();
            } else if let Some((name, answer, img)) = edit_state.steps.get(idx).cloned() {
                edit_state.step_name_buf    = name;
                edit_state.step_buf         = answer;
                edit_state.step_image_path  = img;
                edit_state.editing_step_idx = Some(idx);
                edit_state.focused          = 2; // Step Name field
            }
        }
    } else {
        // simple card: "rename" means editing the question only, no split feature
        edit_state.focused = 1;
    }

    app.add_card = Some(edit_state);
    app.go_to(Screen::AddCard);
    Ok(())
}

// ~~~ Save helper ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

fn save_new_card(app: &mut AppState) -> anyhow::Result<()> {
    let store = app.store;

    let s = app.add_card.as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing add-card state"))?;
    s.validate().map_err(|e| anyhow::anyhow!(e))?;

    let tags = s.parsed_tags();

    if let Some(ref card_id) = s.editing_card_id.clone() {
        // ~~ edit path ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        match s.kind {
            AddKind::Simple => store.update_simple_card(
                card_id,
                s.deck.trim(),
                s.simple_question.trim(),
                s.answer.trim(),
                s.reversible,
                s.answer_image_path.as_deref(),
                &s.review_mode,
            )?,
            AddKind::Multi => store.update_multi_card(
                card_id,
                s.deck.trim(),
                s.multi_question.trim(),
                &s.steps,
                s.show_chain,
                &s.review_mode,
            )?,
        }
        store.set_card_tags(card_id, &tags)?;
    } else {
        // ~~ create path ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        let card_id = match s.kind {
            AddKind::Simple => store.add_simple_card(
                s.deck.trim(),
                s.simple_question.trim(),
                s.answer.trim(),
                s.reversible,
                s.answer_image_path.as_deref(),
                &s.review_mode,
            )?,
            AddKind::Multi => store.add_multi_card(
                s.deck.trim(),
                s.multi_question.trim(),
                &s.steps,
                s.show_chain,
                &s.review_mode,
            )?,
        };
        store.set_card_tags(&card_id, &tags)?;
    }
    Ok(())
}

// ~~~ Editor helper ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

// suspend the TUI and open content in the default $EDITOR, then return the result
fn open_in_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    content: &str,
) -> anyhow::Result<String> {
    // give the terminal back to the OS so the editor gets a clean screen
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture);
    let result = edit::edit(content).unwrap_or_else(|_| content.to_string());
    // reclaim the terminal for ratatui
    let _ = execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture);
    let _ = enable_raw_mode();
    terminal.clear()?; // force a full redraw
    // trim the trailing newline that most editors append on save, since the
    // user's original content wont have had one originally
    let trimmed = result.trim_end_matches('\n').to_string();
    Ok(if trimmed.is_empty() { content.to_string() } else { trimmed })
}
