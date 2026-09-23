// ~~~ outline.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// The "outline" import format: a compact way to describe cards that an AI (or
// a person) can write by hand
//
// The normal export/import JSON carries everything the app stores, UUIDs,
// parent links, positions, scheduling state, so its both huge and easy to get
// wrong. An outline holds ONLY content. 
//
// One card per JSON object. Three shapes:
//
//   {"deck":"C::Pointers","question":"What does *p++ do?","answer":"..."}
//
//   {"deck":"C::Memory","question":"Heap lifecycle?",
//    "steps":["malloc and check NULL",
//             {"answer":"use it","branches":[
//                {"name":"Grow","steps":["realloc, use the returned pointer"]},
//                {"name":"Done","steps":["free once, then set to NULL"]}]}]}
//
//   {"deck":"Systems","question":"SSD or HDD?",
//    "paths":[{"name":"SSD","steps":["..."]},{"name":"HDD","steps":["..."]}]}
//
// Optional on any card: "tags": ["pointers"]. On a simple card only: "reversible": true
// A step is either a plain string or {"name"?, "answer", "branches"?}
//
//   * one object per line (JSON Lines) is the recommended layout, but
//     pretty-printed objects, a surrounding [ ... ] array, ``` fences and
//     chatter between the cards are all fine
//   * raw newlines and tabs inside strings are repaired (code answers!)
//   * a card that is broken or cut off is reported with its line number and
//     skipped, the rest are still imported
//   * unknown fields, missing names on sibling branches, `branches` that
//     aren't on the last step, and so on are errors with a precise location

use std::collections::HashSet;
use std::fmt;

use serde_json::{Map, Value};

use crate::models::StepDraft;

/// A card with more steps than this is almost certainly a mistake
pub const MAX_STEPS_PER_CARD: usize = 100;
/// Longest question / answer / step text accepted
pub const MAX_TEXT_LEN: usize = 20_000;

