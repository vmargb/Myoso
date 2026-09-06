// ~~~ ui/render.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// All screen renderers and shared layout/style helpers
// Called from ui/mod.rs event loop, markdown.rs calls are in this file

use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::state::{
    AddCardState, AddKind, AddPhase, AppState, ClickTarget, ExportFocus, ImportFocus,
    LeechPrompt, ListCardsState, MENU_ITEMS, ReviewFocus, ReviewPhase, WeakSpanPrompt,
};
use crate::models::{CardKind, Item, ItemKind, ReviewMode};

// ~~~ click-region helpers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// every renderer collects clickable Rects into a `clicks` vector as it draws
// the event loop and copies that vector into `app.click_regions` when frame is done
type Clicks = Vec<(Rect, ClickTarget)>;

fn register_span_clicks(clicks: &mut Clicks, area: Rect, spans: &[Span], targets: &[(&str, ClickTarget)]) {
    if area.height == 0 || area.width < 2 { return; }
    let inner_x = area.x + 1;
    let inner_w = area.width.saturating_sub(2);
    let total_w: u16 = spans.iter().map(|s| s.content.chars().count() as u16).sum();
    let mut x = inner_x + inner_w.saturating_sub(total_w.min(inner_w)) / 2;
    let y = area.y + 1;
    for i in 0..spans.len() {
        let w = spans[i].content.chars().count() as u16;
        let trimmed = spans[i].content.trim();
        let mut matched: Option<ClickTarget> = None;
        for &(needle, target) in targets {
            if needle == trimmed { matched = Some(target); break; }
        }
        if let Some(target) = matched {
            let mut click_w = w;
            if i + 1 < spans.len() {
                click_w += spans[i + 1].content.chars().count() as u16;
            }
            clicks.push((Rect { x, y, width: click_w.max(1), height: 1 }, target));
        }
        x += w;
    }
}

/// convert a mouse row into an index into a possibly scrolled List
pub(super) fn row_to_list_index(area: Rect, offset: usize, row: u16) -> Option<usize> {
    let top    = area.y + 1;
    let bottom = area.y + area.height.saturating_sub(1);
    if row < top || row >= bottom { return None; }
    Some(offset + (row - top) as usize)
}
// ~~~ layout / style helpers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub(super) fn centered_rect(pct_x: u16, height: u16, r: Rect) -> Rect {
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

/// bordered block whose border thickens and turns cyan when focused.
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

/// small trailing footer hint for the Feynman pad
fn feynman_footer_hint(scratchpad_open: bool) -> Vec<Span<'static>> {
    if scratchpad_open {
        vec![
            Span::raw("   "),
            Span::styled(" [Tab] ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::raw("Pad/card focus"),
        ]
    } else {
        vec![
            Span::raw("   "),
            Span::styled(" [f] ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::raw("Feynman pad"),
        ]
    }
}

/// readable countdown: "4d 2h", "3h 15m", "45m", "now"
fn format_time_until(due: DateTime<Utc>) -> String {
    let secs = (due - Utc::now()).num_seconds();
    if secs <= 0 {
        return "now".to_string();
    }
    let days  = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins  = (secs % 3_600) / 60;
    match (days, hours, mins) {
        (d, h, _) if d >= 1 && h > 0 => format!("{d}d {h}h"),
        (d, _, _) if d >= 1           => format!("{d}d"),
        (_, h, m) if h >= 1 && m > 0 => format!("{h}h {m}m"),
        (_, h, _) if h >= 1           => format!("{h}h"),
        _                             => format!("{}m", mins.max(1)),
    }
}

// ~~~ Renderers ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub(super) fn render_menu(f: &mut Frame, app: &mut AppState, clicks: &mut Clicks) {
    let size = f.area();
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Length(8),
            Constraint::Length(MENU_ITEMS as u16 + 2),
            Constraint::Min(1),
        ])
        .split(size);

    // horizontal centering helper (20 % column)
    let centre = |area: Rect| {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(50),
                Constraint::Percentage(25),
            ])
            .split(area)[1]
    };

    let title_art = vec![
        " __   __ _  _  _  _________ ___  ",
        "│  ╲ ╱  │ ││ ││ │╱ _ ╲  ___) _ ╲ ",
        "│   v   │ ╲│ │╱ │ │ │ ╲ ╲ │ │ │ │",
        "│ │╲_╱│ │╲_   _╱│ │ │ │> >│ │ │ │",
        "│ │   │ │  │ │  │ │_│ ╱ ╱_│ │_│ │",
        "│_│   │_│  │_│   ╲___╱_____)___╱ ",
    ];

    let mut title_lines: Vec<Line> = title_art
        .into_iter()
        .map(|line| {
            Line::from(Span::styled(
                line,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ))
        })
        .collect();

    title_lines.push(Line::from("")); // empty line for spacing
    title_lines.push(Line::from(Span::styled(
        "Step-by-step flashcards for the terminal",
        Style::default().fg(Color::DarkGray),
    )));

    f.render_widget(
        Paragraph::new(title_lines).alignment(Alignment::Center),
        v[1],
    );

    // Menu list
    let items = [
        " >  Start Review Session",
        " +  Add Card",
        " *  Browse Decks",
        " =  All Cards",
        " @  Statistics",
        " ~  Export",
        " ^  Import",
        " x  Quit",
    ]
    .iter()
    .map(|s| ListItem::new(*s))
    .collect::<Vec<_>>();

    let menu_area = centre(v[2]);
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .title(" Menu ")
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            // .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .border_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD), 
                    ),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(" ▶ "),
        menu_area,
        &mut app.menu_state,
    );
    clicks.push((menu_area, ClickTarget::MenuList));

    if let Some(ref msg) = app.flash {
        f.render_widget(
            Paragraph::new(Span::styled(msg.as_str(), Style::default().fg(Color::Green)))
                .alignment(Alignment::Center),
            centre(v[3]),
        );
    }
}

