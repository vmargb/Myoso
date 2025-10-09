package com.vmargb.myoso.data

import com.vmargb.myoso.scheduling.Confidence
import com.vmargb.myoso.scheduling.ReviewResult
import com.vmargb.myoso.scheduling.Scheduler
import kotlinx.coroutines.flow.Flow

class CardRepository(
    private val database: AppDatabase // inject AppDatabase so DAOs come from it
) {

    private val cardDao: CardDao = database.cardDao()
    private val deckDao: DeckDao = database.deckDao()

    // Get cards for a deck (one-shot)
    suspend fun getCardsByDeck(deckId: String): List<CardEntity> =
        cardDao.getCardsByDeck(deckId)

    // Observe cards by deck
    fun getCardsByDeckFlow(deckId: String): Flow<List<CardEntity>> =
        cardDao.getCardsByDeckFlow(deckId)

    // Get due cards from multiple decks (uses current time)
    suspend fun getDueCards(deckIds: List<String>): List<CardEntity> {
        val now = System.currentTimeMillis()
        return cardDao.getDueCards(deckIds, now)
    }

    suspend fun insertCard(card: CardEntity) {
        cardDao.insertCard(card)
    }

    suspend fun updateCard(card: CardEntity) {
        cardDao.updateCard(card)
    }

    /**
     * Reviews a card using Scheduler.computeNextReview and persists the updated card.
     * Returns Pair of updated CardEntity and nextDueAt timestamp (ms).
     */
    suspend fun reviewCard(
        card: CardEntity,
        confidence: Confidence,
        responseTimeMs: Long?
    ): Pair<CardEntity, Long> {
        val result = ReviewResult(confidence, responseTimeMs)
        val (updatedCard, nextDueAt) = Scheduler.computeNextReview(card, result, useResponseTime = true)
        // Persist review-specific update
        cardDao.updateCardAfterReview(updatedCard)
        return Pair(updatedCard, nextDueAt)
    }
}