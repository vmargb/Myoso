package com.vmargb.myoso.data

import android.content.Context
import java.io.File
import java.util.UUID

// =================================
//     *** MARKDOWN PARSER ***
// =================================
/*
 * This file is a one-file importer that parses
 * a home-made markdown dialect into rows in the database.
 * The dialect is as follows:
    ---
    title: French Basics
    id: fr-basic-001
    ---

    # Notes
    You can write **markdown** here.

    ---cards---

    id: 1
    front: Bonjour
    back: Hello
    tags: greeting,basic

    ---

    id: 2
    front: Au revoir
    back: Goodbye
    tags: greeting
    pinned: true

    ---

    and so on...
    the first '---' block is YAML frontmatter for deck metadata
    the '# Notes' section is optional and can contain any markdown
    the '---cards---' section can contain any number of cards
 */

// ========== Data Classes ==========

data class DeckMetadata( // YAML frontmatter
    val title: String,
    val id: String,
    var reversibleByDefault: Boolean = false // all false by default
)

data class ParsedCard( // each card in the 'cards' section
    val id: String,
    val front: String,
    val back: String,
    val tags: String? = null,
    val pinned: String? = null,
    val reversible: Boolean
)

data class ParsedDeck( // the whole parsed deck
    val metadata: DeckMetadata,
    val notes: String?,
    val cards: List<ParsedCard>
)

// ===================================


// ===================================================
//     *** DeckMarkdownParser Class ***
// ===================================================
/**
 *  This class is responsible for parsing markdown files
 *  and converting them into structured data that can be
 *  imported into the database.
 *
 *  the process is three main steps:
 *  1. Markdown text (a file you drop in)
 *  2. Kotlin objects (ParsedDeck, ParsedCard, etc...)
 *  3. Room entities (DeckEntity, CardEntity, NoteEntity) that actually live in SQLite
 *
 *  DeckMarkdownParser simply goes 1 → 2 → 3 in one call
 */
