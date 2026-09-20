## Myoso
*Step-by-step flashcards for the terminal*
*Built in Rust with [Ratatui](https://ratatui.rs/) and [SQLite](https://sqlite.org/)*

---

<table align="center">
  <tr>
    <td align="center">
      <img src="screenshots/addcard.png" alt="Add Card" width="100%"/>
      <br><b>Add Card</b>
    </td>
    <td align="center">
      <img src="screenshots/examplereview.png" alt="Review Session" width="100%"/>
      <br><b>Review Session</b>
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center">
      <img src="screenshots/menuscreen.png" alt="Menu" width="90%"/>
      <br><b>Main Menu</b>
    </td>
  </tr>
</table>

---

## What is it?

Ordinary flashcard apps are great for 1:1 facts and definitions but struggle with *long chain of thought* or
**Step-by-step knowledge**:
- "What are all the verb endings in past tense?"
- "how do you reverse a linked list?"
- "walk me through this derivation".

Standard flashcard *can* be used in this way, but wind up becoming **heavy**, carrying too much mental load. A deck with hundreds(or thousands) of these *heavy* cards quickly become unsustainable to maintain long-term.

Myoso fixes this problem by modellling your answer as an ordered sequence of steps. During review, you **reveal and rate each step** individually, unlocking later steps only after earlier ones are recalled well. Likewise, forgetting an *earlier* step naturally blocks access to the subsequent steps until you **rebuild the chain again**. This reinforces the full procedural flow, rather than merely memorising isolated bits as normal flashcards would do.

---

## Installation

### Quick install (recommended)

You can install the latest release with a single command:

#### Linux / macOS
```sh
curl -sSf https://raw.githubusercontent.com/vmargb/Myoso/main/install.sh | sh
```

#### Windows
Open "PowerShell" or "Windows Terminal":
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

Myoso keeps simple things simple. A plain card can be made reversible so that question -> answer also works as answer -> question, and a step-by-step card lets you rate every step individually. When you want to see how you are doing, the analytics screen shows statistics for your decks and cards and what is currently due.

Cards are organised with decks, and decks can be split into sub-decks such as vocabulary and grammar, while tags let you filter and search across them. Answers and steps support full markdown, including bold, italics and lists, and code blocks are syntax-highlighted for any programming language. If your working is on paper, you can attach the path of a local image, such as a handwritten derivation or a screenshot, to any answer or step and see it during review.

Writing long answers in a tiny text box is painful, so any text box can be opened in your default editor, whether that is Neovim, VS Code or Notepad. Close the editor and you land back in the app with your updated text. Decks can be exported to JSON, either one at a time or all together and imported by anyone else. For the times you want to test yourself before peeking, an optional Feynman step asks you to explain the answer in your own words before revealing it, so you can compare the two.

### Daily cards

Spaced-repetition isn't everything, some cards require more attention than others,
like the most essential cards in your upcoming exam.
You can mark new cards as daily or move existing SRS cards into your dailies to have them
shown in every review session once per day, bypassing any scheduling applied to them.
Once the demand is gone(e.g. after the exam), you can move those cards back into SRS.

---

## Scheduling algorithm

`Myoso` now uses **FSRS** (Free Spaced Repetition Scheduler). FSRS has been modified to track each **item** (step) independently using two properties:
1. *stability* (how long the memory lasts)
2. *difficulty* (how hard the item is for you personally)

You rate each step with a single key:

| Key | Rating | Meaning |
|-----|--------|---------|
| `1` | **Again** | Complete blank |
| `2` | **Hard** | Correct, but with major effort |
| `3` | **Good** | Correct, with some effort |
| `4` | **Easy** | Recalled instantly and effortlessly |

> [!NOTE]
> It is highly recommended to avoid the `4` option.
> Reserving it only for **rare** occassions for optimal recall performance.

Additionally, `Myoso` handles a unique problem called *re-exposure*, triggered by repeatedly rating a step as `good` or `easy` after you had just recently seen it(from rebuilding the same steps in the **same session**). This would artificially *inflate* the cards schedule because you stacked multiple positive ratings on the same steps. Therefore, any *re-exposure* ratings now introduce a **tapering effect** against the SRS algorithm to prevent this inflation. The exception is a step that has just been marked as due again because something earlier was forgotten. That step needs a real rating, so its rating always counts.

---

## Leech detection and intervention

Some steps just don't stick, and collect `again` and `hard` ratings over and over. In Anki these are called leeches and are usually pulled out of the review queue. Myoso can't simply remove a step from the middle of a chain, so it offers two alternatives instead.

A **cloze test** lets you highlight the exact word, formula or phrase that keeps tripping you up, and future reviews hide just that part. These lighter reviews never touch your normal schedule. They exist only to rebuild the specific piece of recall that was missing, and once it is back the step returns to normal full-recall review. If the whole step is too much to hold at once, you can instead **split it in two**, so that each half is easier to remember.
