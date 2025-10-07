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
class CardDaoTest {
    
    private lateinit var database: AppDatabase
    private lateinit var cardDao: CardDao
    private lateinit var deckDao: DeckDao
    
    @Before
    fun setup() {
        database = Room.inMemoryDatabaseBuilder(
            ApplicationProvider.getApplicationContext(),
            AppDatabase::class.java
        ).allowMainThreadQueries().build()
        
        cardDao = database.cardDao()
        deckDao = database.deckDao()
    }
    
    @After
    fun teardown() {
        database.close()
    }
    
    @Test
    fun `insert and get card by id`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val card = createTestCard("card1", "deck1")
        cardDao.insertCard(card)
        
        val retrievedCard = cardDao.getCardById("card1")
        assertNotNull(retrievedCard)
        assertEquals("card1", retrievedCard!!.id)
        assertEquals("deck1", retrievedCard.deckId)
        assertEquals("Test Front", retrievedCard.front)
        assertEquals("Test Back", retrievedCard.back)
    }
    
    @Test
    fun `get cards by deck`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val card1 = createTestCard("card1", "deck1")
        val card2 = createTestCard("card2", "deck1")
        val card3 = createTestCard("card3", "deck2") // Different deck
        
        cardDao.insertCards(listOf(card1, card2, card3))
        
        val deckCards = cardDao.getCardsByDeck("deck1")
        assertEquals(2, deckCards.size)
        assertTrue(deckCards.any { it.id == "card1" })
        assertTrue(deckCards.any { it.id == "card2" })
        assertFalse(deckCards.any { it.id == "card3" })
    }
    
    @Test
    fun `get due cards`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val now = System.currentTimeMillis()
        val pastTime = now - 86400000L // 1 day ago
        val futureTime = now + 86400000L // 1 day from now
        
        val dueCard = createTestCard("due1", "deck1", nextDueAt = pastTime)
        val notDueCard = createTestCard("notDue1", "deck1", nextDueAt = futureTime)
        
        cardDao.insertCards(listOf(dueCard, notDueCard))
        
        val dueCards = cardDao.getDueCards(listOf("deck1"), now)
        assertEquals(1, dueCards.size)
        assertEquals("due1", dueCards[0].id)
    }
    
    @Test
    fun `get due cards from multiple decks`() = runBlocking {
        val deck1 = createTestDeck("deck1")
        val deck2 = createTestDeck("deck2")
        deckDao.insertDecks(listOf(deck1, deck2))
        
        val now = System.currentTimeMillis()
        val pastTime = now - 86400000L
        
        val card1 = createTestCard("card1", "deck1", nextDueAt = pastTime)
        val card2 = createTestCard("card2", "deck2", nextDueAt = pastTime)
        val card3 = createTestCard("card3", "deck1", nextDueAt = now + 86400000L)
        
        cardDao.insertCards(listOf(card1, card2, card3))
        
        val dueCards = cardDao.getDueCards(listOf("deck1", "deck2"), now)
        assertEquals(2, dueCards.size)
        assertTrue(dueCards.any { it.id == "card1" })
        assertTrue(dueCards.any { it.id == "card2" })
        assertFalse(dueCards.any { it.id == "card3" })
    }
    
    @Test
    fun `get pinned cards`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val dailyCard = createTestCard("daily1", "deck1", pinned = "daily")
        val weeklyCard = createTestCard("weekly1", "deck1", pinned = "weekly")
        val unpinnedCard = createTestCard("unpinned1", "deck1", pinned = null)
        
        cardDao.insertCards(listOf(dailyCard, weeklyCard, unpinnedCard))
        
        val dailyCards = cardDao.getPinnedCards("daily")
        assertEquals(1, dailyCards.size)
        assertEquals("daily1", dailyCards[0].id)
        
        val weeklyCards = cardDao.getPinnedCards("weekly")
        assertEquals(1, weeklyCards.size)
        assertEquals("weekly1", weeklyCards[0].id)
    }
    
    @Test
    fun `get cards by tag`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val taggedCard1 = createTestCard("tagged1", "deck1", tags = "important,urgent")
        val taggedCard2 = createTestCard("tagged2", "deck1", tags = "important")
        val untaggedCard = createTestCard("untagged1", "deck1", tags = "other")
        
        cardDao.insertCards(listOf(taggedCard1, taggedCard2, untaggedCard))
        
        val importantCards = cardDao.getCardsByTag("important")
        assertEquals(2, importantCards.size)
        assertTrue(importantCards.any { it.id == "tagged1" })
        assertTrue(importantCards.any { it.id == "tagged2" })
        assertFalse(importantCards.any { it.id == "untagged1" })
    }
    
    @Test
    fun `update card after review`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val card = createTestCard("card1", "deck1", reviewCount = 0)
        cardDao.insertCard(card)
        
        val updatedCard = card.copy(
            reviewCount = 1,
            intervalDays = 3,
            lastReviewedAt = System.currentTimeMillis(),
            nextDueAt = System.currentTimeMillis() + 259200000L // 3 days
        )
        
        cardDao.updateCardAfterReview(updatedCard)
        
        val retrievedCard = cardDao.getCardById("card1")
        assertNotNull(retrievedCard)
        assertEquals(1, retrievedCard!!.reviewCount)
        assertEquals(3L, retrievedCard.intervalDays)
    }
    
    @Test
    fun `delete card`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val card = createTestCard("card1", "deck1")
        cardDao.insertCard(card)
        
        cardDao.deleteCard(card)
        
        val retrievedCard = cardDao.getCardById("card1")
        assertNull(retrievedCard)
    }
    
    @Test
    fun `delete cards by deck`() = runBlocking {
        val deck1 = createTestDeck("deck1")
        val deck2 = createTestDeck("deck2")
        deckDao.insertDecks(listOf(deck1, deck2))
        
        val card1 = createTestCard("card1", "deck1")
        val card2 = createTestCard("card2", "deck1")
        val card3 = createTestCard("card3", "deck2")
        
        cardDao.insertCards(listOf(card1, card2, card3))
        
        cardDao.deleteCardsByDeck("deck1")
        
        val deck1Cards = cardDao.getCardsByDeck("deck1")
        val deck2Cards = cardDao.getCardsByDeck("deck2")
        
        assertEquals(0, deck1Cards.size)
        assertEquals(1, deck2Cards.size)
        assertEquals("card3", deck2Cards[0].id)
    }
    
    @Test
    fun `get card count by deck`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val cards = listOf(
            createTestCard("card1", "deck1"),
            createTestCard("card2", "deck1"),
            createTestCard("card3", "deck1")
        )
        cardDao.insertCards(cards)
        
        val count = cardDao.getCardCountByDeck("deck1")
        assertEquals(3, count)
    }
    
    @Test
    fun `get due card count`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val now = System.currentTimeMillis()
        val pastTime = now - 86400000L
        val futureTime = now + 86400000L
        
        val cards = listOf(
            createTestCard("card1", "deck1", nextDueAt = pastTime),
            createTestCard("card2", "deck1", nextDueAt = pastTime),
            createTestCard("card3", "deck1", nextDueAt = futureTime)
        )
        cardDao.insertCards(cards)
        
        val dueCount = cardDao.getDueCardCount(listOf("deck1"), now)
        assertEquals(2, dueCount)
    }
    
    @Test
    fun `cards by deck flow`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val card = createTestCard("card1", "deck1")
        cardDao.insertCard(card)
        
        val flow = cardDao.getCardsByDeckFlow("deck1")
        val cards = flow.first()
        
        assertEquals(1, cards.size)
        assertEquals("card1", cards[0].id)
    }
    
    private fun createTestCard(
        id: String,
        deckId: String,
        front: String = "Test Front",
        back: String = "Test Back",
        tags: String? = null,
        pinned: String? = null,
        nextDueAt: Long = System.currentTimeMillis()
    ) = CardEntity(
        id = id,
        deckId = deckId,
        front = front,
        back = back,
        tags = tags,
        pinned = pinned,
        isReversible = false,
        intervalDays = 1,
        easeFactor = 2.3,
        reviewCount = 0,
        consecutiveFails = 0,
        lastReviewedAt = 0L,
        nextDueAt = nextDueAt,
        createdAt = System.currentTimeMillis()
    )
    
    private fun createTestDeck(id: String) = DeckEntity(
        id = id,
        name = "Test Deck $id",
        description = "Test description",
        createdAt = System.currentTimeMillis(),
        updatedAt = System.currentTimeMillis()
    )
}
