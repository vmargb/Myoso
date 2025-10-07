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
class ReviewHistoryDaoTest {
    
    private lateinit var database: AppDatabase
    private lateinit var reviewHistoryDao: ReviewHistoryDao
    private lateinit var cardDao: CardDao
    private lateinit var deckDao: DeckDao
    
    @Before
    fun setup() {
        database = Room.inMemoryDatabaseBuilder(
            ApplicationProvider.getApplicationContext(),
            AppDatabase::class.java
        ).allowMainThreadQueries().build()
        
        reviewHistoryDao = database.reviewHistoryDao()
        cardDao = database.cardDao()
        deckDao = database.deckDao()
    }
    
    @After
    fun teardown() {
        database.close()
    }
    
    @Test
    fun `insert and get review history by card`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card = createTestCard("card1", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCard(card)
        
        val reviewHistory = createTestReviewHistory("review1", "card1")
        reviewHistoryDao.insertReviewHistory(reviewHistory)
        
        val retrievedReviews = reviewHistoryDao.getReviewHistoryByCard("card1")
        assertEquals(1, retrievedReviews.size)
        assertEquals("review1", retrievedReviews[0].id)
        assertEquals("card1", retrievedReviews[0].cardId)
        assertEquals("GOOD", retrievedReviews[0].confidence)
    }
    
    @Test
    fun `get multiple review histories for same card`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card = createTestCard("card1", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCard(card)
        
        val review1 = createTestReviewHistory("review1", "card1", confidence = "AGAIN")
        val review2 = createTestReviewHistory("review2", "card1", confidence = "GOOD")
        val review3 = createTestReviewHistory("review3", "card1", confidence = "EASY")
        
        reviewHistoryDao.insertReviewHistories(listOf(review1, review2, review3))
        
        val retrievedReviews = reviewHistoryDao.getReviewHistoryByCard("card1")
        assertEquals(3, retrievedReviews.size)
        
        // Should be ordered by reviewedAt DESC
        assertEquals("review3", retrievedReviews[0].id)
        assertEquals("review2", retrievedReviews[1].id)
        assertEquals("review1", retrievedReviews[2].id)
    }
    
    @Test
    fun `get recent reviews`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card1 = createTestCard("card1", "deck1")
        val card2 = createTestCard("card2", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCards(listOf(card1, card2))
        
        val review1 = createTestReviewHistory("review1", "card1", reviewedAt = System.currentTimeMillis() - 1000)
        val review2 = createTestReviewHistory("review2", "card2", reviewedAt = System.currentTimeMillis() - 500)
        val review3 = createTestReviewHistory("review3", "card1", reviewedAt = System.currentTimeMillis())
        
        reviewHistoryDao.insertReviewHistories(listOf(review1, review2, review3))
        
        val recentReviews = reviewHistoryDao.getRecentReviews(2)
        assertEquals(2, recentReviews.size)
        
        // Should be ordered by reviewedAt DESC
        assertEquals("review3", recentReviews[0].id)
        assertEquals("review2", recentReviews[1].id)
    }
    
    @Test
    fun `get reviews in time range`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card = createTestCard("card1", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCard(card)
        
        val now = System.currentTimeMillis()
        val review1 = createTestReviewHistory("review1", "card1", reviewedAt = now - 2000)
        val review2 = createTestReviewHistory("review2", "card1", reviewedAt = now - 1000)
        val review3 = createTestReviewHistory("review3", "card1", reviewedAt = now + 1000)
        
        reviewHistoryDao.insertReviewHistories(listOf(review1, review2, review3))
        
        val reviewsInRange = reviewHistoryDao.getReviewsInTimeRange(now - 1500, now + 500)
        assertEquals(1, reviewsInRange.size)
        assertEquals("review2", reviewsInRange[0].id)
    }
    
    @Test
    fun `delete review history`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card = createTestCard("card1", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCard(card)
        
        val reviewHistory = createTestReviewHistory("review1", "card1")
        reviewHistoryDao.insertReviewHistory(reviewHistory)
        
        reviewHistoryDao.deleteReviewHistory(reviewHistory)
        
        val retrievedReviews = reviewHistoryDao.getReviewHistoryByCard("card1")
        assertEquals(0, retrievedReviews.size)
    }
    
