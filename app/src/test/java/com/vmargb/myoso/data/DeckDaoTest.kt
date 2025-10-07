package com.vmargb.myoso.data

import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.Assert.*

@RunWith(AndroidJUnit4::class)
class DeckDaoTest {
    
    private lateinit var database: AppDatabase
    private lateinit var deckDao: DeckDao
    
    @Before
    fun setup() {
        database = Room.inMemoryDatabaseBuilder(
            ApplicationProvider.getApplicationContext(),
            AppDatabase::class.java
        ).allowMainThreadQueries().build()
        
        deckDao = database.deckDao()
    }
    
    @After
    fun teardown() {
        database.close()
    }
    
    @Test
    fun `insert and get deck by id`() = runBlocking {
        val deck = createTestDeck("deck1", "Spanish Basics", "Basic Spanish vocabulary")
        deckDao.insertDeck(deck)
        
        val retrievedDeck = deckDao.getDeckById("deck1")
        assertNotNull(retrievedDeck)
        assertEquals("deck1", retrievedDeck!!.id)
        assertEquals("Spanish Basics", retrievedDeck.name)
        assertEquals("Basic Spanish vocabulary", retrievedDeck.description)
    }
    
    @Test
    fun `get all decks`() = runBlocking {
        val deck1 = createTestDeck("deck1", "Spanish Basics")
        val deck2 = createTestDeck("deck2", "French Basics")
        val deck3 = createTestDeck("deck3", "German Basics")
        
        deckDao.insertDecks(listOf(deck1, deck2, deck3))
        
        val allDecks = deckDao.getAllDecks()
        assertEquals(3, allDecks.size)
        assertTrue(allDecks.any { it.id == "deck1" })
        assertTrue(allDecks.any { it.id == "deck2" })
        assertTrue(allDecks.any { it.id == "deck3" })
    }
    
    @Test
    fun `search decks by name`() = runBlocking {
        val deck1 = createTestDeck("deck1", "Spanish Basics")
        val deck2 = createTestDeck("deck2", "Spanish Advanced")
        val deck3 = createTestDeck("deck3", "French Basics")
        
        deckDao.insertDecks(listOf(deck1, deck2, deck3))
        
        val spanishDecks = deckDao.searchDecks("Spanish")
        assertEquals(2, spanishDecks.size)
        assertTrue(spanishDecks.any { it.id == "deck1" })
        assertTrue(spanishDecks.any { it.id == "deck2" })
        assertFalse(spanishDecks.any { it.id == "deck3" })
    }
    
    @Test
    fun `search decks case insensitive`() = runBlocking {
        val deck1 = createTestDeck("deck1", "Spanish Basics")
        val deck2 = createTestDeck("deck2", "spanish advanced")
        
        deckDao.insertDecks(listOf(deck1, deck2))
        
        val spanishDecks = deckDao.searchDecks("spanish")
        assertEquals(2, spanishDecks.size)
    }
    
    @Test
    fun `update deck`() = runBlocking {
        val deck = createTestDeck("deck1", "Spanish Basics", "Basic vocabulary")
        deckDao.insertDeck(deck)
        
        val updatedDeck = deck.copy(
            name = "Spanish Fundamentals",
            description = "Updated description",
            updatedAt = System.currentTimeMillis()
        )
        
        deckDao.updateDeck(updatedDeck)
        
        val retrievedDeck = deckDao.getDeckById("deck1")
        assertNotNull(retrievedDeck)
        assertEquals("Spanish Fundamentals", retrievedDeck!!.name)
        assertEquals("Updated description", retrievedDeck.description)
    }
    
    @Test
    fun `delete deck`() = runBlocking {
        val deck = createTestDeck("deck1", "Spanish Basics")
        deckDao.insertDeck(deck)
        
        deckDao.deleteDeck(deck)
        
        val retrievedDeck = deckDao.getDeckById("deck1")
        assertNull(retrievedDeck)
    }
    
    @Test
    fun `delete deck by id`() = runBlocking {
        val deck = createTestDeck("deck1", "Spanish Basics")
        deckDao.insertDeck(deck)
        
        deckDao.deleteDeckById("deck1")
        
        val retrievedDeck = deckDao.getDeckById("deck1")
        assertNull(retrievedDeck)
    }
    
    @Test
    fun `get deck count`() = runBlocking {
        val deck1 = createTestDeck("deck1", "Spanish Basics")
        val deck2 = createTestDeck("deck2", "French Basics")
        val deck3 = createTestDeck("deck3", "German Basics")
        
        deckDao.insertDecks(listOf(deck1, deck2, deck3))
        
        val count = deckDao.getDeckCount()
        assertEquals(3, count)
    }
    
    @Test
    fun `get all decks flow`() = runBlocking {
        val deck = createTestDeck("deck1", "Spanish Basics")
        deckDao.insertDeck(deck)
        
        val flow = deckDao.getAllDecksFlow()
        val decks = flow.first()
        
        assertEquals(1, decks.size)
        assertEquals("deck1", decks[0].id)
    }
    
    @Test
    fun `get deck by id flow`() = runBlocking {
        val deck = createTestDeck("deck1", "Spanish Basics")
        deckDao.insertDeck(deck)
        
        val flow = deckDao.getDeckByIdFlow("deck1")
        val retrievedDeck = flow.first()
        
        assertNotNull(retrievedDeck)
        assertEquals("deck1", retrievedDeck!!.id)
        assertEquals("Spanish Basics", retrievedDeck.name)
    }
    
    @Test
    fun `insert deck with same id replaces existing`() = runBlocking {
        val deck1 = createTestDeck("deck1", "Spanish Basics", "Original description")
        val deck2 = createTestDeck("deck1", "Spanish Fundamentals", "Updated description")
        
        deckDao.insertDeck(deck1)
        deckDao.insertDeck(deck2) // Should replace deck1
        
        val retrievedDeck = deckDao.getDeckById("deck1")
        assertNotNull(retrievedDeck)
        assertEquals("Spanish Fundamentals", retrievedDeck!!.name)
        assertEquals("Updated description", retrievedDeck.description)
    }
    
    @Test
    fun `delete all decks`() = runBlocking {
        val deck1 = createTestDeck("deck1", "Spanish Basics")
        val deck2 = createTestDeck("deck2", "French Basics")
        
        deckDao.insertDecks(listOf(deck1, deck2))
        
        deckDao.deleteAllDecks()
        
        val count = deckDao.getDeckCount()
        assertEquals(0, count)
    }
    
    private fun createTestDeck(
        id: String,
        name: String,
        description: String? = null
    ) = DeckEntity(
        id = id,
        name = name,
        description = description,
        createdAt = System.currentTimeMillis(),
        updatedAt = System.currentTimeMillis()
    )
}
