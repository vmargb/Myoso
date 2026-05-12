## Myoso
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

### Quick install (recommended)

You can install the latest release with a single command:

#### Linux / macOS
```sh
curl -sSf https://raw.githubusercontent.com/vmargb/Myoso/main/install.sh | sh
```

#### Windows
```powershell
irm https://raw.githubusercontent.com/vmargb/Myoso/main/install.ps1 | iex
```

After installation, run:
```sh
myoso
```

## Updating

To update to the latest version:
```sh
myoso update
```

Check your current version with:
```sh
myoso --version
```

## Building from source (optional)
If you want to compile it manually, install Rust from [rustup.rs](https://rustup.rs) (or with your package-manager) then:

```sh
git clone https://github.com/vmargb/Myoso.git
cd myoso
cargo run
```

---

## Features

- **Analytics**: Simple statistics of deck/card data, current progress and currently due sessions.
- **Reversible cards**: Support for making cards reversible, where q->a becomes a->q.
- **Step-by-step cards**: Cards that require multiple steps towards the answers, where each step is rated individually.
- **Multi-line support**: Add or edit multiple lines within question and answer textboxes.
- **Import/Export**: Export a specific deck or all decks into `JSON` format, which can be imported by anyone else.
- **External Editor Support**: Open any textbox in your default editor (e.g., Neovim, VS Code, Notepad) directly from the TUI. Works seamlessly across all operating systems. Save and close the editor to automatically return to the TUI with your updated text.
- **Markdown Rendering**: All markdown formatting (e.g., bold, italics, lists) is now rendered during review sessions.
- **Syntax Highlighting**: Code blocks in markdown are syntax-highlighted for any programming language.
- **Image preview**: Insert path to images locally into an answer/step (handwritten work or screenshot)
- **Subdecks**: Organise decks into 'sub-decks', allowing clear separation (e.g., vocabulary, grammar)
- **Tags**: Organise cards even further by adding tags, allowing easy search filtering

### Daily cards

Spaced-repetition isn't everything, some cards require more attention than others,
like the most essential cards in your upcoming exam.
You can mark new cards as daily or move existing SRS cards into your dailies to have them
shown in every review session once per day, bypassing any scheduling applied to them.
Once the demand is gone(e.g. after the exam), you can move those cards back into SRS.

---

## Scheduling algorithm

Uses a dynamic SM-2 variant where each **item** (step) is tracked independently:

| Key | Action                               |
|-----|--------------------------------------|
| `1` | **Again** - total blank, short reset |
| `2` | **Hard** - recalled incorrectly |
| `3` | **Good** - recalled correctly but with effort |
| `4` | **Great** - recalled quickly(under 5s) |
| `5` | **Instant** - instant/effortless (use rarely) |

For **multi-step** cards the session logic is:
- Find the earliest-due step (by position).
- Include **all preceding steps** plus that step in the session.
This forces you to rebuild the full chain from step 1 every time, not just
practise the due step in isolation.
