use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use std::sync::OnceLock;
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

// markdown.rs simply converts a Markdown string into a ratatui Text<'static>
// for rendering inside a `Paragraph` widget. Fenced code blocks are syntax-highlighted
// via syntect. Tables currently not supported yet

// -- Syntect for syntax highlighting
// building a SyntaxSet from the bundled definitions takes around 30 ms the first
// time using OnceLock, but it only pays that cost once per process

fn syntax_set() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    TS.get_or_init(ThemeSet::load_defaults)
}

fn syn_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

// Syntax-highlight for the given language token + return styled Lines
// falls back to plain text for unknown languages
fn highlight_block(code: &str, lang: &str) -> Vec<Line<'static>> {
    let ss = syntax_set();
    let ts = theme_set();
    let syntax = if lang.is_empty() {
        ss.find_syntax_plain_text()
    } else {
        ss.find_syntax_by_extension(lang)
            .or_else(|| ss.find_syntax_by_name(lang))
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    };
    let theme = &ts.themes["base16-ocean.dark"];
    let mut hl = HighlightLines::new(syntax, theme);

    code.lines()
        .map(|line| {
            // load_defaults_newlines requires the \n to be present
            // for it to work correctly stripping it
            // causes the highlighter to produce plain text
            let line_nl = format!("{line}\n");
            let ranges = hl.highlight_line(&line_nl, ss).unwrap_or_default();
            let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
            spans.extend(ranges.into_iter().map(|(sty, txt)| {
                // trim the \n so it doesn't appear as a trailing span
                let text = txt.trim_end_matches('\n').to_string();
                if text.is_empty() {
                    return Span::raw("");
                }
                Span::styled(text, Style::default().fg(syn_color(sty.foreground)))
            }));
            Line::from(spans)
        })
        .collect()
}

// ~~ Renderer state ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

struct Renderer {
    out: Vec<Line<'static>>,
    cur: Vec<Span<'static>>,  // spans accumulating the current line
    style_stack: Vec<Style>,  // pushed/popped for bold, italic, headings
    // code block
    in_code: bool,
    code_lang: String,
    code_buf: String,
    // lists None = unordered, Some(current_counter) = ordered
    // store the *running* counter and pre-increment on each Item start
    list_stack: Vec<Option<u64>>,
    at_item_start: bool, // true until the first text in a new list item
    // blockquote nesting depth
    bq_depth: u32,
}

