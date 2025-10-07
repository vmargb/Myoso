package com.vmargb.myoso.scheduling

import com.vmargb.myoso.data.CardEntity
import kotlin.math.round
import kotlin.math.max
import kotlin.math.min

enum class Confidence(val multiplier: Double) {
    DIDNT_KNOW(0.0),
    BARELY(0.8),
    KNEW(1.0),
    INSTANT(1.3)
}

data class ReviewResult(
    val confidence: Confidence,
    val responseTimeMs: Long?
)

object Scheduler {
    
    /**
     * Computes the next review interval and updates card properties based on review result
     * @param card The card being reviewed
     * @param result The review result with confidence and response time
     * @param useResponseTime Whether to factor in response time for interval calculation
     * @return Pair of updated CardEntity and nextDueAt timestamp (epoch ms)
     */
    fun computeNextReview(
        card: CardEntity, 
        result: ReviewResult, 
        useResponseTime: Boolean = true
    ): Pair<CardEntity, Long> {
        val now = System.currentTimeMillis()
        
        return when (result.confidence) {
            Confidence.DIDNT_KNOW -> {
                // Reset interval to 1 day, increment consecutive fails
                val newConsecutiveFails = card.consecutiveFails + 1
                val newEaseFactor = if (newConsecutiveFails >= 2) {
                    max(1.1, card.easeFactor - 0.15)
                } else {
                    card.easeFactor
                }
                
                val updatedCard = card.copy(
                    intervalDays = 1,
                    consecutiveFails = newConsecutiveFails,
                    easeFactor = newEaseFactor,
                    lastReviewedAt = now,
                    nextDueAt = now + (1 * 24 * 60 * 60 * 1000L) // 1 day in ms
                )
                
                Pair(updatedCard, updatedCard.nextDueAt)
            }
            
            else -> {
                // Calculate new interval based on spaced repetition algorithm
                val baseInterval = when (card.reviewCount) {
                    0 -> 1.0
                    1 -> 3.0
                    else -> round(card.intervalDays * card.easeFactor)
                }
                
                // Apply confidence multiplier
                var newInterval = baseInterval * result.confidence.multiplier
                
                // Apply response time modifier if enabled
                if (useResponseTime && result.responseTimeMs != null) {
                    val timeModifier = when {
                        result.responseTimeMs < 5000 -> 1.1  // < 5 seconds
                        result.responseTimeMs <= 15000 -> 1.0 // 5-15 seconds
                        else -> 0.85 // > 15 seconds
                    }
                    newInterval *= timeModifier
                }
                
                // Clamp interval between 1 and 365 days
                val clampedInterval = min(365.0, max(1.0, newInterval))
                val newIntervalDays = round(clampedInterval).toLong()
                
                val updatedCard = card.copy(
                    intervalDays = newIntervalDays,
                    reviewCount = card.reviewCount + 1,
                    consecutiveFails = 0, // Reset consecutive fails on successful review
                    easeFactor = card.easeFactor, // Keep ease factor for successful reviews
                    lastReviewedAt = now,
                    nextDueAt = now + (newIntervalDays * 24 * 60 * 60 * 1000L)
                )
                
                Pair(updatedCard, updatedCard.nextDueAt)
            }
        }
    }
}