// ~~~ Parsed result ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Simple { answer: String, reversible: bool },
    /// steps in DFS pre-order, ready for `Store::add_multi_card`
    Chain { steps: Vec<StepDraft> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutlineCard {
    /// 1-based line of the file the card starts on
    pub line:     usize,
    pub deck:     String,
    pub question: String,
    pub tags:     Vec<String>,
    pub body:     Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineError {
    pub line:    usize,
    /// the card's question, when it could be read, to make the message findable
    pub label:   Option<String>,
    pub message: String,
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.label {
            Some(q) => write!(f, "line {} (\"{}\"): {}", self.line, shorten(q, 40), self.message),
            None    => write!(f, "line {}: {}", self.line, self.message),
        }
    }
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub cards:  Vec<OutlineCard>,
    pub errors: Vec<LineError>,
}

/// What an import did, for the UI
#[derive(Debug, Default)]
pub struct OutlineSummary {
    pub cards_imported:     usize,
    pub simple_cards:       usize,
    /// multi-step cards that are one straight chain
    pub chain_cards:        usize,
    /// multi-step cards with at least one fork
    pub branching_cards:    usize,
    pub steps_imported:     usize,
    pub decks:              usize,
    pub duplicates_skipped: usize,
    pub errors:             Vec<LineError>,
}

impl OutlineSummary {
    /// One line for the status area, starts with ✓ or ✗ (the UI colours by it)
    pub fn headline(&self) -> String {
        if self.cards_imported == 0 {
            return if self.duplicates_skipped > 0 && self.errors.is_empty() {
                format!("✓  nothing new: all {} card(s) are already in your decks", self.duplicates_skipped)
            } else {
                "✗  no cards were imported".to_string()
            };
        }
        let mut s = format!(
            "✓  {} card(s) added to {} deck(s): {} simple, {} chain(s), {} branching  ({} steps)",
            self.cards_imported, self.decks, self.simple_cards, self.chain_cards,
            self.branching_cards, self.steps_imported,
        );
        if self.duplicates_skipped > 0 {
            s.push_str(&format!("  ·  {} duplicate(s) skipped", self.duplicates_skipped));
        }
        s
    }

    /// Every problem, one per line, in a form that can be pasted back to an AI
    pub fn error_report(&self) -> String {
        self.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")
    }
}

// ~~~ Public entry points ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

/// Parse an outline file. Never fails as a whole: bad cards become `errors`
pub fn parse(text: &str) -> Parsed {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parsed = Parsed::default();

    for rec in split_records(text) {
        if !rec.complete {
            parsed.errors.push(LineError {
                line: rec.line,
                label: guess_question(&rec.text),
                message: "the card is cut off: its JSON never closes (output that hit a length \
                          limit, or a missing quote or bracket). Ask the AI to resend it"
                    .to_string(),
            });
            continue;
        }
        match serde_json::from_str::<Value>(&rec.text) {
            Err(e) => parsed.errors.push(LineError {
                line: rec.line,
                label: guess_question(&rec.text),
                message: format!("not valid JSON: {e}"),
            }),
            Ok(v) => match convert(&v) {
                Ok(mut card) => {
                    card.line = rec.line;
                    parsed.cards.push(card);
                }
                Err(message) => parsed.errors.push(LineError {
                    line: rec.line,
                    label: v.get("question").and_then(|q| q.as_str()).map(str::to_string),
                    message,
                }),
            },
        }
    }

    if parsed.cards.is_empty() && parsed.errors.is_empty() {
        parsed.errors.push(LineError {
            line: 1,
            label: None,
            message: "no cards found. Expected one JSON object per line, like \
                      {\"deck\": \"...\", \"question\": \"...\", \"answer\": \"...\"}"
                .to_string(),
        });
    }
    parsed
}

pub fn looks_like_export(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if !t.starts_with('{') || !t.contains("\"cards\"") {
        return false;
    }
    match serde_json::from_str::<Value>(t) {
        Ok(Value::Object(o)) => matches!(o.get("cards"), Some(Value::Array(_))) && !o.contains_key("question"),
        _ => false,
    }
}

/// Two cards are "the same" if they are in the same deck and ask the same
/// question, ignoring case and spacing. Used to make re-imports harmless
pub fn dup_key(deck: &str, question: &str) -> (String, String) {
    (
        deck.trim().to_lowercase(),
        question.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase(),
    )
}

// ~~~ Splitting text into card records ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

struct Record {
    line:     usize,
    text:     String,
    /// the braces balanced before the text ended / the next card began
    complete: bool,
}

fn starts_with_at(chars: &[char], at: usize, pat: &str) -> bool {
    pat.chars().enumerate().all(|(k, p)| chars.get(at + k) == Some(&p))
}

/// Find each top-level `{ ... }` in `text`. Anything outside braces (fences,
/// commas, a wrapping `[`, "Here are your cards:") is ignored. Inside a
/// record the scan is string-aware, so braces in code samples don't confuse
/// it, and raw newlines/tabs inside strings are escaped on the way through
///
/// If a card is left open and the next line begins a new card (`{"deck"`),
/// the open one is closed off as incomplete so a single broken card can't
/// swallow the ones after it
fn split_records(text: &str) -> Vec<Record> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1usize;

    while i < n {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c != '{' {
            i += 1;
            continue;
        }

        let start_line = line;
        let mut buf = String::new();
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        let mut complete = false;

        while i < n {
            let c = chars[i];

            if c == '\n' {
                line += 1;
                // a raw newline inside a string is invalid JSON but very common in code
                buf.push_str(if in_str { "\\n" } else { "\n" });
                esc = false;
                i += 1;
                let mut j = i;
                while j < n && matches!(chars[j], ' ' | '\t' | '\r') {
                    j += 1;
                }
                if starts_with_at(&chars, j, "{\"deck\"") {
                    break; // this card is unfinished, and a new one starts on the next line
                }
                continue;
            }

            if in_str {
                if esc {
                    buf.push(c);
                    esc = false;
                } else if c == '\\' {
                    buf.push(c);
                    esc = true;
                } else if c == '"' {
                    buf.push(c);
                    in_str = false;
                } else if c == '\r' {
                    // dropped
                } else if c == '\t' {
                    buf.push_str("\\t");
                } else if (c as u32) < 0x20 {
                    buf.push_str(&format!("\\u{:04x}", c as u32));
                } else {
                    buf.push(c);
                }
            } else {
                match c {
                    '"' => {
                        buf.push(c);
                        in_str = true;
                    }
                    '{' | '[' => {
                        depth += 1;
                        buf.push(c);
                    }
                    '}' | ']' => {
                        depth -= 1;
                        buf.push(c);
                        if depth <= 0 {
                            complete = true;
                            i += 1;
                            break;
                        }
                    }
                    '\r' => {}
                    _ => buf.push(c),
                }
            }
            i += 1;
        }

        out.push(Record { line: start_line, text: buf, complete });
    }
    out
}