// ~~ Stats ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub(super) fn render_stats(f: &mut Frame, app: &AppState) {
    let size = f.area();
    let area = centered_rect(50, 14, size);
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
                Span::raw("  SR due now  : "),
                Span::styled(st.due_items.to_string(),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("  Reviews log : "),
                Span::styled(st.review_logs.to_string(),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  Daily cards : "),
                Span::styled(
                    st.daily_cards.to_string(),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("  Daily due   : "),
                Span::styled(
                    format!("{}/{}", st.daily_due, st.daily_cards),
                    if st.daily_due > 0 {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
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

/// Weak-step handling Phase 2: pick which phrases get cloze-blanked (and
/// later highlighted on reveal) for a scaffolded item. Prefers the user's
/// own weight-sorted weak spans 
fn select_cloze_phrases(item: &Item) -> Vec<String> {
    let mut spans: Vec<&crate::models::WeakSpan> = item
        .weak_spans
        .iter()
        .filter(|s| item.answer.contains(s.phrase.as_str()))
        .collect();
    spans.sort_by(|a, b| b.weight.cmp(&a.weight));

    let mut phrases: Vec<String> = spans.into_iter().take(3).map(|s| s.phrase.clone()).collect();

    phrases.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
    phrases
}

/// Pre-reveal: the answer with each selected phrase replaced by a blank of
/// matching visual width.
fn blank_text(answer: &str, phrases: &[String]) -> String {
    let mut out = answer.to_string();
    for p in phrases {
        if p.is_empty() {
            continue;
        }
        let blank = "▁".repeat(p.chars().count().max(3));
        out = out.replace(p.as_str(), &blank);
    }
    out
}

/// Post-reveal: the full, un-blanked answer with the tested phrases
/// highlighted, so the user can directly check their recall against what
/// was actually asked, the whole point of a reveal step.
fn highlight_lines(answer: &str, phrases: &[String]) -> Vec<Line<'static>> {
    answer
        .lines()
        .map(|line| {
            let mut segments: Vec<(String, bool)> = vec![(line.to_string(), false)];
            for p in phrases {
                if p.is_empty() {
                    continue;
                }
                let mut next = Vec::new();
                for (seg, hl) in segments {
                    if hl {
                        next.push((seg, hl));
                        continue;
                    }
                    let mut rest = seg.as_str();
                    while let Some(idx) = rest.find(p.as_str()) {
                        if idx > 0 {
                            next.push((rest[..idx].to_string(), false));
                        }
                        next.push((p.clone(), true));
                        rest = &rest[idx + p.len()..];
                    }
                    if !rest.is_empty() {
                        next.push((rest.to_string(), false));
                    }
                }
                segments = next;
            }
            let spans: Vec<Span<'static>> = segments
                .into_iter()
                .map(|(text, hl)| {
                    if hl {
                        Span::styled(
                            text,
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        Span::styled(text, Style::default().fg(Color::White))
                    }
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}



pub(super) fn render_review(f: &mut Frame, app: &AppState, clicks: &mut Clicks) {
    let size = f.area();
    let rs = match app.review.as_ref() { Some(r) => r, None => return };

    if rs.is_done() {
        if rs.total_items == 0 {
            render_nothing_due(f, app, size);
        } else {
            render_review_done(f, rs, size);
        }
        return;
    }

    let rc   = &rs.session[rs.card_idx];
    let item = &rc.items[rs.item_idx];
    let is_scaffolded = item.scaffold_state.is_scaffolded();

    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(size);

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

    let daily_prefix = if rc.card.review_mode.is_daily() { "★ DAILY  " } else { "" };
    let prompt_label = if rc.card.kind == CardKind::Multi {
        format!(
            " {}{} | multi | step {} of {} ",
            daily_prefix,
            rc.card.deck,
            rs.item_idx + 1,
            rc.items.len(),
        )
    } else {
        match item.kind {
            ItemKind::Reverse => format!(" {}{} | {} | A->Q ", daily_prefix, rc.card.deck, rc.card.kind),
            _                 => format!(" {}{} | {} ", daily_prefix, rc.card.deck, rc.card.kind),
        }
    };

    let prompt_title_style = if rc.card.review_mode.is_daily() {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // show the overarching question for Multi cards instead of "Step X"
    let display_prompt = if rc.card.kind == CardKind::Multi {
        rc.card.question.as_str()
    } else {
        item.prompt.as_str()
    };

    // context lines are shared by both phases, multi-card breadcrumb/chain or a plain kind label
    let mut context_lines: Vec<Line> = Vec::new();

    if rc.card.kind == CardKind::Multi {
        let total_in_session = rc.items.len();
        let is_target = rs.item_idx == total_in_session - 1;

        // ~~ Chain breadcrumb ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        let mut crumb_spans: Vec<Span> = vec![Span::raw("  ")];
        for i in 0..total_in_session {
            if i < rs.item_idx {
                crumb_spans.push(Span::styled("✓ ", Style::default().fg(Color::DarkGray)));
            } else if i == rs.item_idx {
                let col = if is_target { Color::Cyan } else { Color::Yellow };
                crumb_spans.push(Span::styled("▶ ", Style::default().fg(col).add_modifier(Modifier::BOLD)));
            } else {
                crumb_spans.push(Span::styled("○ ", Style::default().fg(Color::DarkGray)));
            }
        }
        if !is_target {
            crumb_spans.push(Span::styled(
                format!(" target is step {total_in_session}"),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ));
        }
        context_lines.push(Line::from(crumb_spans));
        context_lines.push(Line::from(""));

        // ~~ Preceding steps rebuild context ~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        if rc.card.show_chain {
            for (i, prev) in rc.items[..rs.item_idx].iter().enumerate() {
                let step_label = format!("  {} ✓", prev.prompt);
                // if the prompt is just the auto "Step N" we already have the number, otherwise show it
                let display_label = if prev.prompt == format!("Step {}", i + 1) {
                    step_label
                } else {
                    format!("  {}. {} ✓", i + 1, prev.prompt)
                };
                context_lines.push(Line::from(Span::styled(
                    display_label,
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
                )));
                for l in prev.answer.lines() {
                    context_lines.push(Line::from(Span::styled(
                        format!("    {l}"),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                context_lines.push(Line::from(""));
            }
        }

        // ~~ Current step label ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        let cur_step_label = if item.prompt == format!("Step {}", rs.item_idx + 1) {
            format!("  Step {}  ", rs.item_idx + 1)
        } else {
            format!("  {}. {}  ", rs.item_idx + 1, item.prompt)
        };
        if is_target {
            context_lines.push(Line::from(vec![
                Span::styled(
                    cur_step_label,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "← current target  (pass to unlock the next step)",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                ),
            ]));
        } else {
            context_lines.push(Line::from(vec![
                Span::styled(
                    cur_step_label,
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    } else {
        let hint = match item.kind {
            ItemKind::Forward => "  Answer",
            ItemKind::Reverse => "  Question  (reverse card)",
            ItemKind::Step    => "  Step",
        };
        context_lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // ONE stable card for BOTH phases, same position/size always, so revealing
    // never repositions or resizes anything, it only grows the content inside
    // when the Feynman scratchpad is open, the review area is split side-by-side
    let (card, pad_area) = if rs.scratchpad_open {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(v[1]);
        let card = centered_rect(92, cols[0].height.saturating_sub(2).max(10), cols[0]);
        (card, Some(cols[1]))
    } else {
        let card = centered_rect(80, v[1].height.saturating_sub(2).max(10), v[1]);
        (card, None)
    };
    if rs.phase == ReviewPhase::Thinking {
        clicks.push((card, ClickTarget::ReviewCard));
    }

    let mut lines: Vec<Line> = vec![Line::from(""), Line::from("")];
    for l in display_prompt.lines() {
        lines.push(Line::from(Span::styled(
            l.to_string(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(""));
    lines.extend(context_lines);

    let border_color = if is_scaffolded {
        Color::Magenta
    } else {
        match rs.phase {
            ReviewPhase::Thinking => Color::DarkGray,
            ReviewPhase::Revealed => Color::Cyan,
        }
    };

    let ans_title = match rs.phase {
        ReviewPhase::Thinking => String::new(),
        ReviewPhase::Revealed if item.image_path.is_some() => " Answer • image attached ".to_string(),
        ReviewPhase::Revealed => " Answer ".to_string(),
    };

    if is_scaffolded {
        // no reveal gate, the (cloze-blanked) answer is always on screen
        lines.push(Line::from(""));
        let divider_width = (card.width as usize).saturating_sub(4).clamp(10, 60);
        lines.push(Line::from(Span::styled(
            format!("  {}", "─".repeat(divider_width)),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(vec![
            Span::styled(
                " Scaffolded ",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" pass {}/{} today to graduate", item.scaffold_passes.max(0), crate::scheduler::LEECH_STREAK_THRESHOLD),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
        ]));
        lines.push(Line::from(""));

        let phrases = select_cloze_phrases(item);
        match rs.phase {
            ReviewPhase::Thinking => {
                // Cloze rendering is plain text (not markdown-rendered)
                for l in blank_text(&item.answer, &phrases).lines() {
                    lines.push(Line::from(Span::styled(l.to_string(), Style::default().fg(Color::White))));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  [ Space / Enter to reveal ]",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                )));
            }
            ReviewPhase::Revealed => {
                // Full answer, with the tested phrase(s) highlighted so the
                // user can directly confirm their recall was right.
                lines.extend(highlight_lines(&item.answer, &phrases));
            }
        }
    } else {
        match rs.phase {
            ReviewPhase::Thinking => {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  [ Space / Enter to reveal ]",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                )));
            }
            ReviewPhase::Revealed => {
                lines.push(Line::from(""));
                let divider_width = (card.width as usize).saturating_sub(4).clamp(10, 60);
                lines.push(Line::from(Span::styled(
                    format!("  {}", "─".repeat(divider_width)),
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(Span::styled(
                    ans_title.trim().to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));

                if item.image_path.is_some() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  Image attached  ",
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "press ",
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            "[o]",
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            " to open",
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                    lines.push(Line::from(""));
                }

                let rendered = crate::markdown::render(&item.answer);
                lines.extend(rendered.lines);
            }
        }
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color))
                    .title(prompt_label)
                    .title_style(prompt_title_style),
            )
            .wrap(Wrap { trim: false })
            .scroll((rs.answer_scroll, 0)),
        card,
    );

    if let Some(pad_rect) = pad_area {
        let pad_focused = rs.focus == ReviewFocus::Scratchpad;
        let border_color = if pad_focused { Color::Cyan } else { Color::DarkGray };
        let title = if pad_focused {
            " Feynman pad | explain it in your own words "
        } else {
            " Feynman pad "
        };
        let text = with_cursor(&rs.scratchpad, pad_focused);
        f.render_widget(
            Paragraph::new(text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(border_color))
                        .title(title),
                )
                .style(Style::default().fg(if pad_focused { Color::White } else { Color::DarkGray }))
                .wrap(Wrap { trim: false }),
            pad_rect,
        );
    }

    let footer = if is_scaffolded && rs.phase == ReviewPhase::Thinking {
        Line::from(vec![
            Span::styled(
                " [Space] ",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Reveal  "),
            Span::styled(
                " (blanks stay hidden until you reveal)  ",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
            Span::styled(" [q] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Quit"),
        ])
    } else if is_scaffolded {
        // Weak-step handling Phase 2: grading is pass/fail only while
        // scaffolded, [1] Fail and [3] Good/Pass reuse the existing
        // Again/Good confidence semantics end-to-end (chain de-unlock,
        // review_log, rolling averages), just with [2]/[4] not offered.
        let spans = vec![
            Span::styled(" [1] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("Fail  "),
            Span::styled(" [3] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Pass  "),
            Span::styled(
                " (2/4 unavailable while scaffolded)  ",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
            Span::styled(" [e] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Edit"),
        ];
        register_span_clicks(clicks, v[2], &spans, &[
            ("[1]", ClickTarget::ReviewRate(1)),
            ("[3]", ClickTarget::ReviewRate(3)),
        ]);
        Line::from(spans)
    } else if rs.phase == ReviewPhase::Thinking {
        Line::from(vec![
            Span::styled(
                " [Space] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Reveal   "),
            Span::styled(" [e] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Edit   "),
            Span::styled(" [q] ", Style::default().fg(Color::DarkGray)),
            Span::raw("Quit session"),
            Span::styled("   [↑ /↓ ] ", Style::default().fg(Color::DarkGray)),
            Span::raw("Scroll"),
        ])
    } else if item.kind == ItemKind::Step {
        let spans = vec![
            Span::styled(" [1] ", Style::default().fg(Color::Red)),
            Span::raw("Again  "),
            Span::styled("← fail   ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
            Span::styled(" [2] ", Style::default().fg(Color::Yellow)),
            Span::raw("Hard  "),
            Span::styled(" [3] ", Style::default().fg(Color::Green)),
            Span::raw("Good  "),
            Span::styled(" [4] ", Style::default().fg(Color::Blue)),
            Span::raw("Easy  "),
            Span::styled(" [e] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Edit"),
        ];
        register_span_clicks(clicks, v[2], &spans, &[
            ("[1]", ClickTarget::ReviewRate(1)),
            ("[2]", ClickTarget::ReviewRate(2)),
            ("[3]", ClickTarget::ReviewRate(3)),
            ("[4]", ClickTarget::ReviewRate(4)),
        ]);
        Line::from(spans)
    } else {
        let spans = vec![
            Span::styled(" [1] ", Style::default().fg(Color::Red)),
            Span::raw("Again  "),
            Span::styled(" [2] ", Style::default().fg(Color::Yellow)),
            Span::raw("Hard  "),
            Span::styled(" [3] ", Style::default().fg(Color::Green)),
            Span::raw("Good  "),
            Span::styled(" [4] ", Style::default().fg(Color::Blue)),
            Span::raw("Easy  "),
            Span::styled(" [e] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Edit"),
        ];
        register_span_clicks(clicks, v[2], &spans, &[
            ("[1]", ClickTarget::ReviewRate(1)),
            ("[2]", ClickTarget::ReviewRate(2)),
            ("[3]", ClickTarget::ReviewRate(3)),
            ("[4]", ClickTarget::ReviewRate(4)),
        ]);
        Line::from(spans)
    };
    let mut footer = footer;
    footer.spans.extend(feynman_footer_hint(rs.scratchpad_open));
    f.render_widget(
        Paragraph::new(footer)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded))
            .alignment(Alignment::Center),
        v[2],
    );

    if let Some(prompt) = rs.leech_prompt.as_ref() {
        render_leech_prompt(f, prompt, size, clicks);
    }
    if let Some(prompt) = rs.weak_span_prompt.as_ref() {
        render_weak_span_prompt(f, prompt, size);
    }
}

/// Blocking rename/split intervention prompt
fn render_leech_prompt(f: &mut Frame, prompt: &LeechPrompt, size: Rect, clicks: &mut Clicks) {
    let height = 9;
    let area = centered_rect(60, height, size);
    f.render_widget(Clear, area);

    let title: String = prompt.prompt_text.chars().take(40).collect();
    let ellipsis = if prompt.prompt_text.chars().count() > 40 { "…" } else { "" };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " This step keeps giving you trouble ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " what do you want to do?",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(" \"{title}{ellipsis}\""),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [1] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Mark blind spot  "),
            Span::styled("(tag the exact words that tripped you up)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" [c] ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            Span::raw("Ignore"),
        ]),
    ];

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(" Leech detected "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );

    // click regions, one row each starting after the 5-line header
    let base_y = area.y + 6;
    clicks.push((Rect { x: area.x, y: base_y, width: area.width, height: 1 }, ClickTarget::LeechMarkBlindSpot));
    clicks.push((Rect { x: area.x, y: base_y + 1, width: area.width, height: 1 }, ClickTarget::LeechIgnore));
}

/// Weak-span marking prompt. Entirely skippable
/// fires after any confidence <= 2 rating so the user can tag
/// the exact words they blanked on
fn render_weak_span_prompt(f: &mut Frame, prompt: &WeakSpanPrompt, size: Rect) {
    let height = (prompt.words.len() as u16 + 8).min(size.height.saturating_sub(2)).max(10);
    let area = centered_rect(64, height, size);
    f.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Mark the word(s) you blanked on (optional)",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " [space] toggle   [enter] mark phrase   [esc] done",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    // wrap the word list across lines of around 8 words so long answers stay readable
    let mut cur = vec![Span::raw(" ")];
    for (i, w) in prompt.words.iter().enumerate() {
        let selected = prompt.selected.contains(&i);
        let is_cursor = i == prompt.cursor;
        let style = match (selected, is_cursor) {
            (true, true)  => Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
            (true, false) => Style::default().fg(Color::Black).bg(Color::Cyan),
            (false, true) => Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
            (false, false)=> Style::default().fg(Color::White),
        };
        cur.push(Span::styled(format!("{w} "), style));
        if (i + 1) % 8 == 0 {
            lines.push(Line::from(std::mem::replace(&mut cur, vec![Span::raw(" ")])));
        }
    }
    if cur.len() > 1 {
        lines.push(Line::from(cur));
    }
    lines.push(Line::from(""));
    if prompt.committed_any {
        lines.push(Line::from(Span::styled(
            " ✓ marked, select more, or press esc when done",
            Style::default().fg(Color::Green),
        )));
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(" Mark blind spot "),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_nothing_due(f: &mut Frame, app: &AppState, size: Rect) {
    let next  = app.store.next_due().ok().flatten();
    let h     = if next.is_some() { 11 } else { 7 };
    let area  = centered_rect(64, h, size);

    let mut lines = vec![
        Line::from(Span::styled(
            " Nothing due right now!",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::raw(" All caught up. Come back later.")),
        Line::from(""),
    ];

    if let Some((due_at, question, deck)) = next {
        let q: String = question.chars().take(42).collect();
        let ellipsis  = if question.chars().count() > 42 { "…" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(" Next   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("\"{q}{ellipsis}\""),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("  [{}]", deck),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Due in ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format_time_until(due_at),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        " [q] / [Esc] to return",
        Style::default().fg(Color::DarkGray),
    )));

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title(" Review "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_review_done(f: &mut Frame, rs: &super::state::ReviewState, size: Rect) {
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

pub(super) fn render_add_card(f: &mut Frame, app: &mut AppState, clicks: &mut Clicks) {
    let size        = f.area();
    let phase       = app.add_card.as_ref().unwrap().phase;
    let kind        = app.add_card.as_ref().unwrap().kind;
    let is_editing  = app.add_card.as_ref().unwrap().editing_card_id.is_some();

    match phase {
        // never show PickType when editing, the kind is already fixed
        AddPhase::PickType if !is_editing => render_pick_type(f, app.add_card.as_ref().unwrap(), size, clicks),
        _ => match kind {
            AddKind::Simple => render_simple_form(f, app.add_card.as_ref().unwrap(), size, clicks),
            AddKind::Multi  => render_multi_form(f, app, size, clicks),
        },
    }
}

fn render_pick_type(f: &mut Frame, s: &AddCardState, size: Rect, clicks: &mut Clicks) {
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
        (1usize, "[1] Simple",     "  Q →  A  (reversible)", AddKind::Simple),
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
        clicks.push((v[slot], match kind {
            AddKind::Simple => ClickTarget::PickSimple,
            AddKind::Multi  => ClickTarget::PickMulti,
        }));
    }

    f.render_widget(
        Paragraph::new(Span::styled(
            "[1/2] or [<-/->] to pick  •  [Enter] confirm  •  [Esc] cancel",
            Style::default().fg(Color::DarkGray),
        ))
        .alignment(Alignment::Center),
        v[3],
    );
}

fn render_simple_form(f: &mut Frame, s: &AddCardState, size: Rect, clicks: &mut Clicks) {
    let filtered = s.filtered_decks();
    let sugg_h: u16 = if s.focused == 0 && !filtered.is_empty() {
        (filtered.len() as u16 + 2).min(6)
    } else { 0 };

    // heading + deck + sugg + question + answer + reversible + daily + tags + save + hint
    // extra 3 for the tags row
    let total_h: u16 = 1 + 3 + sugg_h + 5 + 5 + 3 + 3 + 3 + 3 + 3 + 1;
    let area = centered_rect(72, total_h, size);
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),       // heading
            Constraint::Length(3),       // deck input
            Constraint::Length(sugg_h),  // deck suggestions
            Constraint::Length(5),       // question
            Constraint::Length(5),       // answer
            Constraint::Length(3),       // reversible
            Constraint::Length(3),       // daily toggle
            Constraint::Length(3),       // tags
            Constraint::Length(3),       // save button
            Constraint::Min(1),          // error / help
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            if s.editing_card_id.is_some() { "Edit Simple Card" } else { "Add Simple Card" },
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
    clicks.push((v[1], ClickTarget::AddCardField(0)));

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
                    .title(" Existing decks  [↓ /↑ ] select  [Enter] confirm "),
            ),
            v[2],
        );
        clicks.push((v[2], ClickTarget::DeckSuggestions));
    }

    let q_text    = with_cursor(&s.simple_question, s.focused == 1);
    let q_lines   = q_text.lines().count() as u16;
    let q_visible = v[3].height.saturating_sub(2);
    let q_scroll  = q_lines.saturating_sub(q_visible);
    f.render_widget(
        Paragraph::new(q_text)
            .block(field_block(if s.focused == 1 { " Question  │  [^E] editor " } else { " Question " }, s.focused == 1))
            .wrap(Wrap { trim: false })
            .scroll((q_scroll, 0))
            .style(text_style(s.focused == 1)),
        v[3],
    );
    clicks.push((v[3], ClickTarget::AddCardField(1)));

    let answer_title = match (s.focused == 2, s.answer_image_path.is_some()) {
        (true,  true)  => " Answer  │  [Ctrl+e] editor  │  [Ctrl+o] change image  [IMG ✓ ] ",
        (true,  false) => " Answer  │  [Ctrl+e] editor  │  [Ctrl+o] open image ",
        (false, true)  => " Answer  [IMG ✓ ] ",
        (false, false) => " Answer ",
    };
    let a_text    = with_cursor(&s.answer, s.focused == 2);
    let a_lines   = a_text.lines().count() as u16;
    let a_visible = v[4].height.saturating_sub(2);
    let a_scroll  = a_lines.saturating_sub(a_visible);
    f.render_widget(
        Paragraph::new(a_text)
            .block(field_block(answer_title, s.focused == 2))
            .wrap(Wrap { trim: false })
            .scroll((a_scroll, 0))
            .style(text_style(s.focused == 2)),
        v[4],
    );
    clicks.push((v[4], ClickTarget::AddCardField(2)));

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
    clicks.push((v[5], ClickTarget::AddCardToggle(3)));

    render_daily_toggle(f, &s.review_mode, s.focused == 4, v[6]);
    clicks.push((v[6], ClickTarget::AddCardToggle(4)));

    // tags field (focused == 5)
    f.render_widget(
        Paragraph::new(with_cursor(&s.tags_buf, s.focused == 5))
            .block(field_block(" Tags  (space or comma separated, e.g. exam-prep year1) ", s.focused == 5))
            .style(text_style(s.focused == 5)),
        v[7],
    );
    clicks.push((v[7], ClickTarget::AddCardField(5)));

    render_save_btn(f, s.focused == 6, v[8]);
    clicks.push((v[8], ClickTarget::AddCardButton(6)));
    render_form_hint(f, s.error.as_deref(), v[9]);
}

fn render_multi_form(f: &mut Frame, app: &mut AppState, size: Rect, clicks: &mut Clicks) {
    let s = app.add_card.as_mut().unwrap();
    let filtered = s.filtered_decks();
    let sugg_h: u16 = if s.focused == 0 && !filtered.is_empty() {
        (filtered.len() as u16 + 2).min(6)
    } else { 0 };

    let steps_h = 6;
    //  heading + deck + sugg + question + step_name + step_ans + add_btn + steps_list
    //  + show_chain + daily + tags + save + hint
    let total_h: u16 = 1 + 3 + sugg_h + 5 + 3 + 5 + 3 + steps_h + 3 + 3 + 3 + 3 + 1;

    let area = centered_rect(72, total_h, size);
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),        // heading
            Constraint::Length(3),        // deck input
            Constraint::Length(sugg_h),   // deck suggestions
            Constraint::Length(5),        // question
            Constraint::Length(3),        // step name   (focused == 2)
            Constraint::Length(5),        // step answer (focused == 3)
            Constraint::Length(3),        // add step button (focused == 4)
            Constraint::Length(steps_h),  // steps list  (focused == 5)
            Constraint::Length(3),        // show_chain  (focused == 6)
            Constraint::Length(3),        // daily       (focused == 7)
            Constraint::Length(3),        // tags        (focused == 8)
            Constraint::Length(3),        // save        (focused == 9)
            Constraint::Min(1),           // error / help
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            if s.editing_card_id.is_some() { "Edit Multi-step Card" } else { "Add Multi-step Card" },
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
    clicks.push((v[1], ClickTarget::AddCardField(0)));

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
                    .title(" Existing decks  [↓ /↑ ] select  [Enter] confirm "),
            ),
            v[2],
        );
        clicks.push((v[2], ClickTarget::DeckSuggestions));
    }

    let q_text    = with_cursor(&s.multi_question, s.focused == 1);
    let q_lines   = q_text.lines().count() as u16;
    let q_visible = v[3].height.saturating_sub(2);
    let q_scroll  = q_lines.saturating_sub(q_visible);
    f.render_widget(
        Paragraph::new(q_text)
            .block(field_block(" Question  │  [^E] editor ", s.focused == 1))
            .wrap(Wrap { trim: false })
            .scroll((q_scroll, 0))
            .style(text_style(s.focused == 1)),
        v[3],
    );
    clicks.push((v[3], ClickTarget::AddCardField(1)));

    f.render_widget(
        Paragraph::new(with_cursor(&s.step_name_buf, s.focused == 2))
            .block(field_block(" Step Name  (optional, leave blank for 'Step N') ", s.focused == 2))
            .style(text_style(s.focused == 2)),
        v[4],
    );
    clicks.push((v[4], ClickTarget::AddCardField(2)));

    let answer_title = match (s.focused == 3, s.step_image_path.is_some()) {
        (true,  true)  => " Step Answer  [Enter] newline  │  [Ctrl+e] editor  │  [Ctrl+o] change image  [IMG ✓ ] ",
        (true,  false) => " Step Answer  [Enter] newline  │  [ctrl+e] editor  │  [Ctrl+o] open image ",
        (false, true)  => " Step Answer  [IMG ✓ ] ",
        (false, false) => " Step Answer ",
    };
    let step_text    = with_cursor(&s.step_buf, s.focused == 3);
    let step_lines   = step_text.lines().count() as u16;
    let step_visible = v[5].height.saturating_sub(2);
    let step_scroll  = step_lines.saturating_sub(step_visible);
    f.render_widget(
        Paragraph::new(step_text)
            .block(field_block(answer_title, s.focused == 3))
            .wrap(Wrap { trim: false })
            .scroll((step_scroll, 0))
            .style(text_style(s.focused == 3)),
        v[5],
    );
    clicks.push((v[5], ClickTarget::AddCardField(3)));

    render_add_step_btn(f, s.focused == 4, s.editing_step_idx.is_some(), v[6]);
    clicks.push((v[6], ClickTarget::AddCardButton(4)));

    let step_items: Vec<ListItem> = if s.steps.is_empty() {
        vec![ListItem::new(Span::styled(
            "  (no steps yet)",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        s.steps.iter().enumerate()
            .map(|(i, (name, answer, _))| {
                let prefix = if Some(i) == s.editing_step_idx { " ✎ " } else { "  " };
                let label = if name.trim().is_empty() {
                    format!("{prefix}{}. {}", i + 1, answer)
                } else {
                    format!("{prefix}{}. [{}]  {}", i + 1, name, answer)
                };
                ListItem::new(Line::from(Span::styled(
                    label,
                    if Some(i) == s.editing_step_idx {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Green)
                    },
                )))
            })
            .collect()
    };

    let list_title = if s.focused == 5 {
        format!(" Steps ({})  [Enter] edit  [i] insert after  [d] delete ", s.steps.len())
    } else {
        format!(" Steps ({}) ", s.steps.len())
    };
    let mut list = List::new(step_items).block(field_block(list_title.as_str(), s.focused == 5));
    if s.focused == 5 {
        list = list
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol(">> ");
    }
    f.render_stateful_widget(list, v[7], &mut s.step_list_state);
    clicks.push((v[7], ClickTarget::StepsList));

    let sc_text = if s.show_chain {
        "  y  Show preceding step answers during review"
    } else {
        "  n  Hide preceding step answers during review (harder, no context)"
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            sc_text,
            if s.focused == 6 {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ))
        .block(field_block(" Show chain  [Space to toggle] ", s.focused == 6)),
        v[8],
    );
    clicks.push((v[8], ClickTarget::AddCardToggle(6)));

    render_daily_toggle(f, &s.review_mode, s.focused == 7, v[9]);
    clicks.push((v[9], ClickTarget::AddCardToggle(7)));

    // tags field (focused == 8)
    f.render_widget(
        Paragraph::new(with_cursor(&s.tags_buf, s.focused == 8))
            .block(field_block(" Tags  (space or comma separated, e.g. exam-prep year1) ", s.focused == 8))
            .style(text_style(s.focused == 8)),
        v[10],
    );
    clicks.push((v[10], ClickTarget::AddCardField(8)));

    render_save_btn(f, s.focused == 9, v[11]);
    clicks.push((v[11], ClickTarget::AddCardButton(9)));
    render_form_hint(f, s.error.as_deref(), v[12]);
}

fn render_daily_toggle(f: &mut Frame, mode: &ReviewMode, focused: bool, area: Rect) {
    let (symbol, label, color) = if mode.is_daily() {
        ("  ★  ", "Daily: reviewed every day (no spaced repetition)", Color::Yellow)
    } else {
        ("  ☆  ", "Spaced Repetition: scheduled automatically", Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("{symbol}{label}"),
            if focused {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            },
        ))
        .block(field_block(" Review Mode  [Space to toggle] ", focused)),
        area,
    );
}

fn render_add_step_btn(f: &mut Frame, focused: bool, editing: bool, area: Rect) {
    let label = if editing { "  ►  Update step" } else { "  ►  Add step" };
    f.render_widget(
        Paragraph::new(Span::styled(
            label,
            if focused {
                Style::default()
                    .bg(Color::Cyan)
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
            "  [Tab] next  │  [Enter] on Add step / Save to confirm  │  [Esc] cancel",
            Style::default().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}

fn render_confirm_dialog(f: &mut Frame, msg: &str, size: Rect) {
    let area = centered_rect(54, 7, size);
    // wipe the area clean before drawing the block on top
    f.render_widget(Clear, area);
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

pub(super) fn render_list_cards(f: &mut Frame, app: &mut AppState, clicks: &mut Clicks) {
    let size = f.area();
    let lc = match app.list_cards.as_mut() { Some(lc) => lc, None => return };

    // ~~ layout ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(size);

    let now          = Utc::now();
    let total_count  = lc.cards.len();
    let filtered     = lc.filtered_cards();
    let filter_count = filtered.len();

    // ~~ card list ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let items: Vec<ListItem> = if filtered.is_empty() {
        vec![ListItem::new(Span::styled(
            if total_count == 0 {
                "  No cards yet,  use Add Card to create one."
            } else {
                "  No cards match the current filter."
            },
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        filtered.iter()
            .map(|c| {
                let daily_tag = if c.review_mode.is_daily() {
                    Span::styled("★ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else {
                    Span::raw("  ")
                };
                let due_tag = if c.due_at <= now && !c.review_mode.is_daily() {
                    Span::styled("DUE ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
                } else if c.review_mode.is_daily() {
                    Span::styled("DAY ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("    ", Style::default())
                };
                let kind_tag = Span::styled(
                    format!("[{:6}] ", c.kind.as_str()),
                    Style::default().fg(Color::Cyan),
                );
                let deck_tag = Span::styled(
                    format!("{:<20} ", c.deck),
                    Style::default().fg(Color::Yellow),
                );
                let q_tag = Span::styled(
                    c.question.chars().take(40).collect::<String>(),
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
                let time_tag = if c.due_at > now {
                    Span::styled(
                        format!("  in {}", format_time_until(c.due_at)),
                        Style::default().fg(Color::DarkGray),
                    )
                } else {
                    Span::raw("")
                };

                // tag chips
                let mut spans = vec![
                    Span::raw(" "),
                    daily_tag,
                    due_tag,
                    kind_tag,
                    deck_tag,
                    q_tag,
                    n_tag,
                    time_tag,
                ];
                for tag in &c.tags {
                    spans.push(Span::styled(
                        format!(" #{tag}"),
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::DIM),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect()
    };

    // ~~ title ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let title = {
        let base = if let Some(d) = &lc.deck {
            format!(" Cards in '{}' ", d)
        } else {
            " All Cards ".to_string()
        };
        let count = if filter_count == total_count {
            format!("({total_count}) ")
        } else {
            format!("({filter_count}/{total_count}) ")
        };
        let filter_tags: String = lc.tag_filter.iter()
            .map(|t| format!(" #{t}"))
            .collect::<Vec<_>>()
            .join("");
        let search_indicator = if !lc.search_query.is_empty() {
            format!(" [/{}] ({})", lc.search_query, lc.search_scope.label())
        } else {
            String::new()
        };
        format!("{base}{count}{filter_tags}{search_indicator}")
    };

    f.render_stateful_widget(
        List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        v[0],
        &mut lc.list_state,
    );
    clicks.push((v[0], ClickTarget::CardsList));

    // ~~ bottom bar ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let bottom_lines = if lc.search_active {
        vec![
            Line::from(vec![
                Span::styled(" Search ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("[{}]", lc.search_scope.label()),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(": ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{}▌", lc.search_query),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(Span::styled(
                " [Enter] done  │  [Esc] clear  │  [Tab] scope  │  [↑ /↓ ] navigate",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else if !lc.search_query.is_empty() {
        let tag_hint = if !lc.tag_filter.is_empty() { "[#] tags ✓" } else { "[#] filter tags" };
        vec![
            Line::from(Span::styled(
                format!(
                    " [/] edit search  │  [Esc] clear filter  │  {tag_hint}  │  [e] edit  │  [m] toggle SR/daily  │  [d] delete"
                ),
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else {
        let tag_hint = if !lc.tag_filter.is_empty() { "[#] tags ✓" } else { "[#] filter tags" };
        vec![
            Line::from(Span::styled(
                format!(
                    " [j/k] navigate  │  [/] search  │  {tag_hint}  │  [e] edit  │  [m] toggle SR/daily  │  [d] delete  │  [Esc] back"
                ),
                Style::default().fg(Color::DarkGray),
            )),
        ]
    };
    f.render_widget(
        Paragraph::new(bottom_lines).alignment(Alignment::Center),
        v[1],
    );

    // ~~ Overlays ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    if lc.confirm_delete {
        render_confirm_dialog(f, "  Delete this card and all its review history?", size);
    }
    if lc.tag_picker_active {
        render_tag_picker(f, lc, size, clicks);
    }
}

fn render_tag_picker(f: &mut Frame, lc: &mut ListCardsState, size: Rect, clicks: &mut Clicks) {
    let area = centered_rect(46, 20, size);
    f.render_widget(Clear, area);

    let items: Vec<ListItem> = lc.tag_picker_tags.iter().map(|tag| {
        let active = lc.tag_filter.contains(tag);
        let check  = if active { "✓" } else { " " };
        ListItem::new(Line::from(Span::styled(
            format!("  [{check}]  #{tag}"),
            if active {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        )))
    }).collect();

    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(" Filter by Tags  [Space] toggle  [Enter/Esc] close "),
            )
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        area,
        &mut lc.tag_picker_state,
    );
    clicks.push((area, ClickTarget::TagPickerList));
}

// ~~ List Decks ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

pub(super) fn render_list_decks(f: &mut Frame, app: &mut AppState, clicks: &mut Clicks) {
    let size = f.area();
    let ld = match app.list_decks.as_mut() { Some(ld) => ld, None => return };

    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(size);

    let total_count  = ld.decks.len();
    let filtered     = ld.filtered_decks();
    let filter_count = filtered.len();

    let items: Vec<ListItem> = if filtered.is_empty() {
        vec![ListItem::new(Span::styled(
            if total_count == 0 {
                "  No decks yet -- add a card to create one."
            } else {
                "  No decks match the search."
            },
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        filtered.iter()
            .map(|d| {
                let depth = crate::models::deck_depth(d);
                let leaf  = crate::models::deck_leaf(d);
                let (indent, glyph, col) = if depth == 0 {
                    (String::new(), "[D]  ", Color::Yellow)
                } else {
                    ("  ".repeat(depth), "╰─  ", Color::Cyan)
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("  {indent}")),
                    Span::styled(
                        format!("{glyph}{leaf}"),
                        Style::default().fg(col).add_modifier(Modifier::BOLD),
                    ),
                ]))
            })
            .collect()
    };

    let count_label = if filter_count == total_count {
        format!("({total_count})")
    } else {
        format!("({filter_count}/{total_count})")
    };
    let search_indicator = if !ld.search_query.is_empty() {
        format!("  [/{}] ", ld.search_query)
    } else {
        String::new()
    };
    let title = format!(" Decks {count_label}  [:: = sub-deck]{search_indicator}");

    f.render_stateful_widget(
        List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        v[0],
        &mut ld.list_state,
    );
    clicks.push((v[0], ClickTarget::DecksList));

    let bottom_lines = if ld.search_active {
        vec![
            Line::from(vec![
                Span::styled(" Search: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}▌", ld.search_query), Style::default().fg(Color::White)),
            ]),
            Line::from(Span::styled(
                " [j/k] navigate  │  [Esc] cancel search  │  [Enter/r] review",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                " [j/k] navigate  │  [/] search  │  [Enter/r] review  │  [l] list cards  │  [M] toggle daily/SR  │  [d] delete  │  [Esc] back",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    };
    f.render_widget(
        Paragraph::new(bottom_lines).alignment(Alignment::Center),
        v[1],
    );

    if ld.confirm_delete {
        render_confirm_dialog(f, "  Delete this deck, all sub-decks, and ALL their cards?", size);
    }
}

pub(super) fn render_export(f: &mut Frame, app: &mut AppState, clicks: &mut Clicks) {
    let size = f.area();
    let ex   = match app.export.as_mut() { Some(e) => e, None => return };

    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),     // deck list
            Constraint::Length(3),  // reset toggle
            Constraint::Length(3),  // path field
            Constraint::Length(3),  // export button
            Constraint::Length(2),  // hint / status
        ])
        .split(size);

    // ~~ Deck list ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let fl = ex.focus == ExportFocus::DeckList;
    let items: Vec<ListItem> = std::iter::once(ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled("All decks", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ])))
    .chain(ex.decks.iter().map(|d| ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(d.as_str(), Style::default().fg(Color::Yellow)),
    ]))))
    .collect();

    f.render_stateful_widget(
        List::new(items)
            .block(field_block(" Select deck to export ", fl))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        v[0],
        &mut ex.list_state,
    );
    clicks.push((v[0], ClickTarget::ExportDeckList));

    // ~~ Reset toggle ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let fr = ex.focus == ExportFocus::ResetToggle;
    let (check, check_col) = if ex.reset_metadata {
        ("[✓ ] Reset SRS metadata  ", Color::Yellow)
    } else {
        ("[ ] Reset SRS metadata  ", Color::White)
    };
    let desc = if ex.reset_metadata {
        "state wiped: share with a friend or start over"
    } else {
        "state preserved, continue where you left off"
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(check, Style::default().fg(check_col).add_modifier(Modifier::BOLD)),
            Span::styled(desc, Style::default().fg(Color::DarkGray)),
        ]))
        .block(field_block("", fr)),
        v[1],
    );
    clicks.push((v[1], ClickTarget::ExportToggle));

    // ~~ Path field ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let fp = ex.focus == ExportFocus::PathField;
    f.render_widget(
        Paragraph::new(with_cursor(&ex.path, fp))
            .block(field_block(" Save path  ([b] browse) ", fp))
            .style(text_style(fp)),
        v[2],
    );
    clicks.push((v[2], ClickTarget::ExportPathField));

    // ~~ Export button ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let fb  = ex.focus == ExportFocus::ConfirmBtn;
    let lbl = ex.selected_deck().unwrap_or("all decks");
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  ►  Export  \"{lbl}\""),
            if fb { Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD) }
            else  { Style::default().fg(Color::DarkGray) },
        ))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(if fb { BorderType::Thick } else { BorderType::Rounded })
            .border_style(if fb { Style::default().fg(Color::Green) } else { Style::default().fg(Color::DarkGray) })),
        v[3],
    );
    clicks.push((v[3], ClickTarget::ExportConfirmBtn));

    // ~~ Hint / status ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let hint = if let Some(ref msg) = ex.status {
        let col = if msg.starts_with('✓') { Color::Green } else { Color::Red };
        Span::styled(msg.as_str(), Style::default().fg(col))
    } else {
        Span::styled(
            "  [j/k] deck  │  [Tab] focus  │  [Space] toggle  │  [b] browse  │  [Enter] export  │  [Esc] back",
            Style::default().fg(Color::DarkGray),
        )
    };
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), v[4]);
}

pub(super) fn render_import(f: &mut Frame, app: &mut AppState, clicks: &mut Clicks) {
    let size = f.area();
    let im   = match app.import.as_mut() { Some(i) => i, None => return };

    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30), // top padding
            Constraint::Length(3),      // path field
            Constraint::Length(3),      // import button
            Constraint::Length(2),      // hint / status
            Constraint::Min(0),
        ])
        .split(size);

    // ~~ Path field ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let fp = im.focus == ImportFocus::PathField;
    f.render_widget(
        Paragraph::new(with_cursor(&im.path, fp))
            .block(field_block(" Path to JSON file  ([b] browse) ", fp))
            .style(text_style(fp)),
        v[1],
    );
    clicks.push((v[1], ClickTarget::ImportPathField));

    // ~~ Import button ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let fb = im.focus == ImportFocus::ConfirmBtn;
    f.render_widget(
        Paragraph::new(Span::styled(
            "  ►  Import",
            if fb { Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD) }
            else  { Style::default().fg(Color::DarkGray) },
        ))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(if fb { BorderType::Thick } else { BorderType::Rounded })
            .border_style(if fb { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) })),
        v[2],
    );
    clicks.push((v[2], ClickTarget::ImportConfirmBtn));

    // ~~ Hint / status ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    let hint = if let Some(ref msg) = im.status {
        let col = if msg.starts_with('✓') { Color::Green } else { Color::Red };
        Span::styled(msg.as_str(), Style::default().fg(col))
    } else {
        Span::styled(
            "  [b] browse  │  [Tab] focus  │  [Enter] import  │  [Esc] back",
            Style::default().fg(Color::DarkGray),
        )
    };
    f.render_widget(Paragraph::new(hint).alignment(Alignment::Center), v[3]);
}
