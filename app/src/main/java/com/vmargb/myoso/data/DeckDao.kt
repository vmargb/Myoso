package com.vmargb.myoso.data

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface DeckDao {
    
    @Query("SELECT * FROM decks ORDER BY createdAt ASC")
    suspend fun getAllDecks(): List<DeckEntity>
    
    @Query("SELECT * FROM decks ORDER BY createdAt ASC")
    fun getAllDecksFlow(): Flow<List<DeckEntity>>
    
    @Query("SELECT * FROM decks WHERE id = :deckId")
    suspend fun getDeckById(deckId: String): DeckEntity?
    
    @Query("SELECT * FROM decks WHERE id = :deckId")
    fun getDeckByIdFlow(deckId: String): Flow<DeckEntity?>
    
    @Query("SELECT * FROM decks WHERE name LIKE '%' || :searchQuery || '%'")
    suspend fun searchDecks(searchQuery: String): List<DeckEntity>
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertDeck(deck: DeckEntity)
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertDecks(decks: List<DeckEntity>)
    
    @Update
    suspend fun updateDeck(deck: DeckEntity)
    
    @Delete
    suspend fun deleteDeck(deck: DeckEntity)
    
    @Query("DELETE FROM decks WHERE id = :deckId")
    suspend fun deleteDeckById(deckId: String)
    
    @Query("SELECT COUNT(*) FROM decks")
    suspend fun getDeckCount(): Int
}
