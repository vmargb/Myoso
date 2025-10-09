package com.vmargb.myoso.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vmargb.myoso.data.CardDao
import com.vmargb.myoso.data.CardEntity
import com.vmargb.myoso.data.CardRepository
import com.vmargb.myoso.scheduling.Confidence
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

class CardViewModel(
    private val cardRepository: CardRepository,
    private val cardDao: CardDao // provided so we can access pinned / dao-specific queries not exposed by repo
) : ViewModel() {

    /**
     * Observe cards for a deck.
     * Uses repository which delegates to [`com.vmargb.myoso.data.CardDao`](app/src/main/java/com/vmargb/myoso/data/CardDao.kt)
     */
    fun getCardsByDeckFlow(deckId: String): Flow<List<CardEntity>> =
        cardRepository.getCardsByDeckFlow(deckId)

    /**
     * One-shot fetch of cards for a deck.
     */
    suspend fun getCardsByDeck(deckId: String): List<CardEntity> =
        cardRepository.getCardsByDeck(deckId)

    /**
     * Get due cards across provided decks (uses current time).
     */
    suspend fun getDueCards(deckIds: List<String>): List<CardEntity> =
        cardRepository.getDueCards(deckIds)

    /**
     * Insert a card.
     */
    fun insertCard(card: CardEntity) {
        viewModelScope.launch {
            cardRepository.insertCard(card)
        }
    }

    /**
     * Update a card.
     */
    fun updateCard(card: CardEntity) {
        viewModelScope.launch {
            cardRepository.updateCard(card)
        }
    }

    /**
     * Review a card. Uses [`com.vmargb.myoso.scheduling.Scheduler`](app/src/main/java/com/vmargb/myoso/scheduling/Scheduler.kt)
     * via the repository which persists the updated card.
     */
    fun reviewCard(card: CardEntity, confidence: Confidence, responseTimeMs: Long?) {
        viewModelScope.launch {
            cardRepository.reviewCard(card, confidence, responseTimeMs)
        }
    }

    /**
     * Get pinned cards (delegates to DAO because repository currently does not wrap this).
     */
    suspend fun getPinnedCards(pinnedType: String): List<CardEntity> =
        cardDao.getPinnedCards(pinnedType)
}