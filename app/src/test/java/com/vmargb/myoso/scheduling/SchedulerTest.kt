package com.vmargb.myoso.scheduling

import com.vmargb.myoso.data.CardEntity
import org.junit.Test
import org.junit.Assert.*
import kotlin.math.abs

class SchedulerTest {
    
    private fun createTestCard(
        id: String = "test-card",
        deckId: String = "test-deck",
        front: String = "Test Front",
        back: String = "Test Back",
        intervalDays: Long = 1,
        easeFactor: Double = 2.3,
        reviewCount: Int = 0,
        consecutiveFails: Int = 0,
        lastReviewedAt: Long = 0L,
        nextDueAt: Long = 0L
    ) = CardEntity(
        id = id,
        deckId = deckId,
        front = front,
        back = back,
        intervalDays = intervalDays,
        easeFactor = easeFactor,
        reviewCount = reviewCount,
        consecutiveFails = consecutiveFails,
        lastReviewedAt = lastReviewedAt,
        nextDueAt = nextDueAt
    )
    
    @Test
    fun `new card marked KNEW should progress from 1 day to 3 days`() {
        // First review of new card
        val newCard = createTestCard(reviewCount = 0, intervalDays = 1)
        val result = ReviewResult(Confidence.KNEW, 8000L) // 8 seconds response
        
        val (updatedCard, nextDueAt) = Scheduler.computeNextReview(newCard, result)
        
        assertEquals(1, updatedCard.reviewCount)
        assertEquals(3L, updatedCard.intervalDays)
        assertEquals(0, updatedCard.consecutiveFails)
        assertEquals(2.3, updatedCard.easeFactor, 0.01)
        
        // Verify next due time is approximately 3 days from now
        val expectedNextDue = System.currentTimeMillis() + (3 * 24 * 60 * 60 * 1000L)
        assertTrue("Next due time should be approximately 3 days", 
            abs(nextDueAt - expectedNextDue) < 1000) // Within 1 second tolerance
    }
    
    @Test
    fun `card with interval 10 days and INSTANT recall with fast response should extend to ~33 days`() {
        val card = createTestCard(
            intervalDays = 10,
            easeFactor = 2.5,
            reviewCount = 5
        )
        val result = ReviewResult(Confidence.INSTANT, 3000L) // 3 seconds response
        
        val (updatedCard, nextDueAt) = Scheduler.computeNextReview(card, result)
        
        // Expected calculation:
        // base = round(10 * 2.5) = 25
        // newInterval = 25 * 1.3 (INSTANT) * 1.1 (fast response) = 35.75
        // clamped to 35 days
        assertEquals(35L, updatedCard.intervalDays)
        assertEquals(6, updatedCard.reviewCount)
        assertEquals(0, updatedCard.consecutiveFails)
        
        // Verify next due time
        val expectedNextDue = System.currentTimeMillis() + (35 * 24 * 60 * 60 * 1000L)
        assertTrue("Next due time should be approximately 35 days", 
            abs(nextDueAt - expectedNextDue) < 1000)
    }
    
    @Test
    fun `repeated DIDNT_KNOW should reduce easeFactor and reset interval`() {
        val card = createTestCard(
            intervalDays = 30,
            easeFactor = 2.3,
            reviewCount = 10,
            consecutiveFails = 1
        )
        val result = ReviewResult(Confidence.DIDNT_KNOW, null)
        
        val (updatedCard, nextDueAt) = Scheduler.computeNextReview(card, result)
        
        assertEquals(1L, updatedCard.intervalDays) // Reset to 1 day
        assertEquals(2, updatedCard.consecutiveFails) // Incremented
        assertEquals(2.15, updatedCard.easeFactor, 0.01) // Reduced by 0.15
        assertEquals(10, updatedCard.reviewCount) // Unchanged
        
        // Verify next due time is 1 day from now
        val expectedNextDue = System.currentTimeMillis() + (1 * 24 * 60 * 60 * 1000L)
        assertTrue("Next due time should be 1 day", 
            abs(nextDueAt - expectedNextDue) < 1000)
    }
    
