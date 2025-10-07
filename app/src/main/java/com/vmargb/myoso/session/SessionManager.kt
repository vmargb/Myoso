package com.vmargb.myoso.session

import com.vmargb.myoso.data.CardEntity
import com.vmargb.myoso.data.CardDao
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

enum class SessionType {
    ALL_CARDS,
    DUE_CARDS,
    PINNED_ONLY,
    TAG_FILTER
}

enum class PinnedFilter {
    DAILY,
    WEEKLY,
    ALL_PINNED
}

data class SessionSpec(
    val sessionId: String,
    val name: String,
    val deckIds: List<String>,
    val sessionType: SessionType,
    val pinnedFilter: PinnedFilter? = null,
    val tagFilter: String? = null,
    val createdAt: Long = System.currentTimeMillis()
)

data class SessionResult(
    val sessionSpec: SessionSpec,
    val cards: List<CardEntity>,
    val totalCards: Int,
    val dueCards: Int,
    val pinnedCards: Int
)

class SessionManager(
    private val cardDao: CardDao
) {
    
    /**
     * Gets cards for a session based on the session specification
     * @param sessionSpec The session configuration
     * @return SessionResult with filtered cards and statistics
     */
    suspend fun getCardsForSession(sessionSpec: SessionSpec): SessionResult = withContext(Dispatchers.IO) {
        val allCards = mutableListOf<CardEntity>()
        
        // Get cards from each selected deck
        sessionSpec.deckIds.forEach { deckId ->
            val deckCards = cardDao.getCardsByDeck(deckId)
            allCards.addAll(deckCards)
        }
        
        // Apply session type filtering
        val filteredCards = when (sessionSpec.sessionType) {
            SessionType.ALL_CARDS -> allCards
            
            SessionType.DUE_CARDS -> {
                val now = System.currentTimeMillis()
                allCards.filter { card ->
                    card.nextDueAt <= now
                }
            }
            
            SessionType.PINNED_ONLY -> {
                val pinnedCards = when (sessionSpec.pinnedFilter) {
                    PinnedFilter.DAILY -> allCards.filter { it.pinned == "daily" }
                    PinnedFilter.WEEKLY -> allCards.filter { it.pinned == "weekly" }
                    PinnedFilter.ALL_PINNED -> allCards.filter { !it.pinned.isNullOrBlank() }
                    null -> allCards.filter { !it.pinned.isNullOrBlank() }
                }
                pinnedCards
            }
            
            SessionType.TAG_FILTER -> {
                val tagFilter = sessionSpec.tagFilter
                if (tagFilter.isNullOrBlank()) {
                    allCards
                } else {
                    allCards.filter { card ->
                        card.tags?.contains(tagFilter, ignoreCase = true) == true
                    }
                }
            }
        }
        
        // Calculate statistics
        val now = System.currentTimeMillis()
        val dueCards = filteredCards.count { it.nextDueAt <= now }
        val pinnedCards = filteredCards.count { !it.pinned.isNullOrBlank() }
        
        SessionResult(
            sessionSpec = sessionSpec,
            cards = filteredCards,
            totalCards = filteredCards.size,
            dueCards = dueCards,
            pinnedCards = pinnedCards
        )
    }
    
    /**
     * Creates a quick session for due cards from selected decks
     */
    suspend fun createDueCardsSession(deckIds: List<String>, sessionName: String = "Due Cards"): SessionResult {
        val sessionSpec = SessionSpec(
            sessionId = generateSessionId(),
            name = sessionName,
            deckIds = deckIds,
            sessionType = SessionType.DUE_CARDS
        )
        return getCardsForSession(sessionSpec)
    }
    
    /**
     * Creates a session for pinned cards from selected decks
     */
    suspend fun createPinnedSession(
        deckIds: List<String>, 
        pinnedFilter: PinnedFilter,
        sessionName: String = "Pinned Cards"
    ): SessionResult {
        val sessionSpec = SessionSpec(
            sessionId = generateSessionId(),
            name = sessionName,
            deckIds = deckIds,
            sessionType = SessionType.PINNED_ONLY,
            pinnedFilter = pinnedFilter
        )
        return getCardsForSession(sessionSpec)
    }
    
    /**
     * Creates a session filtered by tags from selected decks
     */
    suspend fun createTagFilterSession(
        deckIds: List<String>,
        tagFilter: String,
        sessionName: String = "Tag Filter"
    ): SessionResult {
        val sessionSpec = SessionSpec(
            sessionId = generateSessionId(),
            name = sessionName,
            deckIds = deckIds,
            sessionType = SessionType.TAG_FILTER,
            tagFilter = tagFilter
        )
        return getCardsForSession(sessionSpec)
    }
    
    /**
     * Creates a session with all cards from selected decks
     */
    suspend fun createAllCardsSession(deckIds: List<String>, sessionName: String = "All Cards"): SessionResult {
        val sessionSpec = SessionSpec(
            sessionId = generateSessionId(),
            name = sessionName,
            deckIds = deckIds,
            sessionType = SessionType.ALL_CARDS
        )
        return getCardsForSession(sessionSpec)
    }
    
    /**
     * Validates a session specification
     */
    fun validateSessionSpec(sessionSpec: SessionSpec): ValidationResult {
        val errors = mutableListOf<String>()
        
        if (sessionSpec.deckIds.isEmpty()) {
            errors.add("At least one deck must be selected")
        }
        
        if (sessionSpec.name.isBlank()) {
            errors.add("Session name cannot be empty")
        }
        
        when (sessionSpec.sessionType) {
            SessionType.TAG_FILTER -> {
                if (sessionSpec.tagFilter.isNullOrBlank()) {
                    errors.add("Tag filter cannot be empty for tag filter sessions")
                }
            }
            SessionType.PINNED_ONLY -> {
                if (sessionSpec.pinnedFilter == null) {
                    errors.add("Pinned filter must be specified for pinned sessions")
                }
            }
            else -> { /* No additional validation needed */ }
        }
        
        return ValidationResult(
            isValid = errors.isEmpty(),
            errors = errors
        )
    }
    
    private fun generateSessionId(): String {
        return "session_${System.currentTimeMillis()}_${(0..9999).random()}"
    }
}

data class ValidationResult(
    val isValid: Boolean,
    val errors: List<String>
)
