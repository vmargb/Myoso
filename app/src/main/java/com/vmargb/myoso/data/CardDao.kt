package com.vmargb.myoso.data

import androidx.room.*
import kotlinx.coroutines.flow.Flow

// ==========================================
// Data Access Object (DAO) for CardEntity.kt
// ==========================================
/**
 * Interface for managing CardEntity database operations 
 */
@Dao
interface CardDao {
    // fetch all cards for a specific deck
    @Query("SELECT * FROM cards WHERE deckId = :deckId ORDER BY createdAt ASC")
    suspend fun getCardsByDeck(deckId: String): List<CardEntity>
    
    @Query("SELECT * FROM cards WHERE deckId = :deckId ORDER BY createdAt ASC")
    fun getCardsByDeckFlow(deckId: String): Flow<List<CardEntity>>

	// get specific card types from a deck
	@Query("SELECT * FROM cards WHERE deckId = :deckId AND pinned = :pinnedType ORDER BY createdAt ASC")
	suspend fun getCardsByDeckAndType(deckId: String, pinnedType: String): List<CardEntity>

	@Query("SELECT * FROM cards WHERE deckId = :deckId AND pinned = :pinnedType ORDER BY createdAt ASC")
	fun getCardsByDeckAndTypeFlow(deckId: String, pinnedType: String): Flow<List<CardEntity>>
    
    @Query("SELECT * FROM cards WHERE id = :cardId")
    suspend fun getCardById(cardId: String): CardEntity?
    
    @Query("SELECT * FROM cards WHERE pinned = :pinnedType ORDER BY nextDueAt ASC")
    suspend fun getPinnedCards(pinnedType: String): List<CardEntity>
    
    @Query("SELECT * FROM cards WHERE tags LIKE '%' || :tag || '%'")
    suspend fun getCardsByTag(tag: String): List<CardEntity>
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertCard(card: CardEntity)
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertCards(cards: List<CardEntity>)
    
    @Update
    suspend fun updateCard(card: CardEntity)
    
    @Update
    suspend fun updateCardAfterReview(card: CardEntity)
    
    @Delete
    suspend fun deleteCard(card: CardEntity)
    
    @Query("DELETE FROM cards WHERE deckId = :deckId")
    suspend fun deleteCardsByDeck(deckId: String)
    
    @Query("SELECT COUNT(*) FROM cards WHERE deckId = :deckId")
    suspend fun getCardCountByDeck(deckId: String): Int
    
	// get due cards where card type is spaced-repetition
	@Query("SELECT * FROM cards WHERE deckId IN (:deckIds) AND nextDueAt <= :nowMs AND (pinned IS NULL OR pinned = '') ORDER BY nextDueAt ASC")
	suspend fun getDueCards(deckIds: List<String>, nowMs: Long): List<CardEntity>

	// get due card count where card type is spaced-repetition
	@Query("SELECT COUNT(*) FROM cards WHERE deckId IN (:deckIds) AND nextDueAt <= :nowMs AND (pinned IS NULL OR pinned = '')")
	suspend fun getDueCardCount(deckIds: List<String>, nowMs: Long): Int
    
    @Query("DELETE FROM cards")
    suspend fun deleteAllCards()
}
