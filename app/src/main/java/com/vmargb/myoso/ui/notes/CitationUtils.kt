package com.vmargb.myoso.ui.notes

import com.vmargb.myoso.data.CitationEntity
import java.util.UUID
import java.util.regex.Pattern

object CitationUtils {
    
    private val CITATION_PATTERN = Pattern.compile("\\[\\[card:([a-f0-9-]+)\\]\\]")
    
    /**
     * Scans markdown text for citation markers and creates CitationEntity records
     * @param noteId The ID of the note containing the citations
     * @param markdownText The markdown text to scan
     * @return List of CitationEntity objects found in the text
     */
    fun extractCitations(noteId: String, markdownText: String): List<CitationEntity> {
        val citations = mutableListOf<CitationEntity>()
        val matcher = CITATION_PATTERN.matcher(markdownText)
        
        while (matcher.find()) {
            val cardId = matcher.group(1)
            val startIndex = matcher.start()
            val endIndex = matcher.end()
            
            // Extract anchor text (text before the citation)
            val anchorText = extractAnchorText(markdownText, startIndex)
            
            val citation = CitationEntity(
                id = UUID.randomUUID().toString(),
                noteId = noteId,
                cardId = cardId,
                anchorText = anchorText,
                startIndex = startIndex,
                endIndex = endIndex
            )
            citations.add(citation)
        }
        
        return citations
    }
    
    /**
     * Extracts anchor text before a citation marker
     * Looks for the nearest sentence or phrase before the citation
     */
    private fun extractAnchorText(text: String, citationStart: Int): String {
        if (citationStart <= 0) return ""
        
        // Look backwards for anchor text
        var start = citationStart - 1
        while (start >= 0 && (text[start].isLetterOrDigit() || text[start] == ' ')) {
            start--
        }
        start++
        
        // Extract up to 50 characters before the citation
        val anchorStart = maxOf(0, citationStart - 50)
        val anchorText = text.substring(anchorStart, citationStart).trim()
        
        return if (anchorText.length > 30) {
            "..." + anchorText.takeLast(30)
        } else {
            anchorText
        }
    }
    
    /**
     * Inserts a citation marker at the specified position
     * @param text Current markdown text
     * @param cardId ID of the card to cite
     * @param selectionStart Start of text selection
     * @param selectionEnd End of text selection
     * @return Updated text with citation marker
     */
    fun insertCitation(
        text: String,
        cardId: String,
        selectionStart: Int,
        selectionEnd: Int
    ): String {
        val citationMarker = "[[card:$cardId]]"
        
        return if (selectionStart == selectionEnd) {
            // No selection, insert after cursor
            text.substring(0, selectionStart) + citationMarker + text.substring(selectionStart)
        } else {
            // Wrap selection with citation
            text.substring(0, selectionStart) + citationMarker + 
            text.substring(selectionStart, selectionEnd) + citationMarker + 
            text.substring(selectionEnd)
        }
    }
    
    /**
     * Finds all citation markers in text and returns their positions
     * @param text Markdown text to scan
     * @return List of CitationMatch objects with positions and card IDs
     */
    fun findCitationMatches(text: String): List<CitationMatch> {
        val matches = mutableListOf<CitationMatch>()
        val matcher = CITATION_PATTERN.matcher(text)
        
        while (matcher.find()) {
            val cardId = matcher.group(1)
            val start = matcher.start()
            val end = matcher.end()
            
            matches.add(CitationMatch(cardId, start, end))
        }
        
        return matches
    }
    
    /**
     * Removes a citation marker from text
     * @param text Current markdown text
     * @param citationStart Start position of citation marker
     * @param citationEnd End position of citation marker
     * @return Updated text with citation removed
     */
    fun removeCitation(text: String, citationStart: Int, citationEnd: Int): String {
        return text.substring(0, citationStart) + text.substring(citationEnd)
    }
    
    /**
     * Validates that a citation marker is properly formatted
     */
    fun isValidCitation(citationText: String): Boolean {
        return CITATION_PATTERN.matcher(citationText).matches()
    }
    
    /**
     * Extracts card ID from a citation marker
     */
    fun extractCardId(citationText: String): String? {
        val matcher = CITATION_PATTERN.matcher(citationText)
        return if (matcher.matches()) matcher.group(1) else null
    }
}

data class CitationMatch(
    val cardId: String,
    val start: Int,
    val end: Int
)