    @Test
    fun `easeFactor should not go below 1_1 even with multiple consecutive fails`() {
        val card = createTestCard(
            intervalDays = 20,
            easeFactor = 1.2,
            reviewCount = 5,
            consecutiveFails = 3
        )
        val result = ReviewResult(Confidence.DIDNT_KNOW, null)
        
        val (updatedCard, _) = Scheduler.computeNextReview(card, result)
        
        assertEquals(1.1, updatedCard.easeFactor, 0.01) // Clamped to minimum
        assertEquals(4, updatedCard.consecutiveFails)
    }
    
    @Test
    fun `interval should be clamped to maximum 365 days`() {
        val card = createTestCard(
            intervalDays = 200,
            easeFactor = 3.0,
            reviewCount = 10
        )
        val result = ReviewResult(Confidence.INSTANT, 2000L) // Very fast response
        
        val (updatedCard, _) = Scheduler.computeNextReview(card, result)
        
        assertEquals(365L, updatedCard.intervalDays) // Clamped to maximum
    }
    
    @Test
    fun `interval should be clamped to minimum 1 day`() {
        val card = createTestCard(
            intervalDays = 1,
            easeFactor = 0.5, // Very low ease factor
            reviewCount = 5
        )
        val result = ReviewResult(Confidence.BARELY, 20000L) // Slow response
        
        val (updatedCard, _) = Scheduler.computeNextReview(card, result)
        
        assertEquals(1L, updatedCard.intervalDays) // Clamped to minimum
    }
    
    @Test
    fun `response time modifiers should work correctly`() {
        val card = createTestCard(intervalDays = 10, easeFactor = 2.0, reviewCount = 3)
        
        // Fast response (< 5s) should increase interval
        val fastResult = ReviewResult(Confidence.KNEW, 3000L)
        val (fastCard, _) = Scheduler.computeNextReview(card, fastResult)
        val fastInterval = fastCard.intervalDays
        
        // Normal response (5-15s) should be baseline
        val normalResult = ReviewResult(Confidence.KNEW, 10000L)
        val (normalCard, _) = Scheduler.computeNextReview(card, normalResult)
        val normalInterval = normalCard.intervalDays
        
        // Slow response (> 15s) should decrease interval
        val slowResult = ReviewResult(Confidence.KNEW, 20000L)
        val (slowCard, _) = Scheduler.computeNextReview(card, slowResult)
        val slowInterval = slowCard.intervalDays
        
        // Fast should be larger than normal, normal should be larger than slow
        assertTrue("Fast response should have larger interval than normal", fastInterval > normalInterval)
        assertTrue("Normal response should have larger interval than slow", normalInterval > slowInterval)
    }
    
    @Test
    fun `disabling response time should not apply time modifiers`() {
        val card = createTestCard(intervalDays = 10, easeFactor = 2.0, reviewCount = 3)
        
        val result = ReviewResult(Confidence.KNEW, 3000L) // Fast response
        
        val (cardWithTime, _) = Scheduler.computeNextReview(card, result, useResponseTime = true)
        val (cardWithoutTime, _) = Scheduler.computeNextReview(card, result, useResponseTime = false)
        
        // Without response time, the interval should be smaller (no 1.1 multiplier)
        assertTrue("Interval without response time should be smaller", 
            cardWithoutTime.intervalDays < cardWithTime.intervalDays)
    }
    
    @Test
    fun `BARELY confidence should reduce interval appropriately`() {
        val card = createTestCard(intervalDays = 10, easeFactor = 2.0, reviewCount = 3)
        
        val result = ReviewResult(Confidence.BARELY, 10000L)
        val (updatedCard, _) = Scheduler.computeNextReview(card, result)
        
        // Expected: 10 * 2.0 * 0.8 = 16, but rounded
        assertEquals(16L, updatedCard.intervalDays)
    }
    
    @Test
    fun `successful review should reset consecutive fails`() {
        val card = createTestCard(
            intervalDays = 5,
            easeFactor = 2.0,
            reviewCount = 3,
            consecutiveFails = 2
        )
        
        val result = ReviewResult(Confidence.KNEW, 8000L)
        val (updatedCard, _) = Scheduler.computeNextReview(card, result)
        
        assertEquals(0, updatedCard.consecutiveFails) // Should be reset
        assertEquals(4, updatedCard.reviewCount) // Should be incremented
    }
}