/// attempt to read of `"question": "..."` from text that isn't valid JSON, so
/// the error message can say which card it is
fn guess_question(text: &str) -> Option<String> {
    let at = text.find("\"question\"")? + "\"question\"".len();
    let rest = &text[at..];
    let open = rest.find('"')? + 1;
    let mut out = String::new();
    let mut esc = false;
    for c in rest[open..].chars() {
        if esc {
            out.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
        if out.chars().count() > 200 {
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

// ~~~ Validating one card

fn shorten(s: &str, max: usize) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        one_line.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn as_object<'a>(v: &'a Value, ctx: &str) -> Result<&'a Map<String, Value>, String> {
    v.as_object().ok_or_else(|| format!("{ctx}expected an object {{ ... }}"))
}

fn only_keys(m: &Map<String, Value>, allowed: &[&str], ctx: &str) -> Result<(), String> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!(
                "{ctx}unknown field \"{k}\" (allowed here: {})",
                allowed.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    Ok(())
}

fn clean_text(raw: &str, what: &str, ctx: &str) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(format!("{ctx}{what} is empty"));
    }
    if t.chars().count() > MAX_TEXT_LEN {
        return Err(format!("{ctx}{what} is longer than {MAX_TEXT_LEN} characters"));
    }
    Ok(t.to_string())
}

fn required_text(m: &Map<String, Value>, key: &str, ctx: &str) -> Result<String, String> {
    match m.get(key) {
        None => Err(format!("{ctx}missing \"{key}\"")),
        Some(Value::String(s)) => clean_text(s, &format!("\"{key}\""), ctx),
        Some(_) => Err(format!("{ctx}\"{key}\" must be text")),
    }
}

/// `Ok(None)` if absent or blank
fn optional_text(m: &Map<String, Value>, key: &str, ctx: &str) -> Result<Option<String>, String> {
    match m.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => clean_text(s, &format!("\"{key}\""), ctx).map(Some),
        Some(_) => Err(format!("{ctx}\"{key}\" must be text")),
    }
}

fn clean_deck(raw: &str) -> Result<String, String> {
    let parts: Vec<String> = raw.split("::").map(|p| p.trim().to_string()).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(format!(
            "deck \"{raw}\" has an empty part. Use names like \"Math\" or \"Math::Calculus\""
        ));
    }
    Ok(parts.join("::"))
}

fn clean_tags(v: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let arr = v.as_array().ok_or("\"tags\" must be a list of text")?;
    let mut out: Vec<String> = Vec::new();
    for t in arr {
        let s = t.as_str().ok_or("\"tags\" must be a list of text")?.trim().to_lowercase();
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    }
    Ok(out)
}

struct Builder {
    drafts: Vec<StepDraft>,
}

impl Builder {
    fn push(&mut self, parent: Option<u32>, name: String, answer: String) -> Result<u32, String> {
        if self.drafts.len() >= MAX_STEPS_PER_CARD {
            return Err(format!("more than {MAX_STEPS_PER_CARD} steps in one card: split it into several cards"));
        }
        let key = self.drafts.len() as u32;
        self.drafts.push(StepDraft { key, db_id: None, parent, name, answer, image: None });
        Ok(key)
    }

