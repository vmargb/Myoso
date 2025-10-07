package com.vmargb.myoso.data

import android.content.Context
import java.io.File
import java.util.UUID

data class DeckMetadata(
    val title: String,
    val id: String
)

data class ParsedCard(
    val id: String,
    val front: String,
    val back: String,
    val tags: String? = null,
    val pinned: String? = null
)

data class ParsedDeck(
    val metadata: DeckMetadata,
    val notes: String?,
    val cards: List<ParsedCard>
)

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
        val parsedDeck = parseMarkdown(content)
        
        // Import deck metadata
        val deckEntity = DeckEntity(
            id = parsedDeck.metadata.id,
            name = parsedDeck.metadata.title,
            description = parsedDeck.notes,
            createdAt = System.currentTimeMillis(),
            updatedAt = System.currentTimeMillis()
        )
        deckDao.insertDeck(deckEntity)
        
        // Import notes if present
        if (!parsedDeck.notes.isNullOrBlank()) {
            val noteEntity = NoteEntity(
                id = UUID.randomUUID().toString(),
                deckId = parsedDeck.metadata.id,
                markdownBody = parsedDeck.notes,
                updatedAt = System.currentTimeMillis()
            )
            noteDao.insertNote(noteEntity)
        }
        
        // Import cards
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
        if (startIndex >= lines.size || lines[startIndex] != "---") {
            throw IllegalArgumentException("Expected frontmatter to start with '---'")
        }
        
        var currentIndex = startIndex + 1
        val metadataMap = mutableMapOf<String, String>()
        
        while (currentIndex < lines.size && lines[currentIndex] != "---") {
            val line = lines[currentIndex].trim()
            if (line.isNotEmpty()) {
                val colonIndex = line.indexOf(':')
                if (colonIndex > 0) {
                    val key = line.substring(0, colonIndex).trim()
                    val value = line.substring(colonIndex + 1).trim()
                    metadataMap[key] = value
                }
            }
            currentIndex++
        }
        
        if (currentIndex >= lines.size || lines[currentIndex] != "---") {
            throw IllegalArgumentException("Expected frontmatter to end with '---'")
        }
        
        val title = metadataMap["title"] ?: throw IllegalArgumentException("Missing required 'title' in frontmatter")
        val id = metadataMap["id"] ?: throw IllegalArgumentException("Missing required 'id' in frontmatter")
        
        return Pair(DeckMetadata(title, id), currentIndex + 1)
    }
    
    private fun parseNotes(lines: List<String>, startIndex: Int): Pair<String?, Int> {
        var currentIndex = startIndex
        
        // Skip empty lines
        while (currentIndex < lines.size && lines[currentIndex].trim().isEmpty()) {
            currentIndex++
        }
        
        if (currentIndex >= lines.size) {
            return Pair(null, currentIndex)
        }
        
        // Check if we have a notes section
        if (lines[currentIndex].trim() == "# Notes") {
            currentIndex++
            val notesBuilder = StringBuilder()
            
            // Collect notes until we hit the cards section
            while (currentIndex < lines.size && lines[currentIndex].trim() != "---cards---") {
                notesBuilder.appendLine(lines[currentIndex])
                currentIndex++
            }
            
            val notes = notesBuilder.toString().trim()
            return Pair(if (notes.isEmpty()) null else notes, currentIndex)
        }
        
        return Pair(null, currentIndex)
    }
    
    private fun parseCards(lines: List<String>, startIndex: Int): List<ParsedCard> {
        var currentIndex = startIndex
        
        // Find cards section
        while (currentIndex < lines.size && lines[currentIndex].trim() != "---cards---") {
            currentIndex++
        }
        
        if (currentIndex >= lines.size) {
            return emptyList()
        }
        
        currentIndex++ // Skip the ---cards--- line
        val cards = mutableListOf<ParsedCard>()
        
        while (currentIndex < lines.size) {
            // Skip empty lines
            while (currentIndex < lines.size && lines[currentIndex].trim().isEmpty()) {
                currentIndex++
            }
            
            if (currentIndex >= lines.size) break
            
            // Parse card block
            val cardResult = parseCardBlock(lines, currentIndex)
            cards.add(cardResult.first)
            currentIndex = cardResult.second
        }
        
        return cards
    }
    
    private fun parseCardBlock(lines: List<String>, startIndex: Int): Pair<ParsedCard, Int> {
        var currentIndex = startIndex
        val cardMap = mutableMapOf<String, String>()
        
        // Parse key-value pairs and multi-line fields
        while (currentIndex < lines.size) {
            val line = lines[currentIndex].trim()
            
            if (line == "---") {
                // End of card block
                currentIndex++
                break
            }
            
            if (line.isEmpty()) {
                currentIndex++
                continue
            }
            
            val colonIndex = line.indexOf(':')
            if (colonIndex > 0) {
                val key = line.substring(0, colonIndex).trim()
                val value = line.substring(colonIndex + 1).trim()
                
                if (value == "|") {
                    // Multi-line field
                    currentIndex++
                    val multiLineBuilder = StringBuilder()
                    
                    while (currentIndex < lines.size && lines[currentIndex].trim() != "---") {
                        val multiLine = lines[currentIndex]
                        if (multiLine.startsWith("  ") || multiLine.startsWith("\t")) {
                            multiLineBuilder.appendLine(multiLine.substring(2))
                        } else if (multiLine.trim().isEmpty()) {
                            multiLineBuilder.appendLine()
                        } else {
                            break
                        }
                        currentIndex++
                    }
                    
                    cardMap[key] = multiLineBuilder.toString().trim()
                } else {
                    cardMap[key] = value
                    currentIndex++
                }
            } else {
                currentIndex++
            }
        }
        
        val id = cardMap["id"] ?: UUID.randomUUID().toString()
        val front = cardMap["front"] ?: throw IllegalArgumentException("Missing required 'front' field")
        val back = cardMap["back"] ?: throw IllegalArgumentException("Missing required 'back' field")
        val tags = cardMap["tags"]
        val pinned = cardMap["pinned"]
        
        return Pair(ParsedCard(id, front, back, tags, pinned), currentIndex)
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
