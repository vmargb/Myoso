package com.vmargb.myoso.data

import kotlinx.coroutines.flow.Flow

class NoteRepository(
    private val database: AppDatabase
) {

    private val noteDao: NoteDao = database.noteDao()
    private val citationDao: CitationDao = database.citationDao()

    // Notes
    suspend fun getNotesByDeck(deckId: String): List<NoteEntity> = noteDao.getNotesByDeck(deckId)
    fun getNotesByDeckFlow(deckId: String): Flow<List<NoteEntity>> = noteDao.getNotesByDeckFlow(deckId)
    suspend fun getNoteById(noteId: String): NoteEntity? = noteDao.getNoteById(noteId)
    fun getNoteByIdFlow(noteId: String) = noteDao.getNoteByIdFlow(noteId)

    suspend fun insertNote(note: NoteEntity) = noteDao.insertNote(note)
    suspend fun insertNotes(notes: List<NoteEntity>) = noteDao.insertNotes(notes)
    suspend fun updateNote(note: NoteEntity) = noteDao.updateNote(note)
    suspend fun deleteNote(note: NoteEntity) = noteDao.deleteNote(note)
    suspend fun deleteNoteById(noteId: String) = noteDao.deleteNoteById(noteId)
    suspend fun deleteNotesByDeck(deckId: String) = noteDao.deleteNotesByDeck(deckId)

    // Citations
    suspend fun getCitationsByNote(noteId: String): List<CitationEntity> = citationDao.getCitationsByNote(noteId)
    fun getCitationsByNoteFlow(noteId: String) = citationDao.getCitationsByNoteFlow(noteId)
    suspend fun insertCitation(citation: CitationEntity) = citationDao.insertCitation(citation)
    suspend fun insertCitations(citations: List<CitationEntity>) = citationDao.insertCitations(citations)
    suspend fun deleteCitationsByNote(noteId: String) = citationDao.deleteCitationsByNote(noteId)

    /**
     * Helper that inserts/updates a note and replaces its citations atomically (simple approach)
     * Note: call from a transaction scope if you need stronger atomicity
     */
    suspend fun insertNoteWithCitations(note: NoteEntity, citations: List<CitationEntity>) {
        noteDao.insertNote(note)
        // Replace citations by inserting (Dao is REPLACE on conflict)
        citationDao.insertCitations(citations)
    }
}