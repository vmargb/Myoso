package com.vmargb.myoso.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vmargb.myoso.data.DeckEntity
import com.vmargb.myoso.data.DeckRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

data class DeckAnalytics(
    val deckId: String,
    val cardCount: Int,
    val dueCount: Int
)

class DeckViewModel(
    private val deckRepository: DeckRepository
) : ViewModel() {

    // Expose decks as a Flow
    fun getAllDecksFlow(): Flow<List<DeckEntity>> = deckRepository.getAllDecksFlow()

    // One-shot
    suspend fun getAllDecks(): List<DeckEntity> = deckRepository.getAllDecks()

    fun createOrUpdateDeck(deck: DeckEntity) {
        viewModelScope.launch {
            deckRepository.insertDeck(deck)
        }
    }

    fun deleteDeck(deck: DeckEntity) {
        viewModelScope.launch {
            deckRepository.deleteDeck(deck)
        }
    }

    fun deleteDeckById(deckId: String) {
        viewModelScope.launch {
            deckRepository.deleteDeckById(deckId)
        }
    }

    // Analytics helpers

    suspend fun getCardCountByDeck(deckId: String): Int =
        deckRepository.getCardCountByDeck(deckId)

    suspend fun getDueCardCount(deckIds: List<String>): Int =
        deckRepository.getDueCardCount(deckIds)

    // Optional helper: load combined analytics for a deck
    private val _analyticsState = MutableStateFlow<List<DeckAnalytics>>(emptyList())
    val analyticsState: StateFlow<List<DeckAnalytics>> = _analyticsState

    fun loadAnalyticsForDecks(deckIds: List<String>) {
        viewModelScope.launch {
            val analytics = deckIds.map { id ->
                val cardCount = deckRepository.getCardCountByDeck(id)
                val dueCount = deckRepository.getDueCardCount(listOf(id))
                DeckAnalytics(deckId = id, cardCount = cardCount, dueCount = dueCount)
            }
            _analyticsState.value = analytics
        }
    }
}