class DeckMarkdownParser(
    private val context: Context,
    private val cardDao: CardDao,
    private val deckDao: DeckDao,
    private val noteDao: NoteDao
) {
    
    /**
     * Parses a markdown deck file and imports it into the database
     * @param filePath Path to the markdown file
     * @return ParsedDeck object with metadata, notes, and cards
     */
    suspend fun parseAndImport(filePath: String): ParsedDeck {
        val file = File(filePath)
        val content = file.readText()
        val parsedDeck = parseMarkdown(content) // parse yaml, notes, cards into ParsedDeck
        
        // Import deck metadata
        val deckEntity = DeckEntity( // create DeckEntity from ParsedDeck
            id = parsedDeck.metadata.id,
            name = parsedDeck.metadata.title,
            description = parsedDeck.notes,
            createdAt = System.currentTimeMillis(),
            updatedAt = System.currentTimeMillis()
        )
        deckDao.insertDeck(deckEntity)
        
        // Import notes if present
        if (!parsedDeck.notes.isNullOrBlank()) { // if notes are present
            val noteEntity = NoteEntity( // create NoteEntity
                id = UUID.randomUUID().toString(),
                deckId = parsedDeck.metadata.id, // foreign key to Deck
                markdownBody = parsedDeck.notes,
                updatedAt = System.currentTimeMillis()
            )
            noteDao.insertNote(noteEntity)
        }
        
        // Import cards
        val cardEntities = parsedDeck.cards.map { card -> // map each card to a CardEntity
            CardEntity(
                id = card.id,
                deckId = parsedDeck.metadata.id, // foreign key to Deck
                front = card.front.trim(),
                back = card.back.trim(),
                tags = card.tags,
                pinned = card.pinned,
                isReversible = card.reversible ?: parsedDeck.metadata.reversibleByDefault,
                intervalDays = 1,
                easeFactor = 2.3,
                reviewCount = 0,
                consecutiveFails = 0,
                lastReviewedAt = 0L,
                nextDueAt = System.currentTimeMillis(),
                createdAt = System.currentTimeMillis()
            )
        }
        cardDao.insertCards(cardEntities)
        
        return parsedDeck
    }
    
    /**
     * Parses markdown content into structured data
     */
    fun parseMarkdown(content: String): ParsedDeck {
        val lines = content.lines()
        var currentIndex = 0
        
        // Parse frontmatter
        val metadata = parseFrontmatter(lines, currentIndex)
        currentIndex = metadata.second
        
        // Parse notes section
        val notesResult = parseNotes(lines, currentIndex)
        val notes = notesResult.first
        currentIndex = notesResult.second
        
        // Parse cards section
        val cards = parseCards(lines, currentIndex)
        
        return ParsedDeck(
            metadata = metadata.first,
            notes = notes,
            cards = cards
        )
    }
    
    private fun parseFrontmatter(lines: List<String>, startIndex: Int): Pair<DeckMetadata, Int> {
        if (startIndex >= lines.size || lines[startIndex] != "---") { // frontmatter must start with ---
            throw IllegalArgumentException("Expected frontmatter to start with '---'")
        }
        
        var currentIndex = startIndex + 1 // move past the starting ---
        val metadataMap = mutableMapOf<String, String>() // to hold key-value pairs
        
        while (currentIndex < lines.size && lines[currentIndex] != "---") { // until we hit the ending ---
            val line = lines[currentIndex].trim()
            if (line.isNotEmpty()) {
                val colonIndex = line.indexOf(':') // find key:value
                if (colonIndex > 0) {
                    val key = line.substring(0, colonIndex).trim() // get key on LHS
                    val value = line.substring(colonIndex + 1).trim() // get value on RHS
                    metadataMap[key] = value
                }
            }
            currentIndex++ // repeat for next line
        }
        // after every card, we should be at the ending ---
        if (currentIndex >= lines.size || lines[currentIndex] != "---") {
            throw IllegalArgumentException("Expected frontmatter to end with '---'")
        }
        
        val title = metadataMap["title"] ?: throw IllegalArgumentException("Missing required 'title' in frontmatter")
        val id = metadataMap["id"] ?: throw IllegalArgumentException("Missing required 'id' in frontmatter")
        val reversibleByDefault = metadataMap["reversibleByDefault"]?.toBoolean() ?: false

        return Pair(DeckMetadata(title, id, reversibleByDefault), currentIndex + 1) // return metadata and index after ---
    }
    
    private fun parseNotes(lines: List<String>, startIndex: Int): Pair<String?, Int> {
        var currentIndex = startIndex // index after frontmatter
        
        // Skip empty lines
        while (currentIndex < lines.size && lines[currentIndex].trim().isEmpty()) {
            currentIndex++
        }
        
        if (currentIndex >= lines.size) { // notes section not present
            return Pair(null, currentIndex)
        }
        
        // Check if we have a notes section
        if (lines[currentIndex].trim() == "# Notes") {
            currentIndex++
            val notesBuilder = StringBuilder() // accumulate notes in a StringBuilder
            
            // keep collecting notes until we hit the cards section
            while (currentIndex < lines.size && lines[currentIndex].trim() != "---cards---") {
                notesBuilder.appendLine(lines[currentIndex]) // add current line to notes
                currentIndex++
            }
            
            val notes = notesBuilder.toString().trim()
            return Pair(if (notes.isEmpty()) null else notes, currentIndex) // return notes or null
        }
        
        return Pair(null, currentIndex) // if we reach here, no notes section so null
    }
    
    private fun parseCards(lines: List<String>, startIndex: Int): List<ParsedCard> {
        var currentIndex = startIndex // index after notes
        
        // Find cards section
        while (currentIndex < lines.size && lines[currentIndex].trim() != "---cards---") {
            currentIndex++
        }
        
        if (currentIndex >= lines.size) {
            return emptyList() // if there's no cards section, return empty list
        }
        
        currentIndex++ // skip the ---cards--- line
        val cards = mutableListOf<ParsedCard>() // cards list to hold each parsed card
        
        // parsing starts here
        while (currentIndex < lines.size) {
            while (currentIndex < lines.size && lines[currentIndex].trim().isEmpty()) { // Skip empty lines
                currentIndex++
            }
            
            if (currentIndex >= lines.size) break // if no more lines, break
            
            val cardResult = parseCardBlock(lines, currentIndex) // parse this card block at the current index
            cards.add(cardResult.first) // then add the parsed card to the list
            currentIndex = cardResult.second // move index to after this card block
        }
        
        return cards
    }
    
    // parses each card block until the next '---' or end of file
    private fun parseCardBlock(lines: List<String>, startIndex: Int): Pair<ParsedCard, Int> {
        var currentIndex = startIndex // index at start of card block
        val cardMap = mutableMapOf<String, String>() // front-back of card
        
        while (currentIndex < lines.size) { // Parse key-value pairs for current card block
            val line = lines[currentIndex].trim()
            
            if (line == "---") { // end of this card block
                currentIndex++
                break
            }
            if (line.isEmpty()) {
                currentIndex++ // skip empty lines
                continue
            }
            
            val colonIndex = line.indexOf(':') // find key:value
            if (colonIndex > 0) { // if valid key:value line
                val key = line.substring(0, colonIndex).trim() // lhs is key
                val value = line.substring(colonIndex + 1).trim() // rhs is value
                
                if (value == "|") { // multi-line value
                    currentIndex++ // goto next line
                    val multiLineBuilder = StringBuilder() // accumulate multi-line value as one string
                    
                    // keep reading indented lines until we hit '---' or non-indented line
                    while (currentIndex < lines.size && lines[currentIndex].trim() != "---") {
                        val multiLine = lines[currentIndex] // get current muti-line
                        if (multiLine.startsWith("  ") || multiLine.startsWith("\t")) { // if indented
                            multiLineBuilder.appendLine(multiLine.substring(2)) // add line without indentation
                        } else if (multiLine.trim().isEmpty()) {
                            multiLineBuilder.appendLine() // add empty line
                        } else { // non-indented line means end of multi-line
                            break
                        }
                        currentIndex++ // goto next multi-line
                    }
                    
                    cardMap[key] = multiLineBuilder.toString().trim() // key gets the whole multi-line value
                } else {
                    cardMap[key] = value // not multi-line, just a single line value
                    currentIndex++
                }
            } else { // not a valid key:value line, skip it
                currentIndex++
            }
        }
        
        val id = cardMap["id"] ?: UUID.randomUUID().toString()
        val front = cardMap["front"] ?: throw IllegalArgumentException("Missing required 'front' field")
        val back = cardMap["back"] ?: throw IllegalArgumentException("Missing required 'back' field")
        val tags = cardMap["tags"]
        val pinned = cardMap["pinned"]
        val reversible = cardMap["reversible"]?.toBoolean() // null -> missing

        return Pair(ParsedCard(id, front, back, tags, pinned, reversible), currentIndex)
    }
    
    /**
     * Loads a deck from assets folder
     */
    suspend fun loadFromAssets(assetPath: String): ParsedDeck {
        val content = context.assets.open(assetPath).bufferedReader().use { it.readText() }
        val parsedDeck = parseMarkdown(content)
        
        // Import into database
        val deckEntity = DeckEntity(
            id = parsedDeck.metadata.id,
            name = parsedDeck.metadata.title,
            description = parsedDeck.notes,
            createdAt = System.currentTimeMillis(),
            updatedAt = System.currentTimeMillis()
        )
        deckDao.insertDeck(deckEntity)
        
        if (!parsedDeck.notes.isNullOrBlank()) {
            val noteEntity = NoteEntity(
                id = UUID.randomUUID().toString(),
                deckId = parsedDeck.metadata.id,
                markdownBody = parsedDeck.notes,
                updatedAt = System.currentTimeMillis()
            )
            noteDao.insertNote(noteEntity)
        }
        
        val cardEntities = parsedDeck.cards.map { card ->
            CardEntity(
                id = card.id,
                deckId = parsedDeck.metadata.id,
                front = card.front.trim(),
                back = card.back.trim(),
                tags = card.tags,
                pinned = card.pinned,
                isReversible = false,
                intervalDays = 1,
                easeFactor = 2.3,
                reviewCount = 0,
                consecutiveFails = 0,
                lastReviewedAt = 0L,
                nextDueAt = System.currentTimeMillis(),
                createdAt = System.currentTimeMillis()
            )
        }
        cardDao.insertCards(cardEntities)
        
        return parsedDeck
    }
}