    @Test
    fun `delete review history by card`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card1 = createTestCard("card1", "deck1")
        val card2 = createTestCard("card2", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCards(listOf(card1, card2))
        
        val review1 = createTestReviewHistory("review1", "card1")
        val review2 = createTestReviewHistory("review2", "card1")
        val review3 = createTestReviewHistory("review3", "card2")
        
        reviewHistoryDao.insertReviewHistories(listOf(review1, review2, review3))
        
        reviewHistoryDao.deleteReviewHistoryByCard("card1")
        
        val card1Reviews = reviewHistoryDao.getReviewHistoryByCard("card1")
        val card2Reviews = reviewHistoryDao.getReviewHistoryByCard("card2")
        
        assertEquals(0, card1Reviews.size)
        assertEquals(1, card2Reviews.size)
        assertEquals("review3", card2Reviews[0].id)
    }
    
    @Test
    fun `get review count by card`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card = createTestCard("card1", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCard(card)
        
        val reviews = listOf(
            createTestReviewHistory("review1", "card1"),
            createTestReviewHistory("review2", "card1"),
            createTestReviewHistory("review3", "card1")
        )
        reviewHistoryDao.insertReviewHistories(reviews)
        
        val count = reviewHistoryDao.getReviewCountByCard("card1")
        assertEquals(3, count)
    }
    
    @Test
    fun `get average response time by card`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card = createTestCard("card1", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCard(card)
        
        val review1 = createTestReviewHistory("review1", "card1", responseTimeMs = 5000)
        val review2 = createTestReviewHistory("review2", "card1", responseTimeMs = 7000)
        val review3 = createTestReviewHistory("review3", "card1", responseTimeMs = 3000)
        
        reviewHistoryDao.insertReviewHistories(listOf(review1, review2, review3))
        
        val averageResponseTime = reviewHistoryDao.getAverageResponseTimeByCard("card1")
        assertNotNull(averageResponseTime)
        assertEquals(5000.0, averageResponseTime!!, 0.1) // (5000 + 7000 + 3000) / 3 = 5000
    }
    
    @Test
    fun `get review history by card flow`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card = createTestCard("card1", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCard(card)
        
        val reviewHistory = createTestReviewHistory("review1", "card1")
        reviewHistoryDao.insertReviewHistory(reviewHistory)
        
        val flow = reviewHistoryDao.getReviewHistoryByCardFlow("card1")
        val reviews = flow.first()
        
        assertEquals(1, reviews.size)
        assertEquals("review1", reviews[0].id)
    }
    
    @Test
    fun `delete all review history`() = runBlocking {
        val deck = createTestDeck("deck1")
        val card1 = createTestCard("card1", "deck1")
        val card2 = createTestCard("card2", "deck1")
        deckDao.insertDeck(deck)
        cardDao.insertCards(listOf(card1, card2))
        
        val review1 = createTestReviewHistory("review1", "card1")
        val review2 = createTestReviewHistory("review2", "card2")
        
        reviewHistoryDao.insertReviewHistories(listOf(review1, review2))
        
        reviewHistoryDao.deleteAllReviewHistory()
        
        val count1 = reviewHistoryDao.getReviewCountByCard("card1")
        val count2 = reviewHistoryDao.getReviewCountByCard("card2")
        
        assertEquals(0, count1)
        assertEquals(0, count2)
    }
    
    private fun createTestReviewHistory(
        id: String,
        cardId: String,
        confidence: String = "GOOD",
        responseTimeMs: Long = 5000,
        reviewedAt: Long = System.currentTimeMillis()
    ) = ReviewHistoryEntity(
        id = id,
        cardId = cardId,
        reviewedAt = reviewedAt,
        confidence = confidence,
        responseTimeMs = responseTimeMs,
        oldIntervalDays = 1,
        newIntervalDays = 3
    )
    
    private fun createTestCard(id: String, deckId: String) = CardEntity(
        id = id,
        deckId = deckId,
        front = "Test Front",
        back = "Test Back",
        isReversible = false,
        intervalDays = 1,
        easeFactor = 2.3,
        reviewCount = 0,
        consecutiveFails = 0,
        lastReviewedAt = 0L,
        nextDueAt = System.currentTimeMillis(),
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