impl Renderer {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            cur: Vec::new(),
            // green, matching the answer colour
            style_stack: vec![Style::default().fg(Color::Green)],
            in_code: false,
            code_lang: String::new(),
            code_buf: String::new(),
            list_stack: Vec::new(),
            at_item_start: false,
            bq_depth: 0,
        }
    }

    fn cur_style(&self) -> Style {
        *self.style_stack.last().unwrap_or(&Style::default())
    }

    // commit 'cur' as a finished 'Line', prepending a blockquote gutter if needed
    fn flush(&mut self) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if self.bq_depth > 0 {
            spans.push(Span::styled(
                "│ ".repeat(self.bq_depth as usize),
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.extend(self.cur.drain(..));
        self.out.push(Line::from(spans));
    }

    fn blank(&mut self) {
        self.out.push(Line::from(""));
    }

    fn current_bullet(&self) -> String {
        match self.list_stack.last() {
            Some(Some(n)) => format!("{n}."),
            _ => "•".to_string(),
        }
    }

    /// Append 'text' as styled spans, splitting on embedded newlines and
    /// prepending the list bullet if we're at the start of a new item
    fn push_text(&mut self, text: &str) {
        let style = self.cur_style();

        if self.at_item_start {
            self.at_item_start = false;
            // indent by 2 spaces per nesting level beyond the first
            let depth = self.list_stack.len().saturating_sub(1);
            let indent = "  ".repeat(depth);
            let bullet = self.current_bullet();
            self.cur.push(Span::styled(
                format!("{indent}{bullet} "),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                self.flush();
            }
            if !part.is_empty() {
                self.cur.push(Span::styled(part.to_string(), style));
            }
        }
    }

    fn finish(mut self) -> Text<'static> {
        if !self.cur.is_empty() {
            self.flush();
        }
        // strip trailing blank lines so the widget doesn't waste vertical space
        while self
            .out
            .last()
            .map_or(false, |l: &Line| l.spans.iter().all(|s| s.content.trim().is_empty()))
        {
            self.out.pop();
        }
        Text::from(self.out)
    }
}

// ~~ Public API ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Convert a Markdown string to a ratatui `Text<'static>`.
///
/// Supported elements:
///   - Headings (H1 cyan/underline, H2 blue/bold, H3+ magenta/bold)
///   - **Bold**, *italic*, ~~strikethrough~~
///   - `inline code` (yellow)
///   - Fenced code blocks with syntax highlighting
///   - Unordered and ordered lists (nested)
///   - Blockquotes (│ gutter, dim)
///   - Horizontal rules (─ line)
///   - Soft and hard line breaks
pub fn render(src: &str) -> Text<'static> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    // ENABLE_TABLES intentionally omitted table cells fall through as plain
    // text rather than rendering broken pipe characters everywhere

    let mut r = Renderer::new();

    for event in Parser::new_ext(src, opts) {
        match event {
            // Opening tags
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let style = match level {
                        HeadingLevel::H1 => Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                        HeadingLevel::H2 => Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::BOLD),
                        _ => Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    };
                    r.style_stack.push(style);
                }
                Tag::Strong => {
                    let s = r.cur_style().add_modifier(Modifier::BOLD);
                    r.style_stack.push(s);
                }
                Tag::Emphasis => {
                    let s = r.cur_style().add_modifier(Modifier::ITALIC);
                    r.style_stack.push(s);
                }
                Tag::Strikethrough => {
                    let s = r.cur_style().add_modifier(Modifier::CROSSED_OUT);
                    r.style_stack.push(s);
                }
                Tag::BlockQuote(_) => {
                    r.bq_depth += 1;
                }
                Tag::List(start) => {
                    // Store start-1 so the pre-increment on the first Item lands
                    // on `start` exactly
                    r.list_stack.push(start.map(|n| n.saturating_sub(1)));
                }
                Tag::Item => {
                    // increment counter for ordered lists.
                    if let Some(Some(n)) = r.list_stack.last_mut() {
                        *n += 1;
                    }
                    if !r.cur.is_empty() {
                        r.flush();
                    }
                    r.at_item_start = true;
                }
                Tag::CodeBlock(kind) => {
                    r.in_code = true;
                    r.code_lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    r.code_buf.clear();
                }
                _ => {}
            },

            // closing tags
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    if !r.cur.is_empty() {
                        r.flush();
                    }
                    r.blank();
                    r.style_stack.pop();
                }
                TagEnd::Paragraph => {
                    if !r.cur.is_empty() {
                        r.flush();
                    }
                    r.blank();
                }
                TagEnd::Item => {
                    if !r.cur.is_empty() {
                        r.flush();
                    }
                }
                TagEnd::List(_) => {
                    r.list_stack.pop();
                    // only add a blank line after the outermost list closes
                    // not after every nested list
                    if r.list_stack.is_empty() {
                        r.blank();
                    }
                }
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {
                    r.style_stack.pop();
                }
                TagEnd::BlockQuote(_) => {
                    if !r.cur.is_empty() {
                        r.flush();
                    }
                    if r.bq_depth > 0 {
                        r.bq_depth -= 1;
                    }
                    r.blank();
                }
                TagEnd::CodeBlock => {
                    // render a thin separator above and below the highlighted block.
                    let sep = Span::styled(
                        "  ──────────────────────────────".to_string(),
                        Style::default().fg(Color::DarkGray),
                    );
                    r.out.push(Line::from(vec![sep.clone()]));
                    let lang = r.code_lang.clone();
                    let code = r.code_buf.clone();
                    for line in highlight_block(&code, &lang) {
                        r.out.push(line);
                    }
                    r.out.push(Line::from(vec![sep]));
                    r.blank();
                    r.in_code = false;
                    r.code_lang.clear();
                    r.code_buf.clear();
                }
                _ => {}
            },

            // inline events
            Event::Text(t) => {
                if r.in_code {
                    r.code_buf.push_str(&t);
                } else {
                    r.push_text(&t.to_string());
                }
            }
            // inline code span distinct yellow colour so it stands out0
            Event::Code(t) => {
                r.cur.push(Span::styled(
                    format!("`{}`", t),
                    Style::default().fg(Color::Yellow),
                ));
            }
            // soft break (single newline in source) becomes a space
            Event::SoftBreak => {
                r.cur.push(Span::raw(" "));
            }
            // hard break (two trailing spaces or backslash) starts a new line
            Event::HardBreak => {
                r.flush();
            }
            // thematic break (---, ***) rendered as a dim horizontal rule.
            Event::Rule => {
                r.out.push(Line::from(Span::styled(
                    "─".repeat(48),
                    Style::default().fg(Color::DarkGray),
                )));
                r.blank();
            }
            _ => {}
        }
    }

    r.finish()
}
