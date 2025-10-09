package com.vmargb.myoso.data

import kotlinx.coroutines.flow.Flow

class DeckRepository(
    private val database: AppDatabase
) {

    private val deckDao: DeckDao = database.deckDao()
    private val cardDao: CardDao = database.cardDao()

    // CRUD
    suspend fun getAllDecks(): List<DeckEntity> = deckDao.getAllDecks()
    fun getAllDecksFlow(): Flow<List<DeckEntity>> = deckDao.getAllDecksFlow()
    suspend fun getDeckById(deckId: String): DeckEntity? = deckDao.getDeckById(deckId)
    fun getDeckByIdFlow(deckId: String) = deckDao.getDeckByIdFlow(deckId)
    suspend fun insertDeck(deck: DeckEntity) = deckDao.insertDeck(deck)
    suspend fun insertDecks(decks: List<DeckEntity>) = deckDao.insertDecks(decks)
    suspend fun updateDeck(deck: DeckEntity) = deckDao.updateDeck(deck)
    suspend fun deleteDeck(deck: DeckEntity) = deckDao.deleteDeck(deck)
    suspend fun deleteDeckById(deckId: String) = deckDao.deleteDeckById(deckId)

    // Analytics
    suspend fun getDeckCount(): Int = deckDao.getDeckCount()
    suspend fun getCardCountByDeck(deckId: String): Int = cardDao.getCardCountByDeck(deckId)

    /**
     * Returns number of due cards across provided deck IDs (based on current time).
     */
    suspend fun getDueCardCount(deckIds: List<String>): Int {
        val now = System.currentTimeMillis()
        return cardDao.getDueCardCount(deckIds, now)
    }
}