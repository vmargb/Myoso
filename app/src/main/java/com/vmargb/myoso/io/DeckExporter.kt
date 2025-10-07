package com.vmargb.myoso.io

import com.vmargb.myoso.data.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.text.SimpleDateFormat
import java.util.*

class DeckExporter(
    private val deckDao: DeckDao,
    private val cardDao: CardDao,
    private val noteDao: NoteDao,
    private val citationDao: CitationDao
) {
    
    /**
     * Exports a single deck to markdown format
     * @param deckId The ID of the deck to export
     * @param outputFile The file to write the markdown to
     * @return Result indicating success or failure
     */
    suspend fun exportDeckToMarkdown(deckId: String, outputFile: File): Result<Unit> = withContext(Dispatchers.IO) {
        try {
            val deck = deckDao.getDeckById(deckId) ?: return@withContext Result.failure(
                IllegalArgumentException("Deck not found: $deckId")
            )
            
            val cards = cardDao.getCardsByDeck(deckId)
            val notes = noteDao.getNotesByDeck(deckId)
            
            val markdownContent = buildMarkdownContent(deck, cards, notes)
            outputFile.writeText(markdownContent)
            
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
    
    /**
     * Exports multiple decks to separate markdown files
     * @param deckIds List of deck IDs to export
     * @param outputDirectory Directory to write the files to
     * @return Result with list of exported files
     */
    suspend fun exportDecksToMarkdown(deckIds: List<String>, outputDirectory: File): Result<List<File>> = withContext(Dispatchers.IO) {
        try {
            val exportedFiles = mutableListOf<File>()
            
            deckIds.forEach { deckId ->
                val deck = deckDao.getDeckById(deckId) ?: return@forEach
                val fileName = generateDeckFilename(deck.name)
                val outputFile = File(outputDirectory, fileName)
                
                val result = exportDeckToMarkdown(deckId, outputFile)
                if (result.isSuccess) {
                    exportedFiles.add(outputFile)
                }
            }
            
            Result.success(exportedFiles)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
    
    /**
     * Builds the markdown content for a deck
     */
    private suspend fun buildMarkdownContent(
        deck: DeckEntity,
        cards: List<CardEntity>,
        notes: List<NoteEntity>
    ): String {
        val markdown = StringBuilder()
        
        // Frontmatter
        markdown.appendLine("---")
        markdown.appendLine("title: ${escapeYaml(deck.name)}")
        markdown.appendLine("id: ${deck.id}")
        markdown.appendLine("description: ${deck.description?.let { escapeYaml(it) } ?: ""}")
        markdown.appendLine("createdAt: ${deck.createdAt}")
        markdown.appendLine("updatedAt: ${deck.updatedAt}")
        markdown.appendLine("---")
        markdown.appendLine()
        
        // Notes section
        if (notes.isNotEmpty()) {
            markdown.appendLine("# Notes")
            markdown.appendLine()
            
            notes.forEach { note ->
                markdown.appendLine("## ${note.id}")
                markdown.appendLine()
                
                // Process notes with citations
                val processedNote = processNoteWithCitations(note)
                markdown.appendLine(processedNote)
                markdown.appendLine()
            }
        }
        
        // Cards section
        if (cards.isNotEmpty()) {
            markdown.appendLine("---cards---")
            markdown.appendLine()
            
            cards.forEach { card ->
                markdown.appendLine("id: ${card.id}")
                markdown.appendLine("front: |")
                markdown.appendLine("  ${card.front.replace("\n", "\n  ")}")
                markdown.appendLine("back: |")
                markdown.appendLine("  ${card.back.replace("\n", "\n  ")}")
                
                if (!card.tags.isNullOrBlank()) {
                    markdown.appendLine("tags: ${card.tags}")
                }
                
                if (!card.pinned.isNullOrBlank()) {
                    markdown.appendLine("pinned: ${card.pinned}")
                }
                
                markdown.appendLine("isReversible: ${card.isReversible}")
                markdown.appendLine("intervalDays: ${card.intervalDays}")
                markdown.appendLine("easeFactor: ${card.easeFactor}")
                markdown.appendLine("reviewCount: ${card.reviewCount}")
                markdown.appendLine("consecutiveFails: ${card.consecutiveFails}")
                markdown.appendLine("lastReviewedAt: ${card.lastReviewedAt}")
                markdown.appendLine("nextDueAt: ${card.nextDueAt}")
                markdown.appendLine("createdAt: ${card.createdAt}")
                markdown.appendLine("---")
                markdown.appendLine()
            }
        }
        
        return markdown.toString()
    }
    
    /**
     * Processes notes to include citation information
     */
    private suspend fun processNoteWithCitations(note: NoteEntity): String {
        val citations = citationDao.getCitationsByNote(note.id)
        var processedNote = note.markdownBody
        
        // Add citation metadata as comments
        if (citations.isNotEmpty()) {
            val citationComments = StringBuilder()
            citationComments.appendLine("<!-- Citations: ${citations.size} -->")
            
            citations.forEach { citation ->
                citationComments.appendLine("<!-- Citation: ${citation.cardId} at ${citation.startIndex}-${citation.endIndex} -->")
            }
            
            processedNote = citationComments.toString() + "\n" + processedNote
        }
        
        return processedNote
    }
    
    /**
     * Escapes YAML content to prevent parsing issues
     */
    private fun escapeYaml(text: String): String {
        return text
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
    }
    
    /**
     * Generates a filename for a deck
     */
    private fun generateDeckFilename(deckName: String): String {
        val sanitizedName = deckName
            .replace("[^a-zA-Z0-9\\s]".toRegex(), "")
            .replace("\\s+".toRegex(), "_")
            .lowercase()
        
        val timestamp = SimpleDateFormat("yyyyMMdd", Locale.getDefault()).format(Date())
        return "${sanitizedName}_${timestamp}.md"
    }
    
    /**
     * Gets export statistics for a deck
     */
    suspend fun getDeckExportInfo(deckId: String): DeckExportInfo? = withContext(Dispatchers.IO) {
        try {
            val deck = deckDao.getDeckById(deckId) ?: return@withContext null
            val cards = cardDao.getCardsByDeck(deckId)
            val notes = noteDao.getNotesByDeck(deckId)
            
            val totalCitations = notes.sumOf { note ->
                citationDao.getCitationCountByNote(note.id)
            }
            
            DeckExportInfo(
                deck = deck,
                cardCount = cards.size,
                noteCount = notes.size,
                citationCount = totalCitations,
                estimatedSize = estimateMarkdownSize(deck, cards, notes)
            )
        } catch (e: Exception) {
            null
        }
    }
    
    /**
     * Estimates the size of the markdown file
     */
    private fun estimateMarkdownSize(
        deck: DeckEntity,
        cards: List<CardEntity>,
        notes: List<NoteEntity>
    ): Long {
        var size = 0L
        
        // Frontmatter
        size += 200 // Approximate frontmatter size
        
        // Notes
        notes.forEach { note ->
            size += note.markdownBody.length + 100 // Extra for formatting
        }
        
        // Cards
        cards.forEach { card ->
            size += card.front.length + card.back.length + 200 // Extra for formatting
        }
        
        return size
    }
}

data class DeckExportInfo(
    val deck: DeckEntity,
    val cardCount: Int,
    val noteCount: Int,
    val citationCount: Int,
    val estimatedSize: Long
) {
    val formattedSize: String
        get() = when {
            estimatedSize < 1024 -> "$estimatedSize B"
            estimatedSize < 1024 * 1024 -> "${estimatedSize / 1024} KB"
            else -> "${estimatedSize / (1024 * 1024)} MB"
        }
}
