package com.vmargb.myoso.data

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface CardDao {
    
    @Query("SELECT * FROM cards WHERE deckId IN (:deckIds) AND nextDueAt <= :nowMs ORDER BY nextDueAt ASC")
    suspend fun getDueCards(deckIds: List<String>, nowMs: Long): List<CardEntity>
    
    @Query("SELECT * FROM cards WHERE deckId = :deckId ORDER BY createdAt ASC")
    suspend fun getCardsByDeck(deckId: String): List<CardEntity>
    
    @Query("SELECT * FROM cards WHERE deckId = :deckId ORDER BY createdAt ASC")
    fun getCardsByDeckFlow(deckId: String): Flow<List<CardEntity>>
    
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
    
    @Query("SELECT COUNT(*) FROM cards WHERE deckId IN (:deckIds) AND nextDueAt <= :nowMs")
    suspend fun getDueCardCount(deckIds: List<String>, nowMs: Long): Int
}
