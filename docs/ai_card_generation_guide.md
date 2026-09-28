# AI Card Generation Guide

## Copy the Prompt Template

> **Copy this Prompt:**
> ```text
> You are an expert learning assistant. I will provide you with source material, and you will use cognitive load theory to convert it into a set of interconnected learning cards formatted STRICTLY as JSONL (one JSON object per line).
> 
> These cards form a tree structure. Each card must have:
> - `id`: A unique string identifier (e.g., "card_1").
> - `front`: The question, concept, or prompt.
> - `back`: The answer or explanation.
> - `parent_id`: The `id` of the card that this concept branches off from. Use `null` for root/main concepts.
> 
> DO NOT output markdown blocks (```json). Output ONLY raw JSONL. 
> 
> Example format:
> {"id": "1", "front": "Main Concept", "back": "Definition of main concept", "parent_id": null}
> {"id": "2", "front": "Sub-concept A", "back": "Details about A", "parent_id": "1"}
> {"id": "3", "front": "Sub-concept B", "back": "Details about B", "parent_id": "1"}
> {"id": "4", "front": "Finer detail of B", "back": "More on B", "parent_id": "3"}
> 
> The source material to convert:
> [PASTE_SOURCE_TEXT_HERE]
> ```

Replace `[PASTE_SOURCE_TEXT_HERE]` with your actual study notes, article, or text.

## Save the Output

1. Copy the generated JSONL text provided by the AI.
2. Open a plain text editor (like Notepad on Windows or TextEdit on Mac).
3. Paste the text.
4. Save the file with a `.jsonl` extension (for example: `biology_cards.jsonl`).

## Import

1. Open the application.
2. Navigate to **Import Cards**.
3. Select your newly saved `.jsonl` file.
4. Done!
