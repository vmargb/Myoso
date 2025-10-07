package com.vmargb.myoso.data

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.Assert.*

@RunWith(AndroidJUnit4::class)
class DeckMarkdownParserTest {
    
    private lateinit var database: AppDatabase
    private lateinit var cardDao: CardDao
    private lateinit var deckDao: DeckDao
    private lateinit var noteDao: NoteDao
    private lateinit var parser: DeckMarkdownParser
    private lateinit var context: Context
    
    @Before
    fun setup() {
        context = ApplicationProvider.getApplicationContext()
        database = Room.inMemoryDatabaseBuilder(
            context,
            AppDatabase::class.java
        ).allowMainThreadQueries().build()
        
        cardDao = database.cardDao()
        deckDao = database.deckDao()
        noteDao = database.noteDao()
        parser = DeckMarkdownParser(context, cardDao, deckDao, noteDao)
    }
    
    @After
    fun teardown() {
        database.close()
    }
    
    @Test
    fun `parseMarkdown should correctly parse deck metadata and cards`() = runBlocking {
        val markdownContent = """
            ---
            title: Test Deck
            id: test-deck
            ---
            # Notes
            This is a test deck with sample cards.
            
            ---cards---
            id: card-1
            front: |
              What is the capital of France?
            back: |
              Paris
            tags: geography,capital
            pinned: daily
            ---
            id: card-2
            front: |
              What is 2 + 2?
            back: |
              4
            tags: math,basic
            ---
        """.trimIndent()
        
        val parsedDeck = parser.parseMarkdown(markdownContent)
        
        // Verify metadata
        assertEquals("Test Deck", parsedDeck.metadata.title)
        assertEquals("test-deck", parsedDeck.metadata.id)
        
        // Verify notes
        assertTrue(parsedDeck.notes!!.contains("This is a test deck"))
        
        // Verify cards
        assertEquals(2, parsedDeck.cards.size)
        
        val card1 = parsedDeck.cards[0]
        assertEquals("card-1", card1.id)
        assertEquals("What is the capital of France?", card1.front.trim())
        assertEquals("Paris", card1.back.trim())
        assertEquals("geography,capital", card1.tags)
        assertEquals("daily", card1.pinned)
        
        val card2 = parsedDeck.cards[1]
        assertEquals("card-2", card2.id)
        assertEquals("What is 2 + 2?", card2.front.trim())
        assertEquals("4", card2.back.trim())
        assertEquals("math,basic", card2.tags)
        assertNull(card2.pinned)
    }
    
    @Test
    fun `loadFromAssets should import sample deck correctly`() = runBlocking {
        val parsedDeck = parser.loadFromAssets("decks/sample.md")
        
        // Verify deck was created
        val deck = deckDao.getDeckById("spanish-basics")
        assertNotNull(deck)
        assertEquals("Spanish Basics", deck!!.name)
        assertTrue(deck.description!!.contains("essential Spanish vocabulary"))
        
        // Verify notes were created
        val notes = noteDao.getNotesByDeck("spanish-basics")
        assertEquals(1, notes.size)
        assertTrue(notes[0].markdownBody.contains("essential Spanish vocabulary"))
        
        // Verify cards were created
        val cards = cardDao.getCardsByDeck("spanish-basics")
        assertEquals(10, cards.size)
        
        // Check specific cards
        val holaCard = cards.find { it.id == "uuid-1" }
        assertNotNull(holaCard)
        assertEquals("Hola", holaCard!!.front)
        assertEquals("Hello", holaCard.back)
        assertEquals("greeting,common", holaCard.tags)
        assertEquals("daily", holaCard.pinned)
        
        val adiosCard = cards.find { it.id == "uuid-2" }
        assertNotNull(adiosCard)
        assertEquals("Adiós", adiosCard!!.front)
        assertEquals("Goodbye", adiosCard.back)
        assertEquals("greeting", adiosCard.tags)
        assertNull(adiosCard.pinned)
        
        // Verify pinned cards
        val pinnedCards = cardDao.getPinnedCards("daily")
        assertEquals(3, pinnedCards.size) // hola, gracias, and one more
    }
    
    @Test
    fun `parseMarkdown should handle deck without notes section`() = runBlocking {
        val markdownContent = """
            ---
            title: Simple Deck
            id: simple-deck
            ---
            ---cards---
            id: simple-card
            front: |
              Question?
            back: |
              Answer
            ---
        """.trimIndent()
        
        val parsedDeck = parser.parseMarkdown(markdownContent)
        
        assertEquals("Simple Deck", parsedDeck.metadata.title)
        assertNull(parsedDeck.notes)
        assertEquals(1, parsedDeck.cards.size)
        assertEquals("Question?", parsedDeck.cards[0].front.trim())
        assertEquals("Answer", parsedDeck.cards[0].back.trim())
    }
    
    @Test
    fun `parseMarkdown should handle deck without cards section`() = runBlocking {
        val markdownContent = """
            ---
            title: Notes Only Deck
            id: notes-only
            ---
            # Notes
            This deck only has notes, no cards.
        """.trimIndent()
        
        val parsedDeck = parser.parseMarkdown(markdownContent)
        
        assertEquals("Notes Only Deck", parsedDeck.metadata.title)
        assertTrue(parsedDeck.notes!!.contains("This deck only has notes"))
        assertEquals(0, parsedDeck.cards.size)
    }
    
    @Test
    fun `parseMarkdown should generate UUID for cards without id`() = runBlocking {
        val markdownContent = """
            ---
            title: Auto ID Deck
            id: auto-id-deck
            ---
            ---cards---
            front: |
              Question without ID?
            back: |
              Answer without ID
            ---
        """.trimIndent()
        
        val parsedDeck = parser.parseMarkdown(markdownContent)
        
        assertEquals(1, parsedDeck.cards.size)
        val card = parsedDeck.cards[0]
        assertNotNull(card.id)
        assertTrue(card.id.isNotEmpty())
        assertEquals("Question without ID?", card.front.trim())
        assertEquals("Answer without ID", card.back.trim())
    }
}
