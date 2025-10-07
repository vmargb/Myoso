package com.vmargb.myoso.data

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface CitationDao {
    
    @Query("SELECT * FROM citations WHERE noteId = :noteId ORDER BY startIndex ASC")
    suspend fun getCitationsByNote(noteId: String): List<CitationEntity>
    
    @Query("SELECT * FROM citations WHERE noteId = :noteId ORDER BY startIndex ASC")
    fun getCitationsByNoteFlow(noteId: String): Flow<List<CitationEntity>>
    
    @Query("SELECT * FROM citations WHERE cardId = :cardId ORDER BY startIndex ASC")
    suspend fun getCitationsByCard(cardId: String): List<CitationEntity>
    
    @Query("SELECT * FROM citations WHERE cardId = :cardId ORDER BY startIndex ASC")
    fun getCitationsByCardFlow(cardId: String): Flow<List<CitationEntity>>
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertCitation(citation: CitationEntity)
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertCitations(citations: List<CitationEntity>)
    
    @Update
    suspend fun updateCitation(citation: CitationEntity)
    
    @Delete
    suspend fun deleteCitation(citation: CitationEntity)
    
    @Query("DELETE FROM citations WHERE noteId = :noteId")
    suspend fun deleteCitationsByNote(noteId: String)
    
    @Query("DELETE FROM citations WHERE cardId = :cardId")
    suspend fun deleteCitationsByCard(cardId: String)
    
    @Query("SELECT COUNT(*) FROM citations WHERE noteId = :noteId")
    suspend fun getCitationCountByNote(noteId: String): Int
    
    @Query("SELECT COUNT(*) FROM citations WHERE cardId = :cardId")
    suspend fun getCitationCountByCard(cardId: String): Int
}