    /// a `steps` list: each step follows the one before it. Pushed in DFS order
    /// (a step, then everything below it), which is what the editor and the
    /// database expect. `first_name` names the first step (a branch's name)
    fn chain(
        &mut self,
        steps: &Value,
        parent: Option<u32>,
        first_name: Option<&str>,
        ctx: &str,
    ) -> Result<(), String> {
        let arr = steps.as_array().ok_or_else(|| format!("{ctx}\"steps\" must be a list"))?;
        if arr.is_empty() {
            return Err(format!("{ctx}\"steps\" is empty"));
        }
        let mut parent = parent;
        for (i, s) in arr.iter().enumerate() {
            let sctx = format!("{ctx}steps[{i}]: ");
            let (own_name, answer, branches) = match s {
                Value::String(t) => (String::new(), clean_text(t, "the step", &sctx)?, None),
                Value::Object(m) => {
                    only_keys(m, &["name", "answer", "branches"], &sctx)?;
                    (
                        optional_text(m, "name", &sctx)?.unwrap_or_default(),
                        required_text(m, "answer", &sctx)?,
                        m.get("branches"),
                    )
                }
                _ => {
                    return Err(format!(
                        "{sctx}a step must be text, or an object with an \"answer\""
                    ))
                }
            };
            let name = match (i, first_name) {
                (0, Some(n)) if !n.trim().is_empty() => n.trim().to_string(),
                _ => own_name,
            };
            let key = self.push(parent, name, answer)?;
            parent = Some(key);

            if let Some(b) = branches {
                if i + 1 != arr.len() {
                    return Err(format!(
                        "{sctx}\"branches\" must be on the LAST step of a list. The steps after \
                         it can't belong to any branch, so move them into one of the branches"
                    ));
                }
                let list = b
                    .as_array()
                    .ok_or_else(|| format!("{sctx}\"branches\" must be a list"))?;
                if list.is_empty() {
                    return Err(format!("{sctx}\"branches\" is empty"));
                }
                self.fork(list, Some(key), "branches", &sctx)?;
            }
        }
        Ok(())
    }

    /// several alternatives under one parent (`None` = the question itself)
    fn fork(
        &mut self,
        list: &[Value],
        parent: Option<u32>,
        label: &str,
        ctx: &str,
    ) -> Result<(), String> {
        let need_names = list.len() > 1;
        let mut seen: HashSet<String> = HashSet::new();
        for (j, b) in list.iter().enumerate() {
            let bctx = format!("{ctx}{label}[{j}]: ");
            let m = as_object(b, &bctx)?;
            only_keys(m, &["name", "steps"], &bctx)?;
            let steps = m.get("steps").ok_or_else(|| format!("{bctx}missing \"steps\""))?;
            let name = optional_text(m, "name", &bctx)?;

            if need_names {
                // the first step's own name counts too, as it would in the editor
                let first_own = steps
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|s| s.as_object())
                    .and_then(|o| o.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty());
                let effective = name.clone().or(first_own).ok_or_else(|| {
                    format!(
                        "{bctx}needs a \"name\". Alternatives under the same step must each be \
                         named (like \"SSD\" / \"HDD\") so they can be told apart in review"
                    )
                })?;
                if !seen.insert(effective.to_lowercase()) {
                    return Err(format!("{bctx}another alternative is already named \"{effective}\""));
                }
            }
            self.chain(steps, parent, name.as_deref(), &bctx)?;
        }
        Ok(())
    }
}

fn convert(v: &Value) -> Result<OutlineCard, String> {
    let m = as_object(v, "")?;
    only_keys(m, &["deck", "question", "answer", "steps", "paths", "tags", "reversible"], "")?;

    let deck = clean_deck(&required_text(m, "deck", "")?)?;
    let question = required_text(m, "question", "")?;
    let tags = clean_tags(m.get("tags"))?;

    let has = |k: &str| m.get(k).is_some();
    let shapes = ["answer", "steps", "paths"].iter().filter(|k| has(k)).count();
    if shapes == 0 {
        return Err("needs one of \"answer\" (a simple card), \"steps\" (a chain), or \"paths\" \
                    (alternative chains)"
            .to_string());
    }
    if shapes > 1 {
        return Err("use only ONE of \"answer\", \"steps\" and \"paths\" on a card".to_string());
    }
    if has("reversible") && !has("answer") {
        return Err("\"reversible\" only applies to simple cards (those with an \"answer\")".to_string());
    }

    let body = if has("answer") {
        let reversible = match m.get("reversible") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err("\"reversible\" must be true or false".to_string()),
        };
        Body::Simple { answer: required_text(m, "answer", "")?, reversible }
    } else {
        let mut b = Builder { drafts: Vec::new() };
        if has("steps") {
            b.chain(&m["steps"], None, None, "")?;
        } else {
            let list = m["paths"].as_array().ok_or("\"paths\" must be a list")?;
            if list.is_empty() {
                return Err("\"paths\" is empty".to_string());
            }
            b.fork(list, None, "paths", "")?;
        }
        // the same rule the editor enforces, as a backstop
        crate::tree::check_branch_names(&b.drafts).map_err(str::to_string)?;
        Body::Chain { steps: b.drafts }
    };

    Ok(OutlineCard { line: 0, deck, question, tags, body })
}
