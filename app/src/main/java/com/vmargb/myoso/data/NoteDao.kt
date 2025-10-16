package com.vmargb.myoso.data

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface NoteDao {
    // return all notes in a specific deck, ordered by most recently updated
    @Query("SELECT * FROM notes WHERE deckId = :deckId ORDER BY updatedAt DESC")
    suspend fun getNotesByDeck(deckId: String): List<NoteEntity>
    
    @Query("SELECT * FROM notes WHERE deckId = :deckId ORDER BY updatedAt DESC")
    fun getNotesByDeckFlow(deckId: String): Flow<List<NoteEntity>>
    
    @Query("SELECT * FROM notes WHERE id = :noteId")
    suspend fun getNoteById(noteId: String): NoteEntity?
    
    @Query("SELECT * FROM notes WHERE id = :noteId")
    fun getNoteByIdFlow(noteId: String): Flow<NoteEntity?>
    
    // return all notes that contain the search query in their markdown body
    @Query("SELECT * FROM notes WHERE markdownBody LIKE '%' || :searchQuery || '%'")
    suspend fun searchNotes(searchQuery: String): List<NoteEntity>
    
    // return all notes in a specific deck that contain the search query in their markdown body
    @Query("SELECT * FROM notes WHERE deckId = :deckId AND markdownBody LIKE '%' || :searchQuery || '%'")
    suspend fun searchNotesInDeck(deckId: String, searchQuery: String): List<NoteEntity>
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertNote(note: NoteEntity)
    
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertNotes(notes: List<NoteEntity>)
    
    @Update
    suspend fun updateNote(note: NoteEntity)
    
    @Delete
    suspend fun deleteNote(note: NoteEntity)
    
    @Query("DELETE FROM notes WHERE id = :noteId")
    suspend fun deleteNoteById(noteId: String)
    
    @Query("DELETE FROM notes WHERE deckId = :deckId")
    suspend fun deleteNotesByDeck(deckId: String)
    
    @Query("SELECT COUNT(*) FROM notes WHERE deckId = :deckId")
    suspend fun getNoteCountByDeck(deckId: String): Int
    
    @Query("DELETE FROM notes")
    suspend fun deleteAllNotes()
}
