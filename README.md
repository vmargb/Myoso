## arrow.el
*Step-by-step flashcards for the terminal*
*Built in Rust with [Ratatui](https://ratatui.rs/) and [SQLite](https://sqlite.org/)*

---

<p align="center">
    <img src="screenshots/menu.png" alt="menu" width="45%" />
    <img src="screenshots/cards.png" alt="cards" width="45%" />
</p>
<p align="center">
    <img src="screenshots/multi.png" alt="multi cards" width="90%" />
</p>

---

## What is it?

Standard flashcard apps are great for 1:1 facts and definitions but struggle with long chains of thought or
**procedural knowledge**:
- "What are all the verb endings in past tense?"
- "how do I reverse a linked list?"
- "walk me through this derivation".

Standard flashcard *can* be used in this way, but end up adding too much mental load to each card. Myoso models each procedure as an ordered sequence of steps. During review you reveal and rate each step individually, unlocking later steps only after earlier ones are recalled. Forgetting an earlier step automatically blocks access to the subsequent steps until you rebuild the chain again, which reinforces the full procedural flow rather than merely memorising isolated bits.

---

## Installation

If compiling from source, install Rust with your package-manager or from [rustup.rs](https://rustup.rs)

```bash
git clone https://github.com/vmargb/Myoso.git
cd myoso
cargo run
```

---

## Review TUI controls

| Key | Action |
|-----|--------|
| `Space` / `Enter` | Reveal the answer for the current step |
| `1` | Rate: **Again** - total blank, short reset |
| `2` | Rate: **Hard** - recalled incorrectly |
| `3` | Rate: **Good** - recalled correctly but with effort |
| `4` | Rate: **Great** - recalled correctly & quickly |
| `5` | Rate: **Easy** - instant/effortless |
| `q` / `Esc` | Quit the session |

---

## Scheduling algorithm

Uses a simplified SM-2 variant. Each **item** (step) is tracked independently:
Ease is clamped to `[1.3, 3.0]`. The first interval is always 1 day.

For **multi-step** cards the session logic is:
- Find the earliest-due step (by position).
- Include **all preceding steps** plus that step in the session.
- This forces you to rebuild the full chain from step 1 every time, not just
  practise the due step in isolation.

