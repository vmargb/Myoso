package com.vmargb.myoso.data

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface CitationDao {
    // get all citations in a specific note
    @Query("SELECT * FROM citations WHERE noteId = :noteId ORDER BY startIndex ASC")
    suspend fun getCitationsByNote(noteId: String): List<CitationEntity>
    
    @Query("SELECT * FROM citations WHERE noteId = :noteId ORDER BY startIndex ASC")
    fun getCitationsByNoteFlow(noteId: String): Flow<List<CitationEntity>>
    
    // get all citations that reference a specific card
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
    
    // delete all citations associated with a specific note
    @Query("DELETE FROM citations WHERE noteId = :noteId")
    suspend fun deleteCitationsByNote(noteId: String)
    
    // delete all citations associated with a specific card
    @Query("DELETE FROM citations WHERE cardId = :cardId")
    suspend fun deleteCitationsByCard(cardId: String)
    
    // get all citations count for a specific note or card
    @Query("SELECT COUNT(*) FROM citations WHERE noteId = :noteId")
    suspend fun getCitationCountByNote(noteId: String): Int
    
    @Query("SELECT COUNT(*) FROM citations WHERE cardId = :cardId")
    suspend fun getCitationCountByCard(cardId: String): Int
    
    @Query("DELETE FROM citations")
    suspend fun deleteAllCitations()
}
