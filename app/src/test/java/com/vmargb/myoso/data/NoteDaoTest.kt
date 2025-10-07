package com.vmargb.myoso.data

import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.Assert.*

@RunWith(AndroidJUnit4::class)
class NoteDaoTest {
    
    private lateinit var database: AppDatabase
    private lateinit var noteDao: NoteDao
    private lateinit var deckDao: DeckDao
    
    @Before
    fun setup() {
        database = Room.inMemoryDatabaseBuilder(
            ApplicationProvider.getApplicationContext(),
            AppDatabase::class.java
        ).allowMainThreadQueries().build()
        
        noteDao = database.noteDao()
        deckDao = database.deckDao()
    }
    
    @After
    fun teardown() {
        database.close()
    }
    
    @Test
    fun `insert and get note by id`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note = createTestNote("note1", "deck1", "# Spanish Learning\n\nThis is a note about Spanish learning.")
        noteDao.insertNote(note)
        
        val retrievedNote = noteDao.getNoteById("note1")
        assertNotNull(retrievedNote)
        assertEquals("note1", retrievedNote!!.id)
        assertEquals("deck1", retrievedNote.deckId)
        assertTrue(retrievedNote.markdownBody.contains("Spanish Learning"))
    }
    
    @Test
    fun `get notes by deck`() = runBlocking {
        val deck1 = createTestDeck("deck1")
        val deck2 = createTestDeck("deck2")
        deckDao.insertDecks(listOf(deck1, deck2))
        
        val note1 = createTestNote("note1", "deck1", "Note 1 content")
        val note2 = createTestNote("note2", "deck1", "Note 2 content")
        val note3 = createTestNote("note3", "deck2", "Note 3 content")
        
        noteDao.insertNotes(listOf(note1, note2, note3))
        
        val deck1Notes = noteDao.getNotesByDeck("deck1")
        assertEquals(2, deck1Notes.size)
        assertTrue(deck1Notes.any { it.id == "note1" })
        assertTrue(deck1Notes.any { it.id == "note2" })
        assertFalse(deck1Notes.any { it.id == "note3" })
    }
    
    @Test
    fun `search notes by content`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note1 = createTestNote("note1", "deck1", "This note is about Spanish vocabulary")
        val note2 = createTestNote("note2", "deck1", "This note is about French grammar")
        val note3 = createTestNote("note3", "deck1", "Another Spanish learning note")
        
        noteDao.insertNotes(listOf(note1, note2, note3))
        
        val spanishNotes = noteDao.searchNotes("Spanish")
        assertEquals(2, spanishNotes.size)
        assertTrue(spanishNotes.any { it.id == "note1" })
        assertTrue(spanishNotes.any { it.id == "note3" })
        assertFalse(spanishNotes.any { it.id == "note2" })
    }
    
    @Test
    fun `search notes in specific deck`() = runBlocking {
        val deck1 = createTestDeck("deck1")
        val deck2 = createTestDeck("deck2")
        deckDao.insertDecks(listOf(deck1, deck2))
        
        val note1 = createTestNote("note1", "deck1", "Spanish vocabulary")
        val note2 = createTestNote("note2", "deck1", "Spanish grammar")
        val note3 = createTestNote("note3", "deck2", "Spanish culture")
        
        noteDao.insertNotes(listOf(note1, note2, note3))
        
        val spanishNotesInDeck1 = noteDao.searchNotesInDeck("deck1", "Spanish")
        assertEquals(2, spanishNotesInDeck1.size)
        assertTrue(spanishNotesInDeck1.any { it.id == "note1" })
        assertTrue(spanishNotesInDeck1.any { it.id == "note2" })
        assertFalse(spanishNotesInDeck1.any { it.id == "note3" })
    }
    
    @Test
    fun `update note`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note = createTestNote("note1", "deck1", "Original content")
        noteDao.insertNote(note)
        
        val updatedNote = note.copy(
            markdownBody = "Updated content with more information",
            updatedAt = System.currentTimeMillis()
        )
        
        noteDao.updateNote(updatedNote)
        
        val retrievedNote = noteDao.getNoteById("note1")
        assertNotNull(retrievedNote)
        assertEquals("Updated content with more information", retrievedNote!!.markdownBody)
    }
    
    @Test
    fun `delete note`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note = createTestNote("note1", "deck1", "Content to be deleted")
        noteDao.insertNote(note)
        
        noteDao.deleteNote(note)
        
        val retrievedNote = noteDao.getNoteById("note1")
        assertNull(retrievedNote)
    }
    
    @Test
    fun `delete note by id`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note = createTestNote("note1", "deck1", "Content to be deleted")
        noteDao.insertNote(note)
        
        noteDao.deleteNoteById("note1")
        
        val retrievedNote = noteDao.getNoteById("note1")
        assertNull(retrievedNote)
    }
    
    @Test
    fun `delete notes by deck`() = runBlocking {
        val deck1 = createTestDeck("deck1")
        val deck2 = createTestDeck("deck2")
        deckDao.insertDecks(listOf(deck1, deck2))
        
        val note1 = createTestNote("note1", "deck1", "Deck 1 note")
        val note2 = createTestNote("note2", "deck1", "Another deck 1 note")
        val note3 = createTestNote("note3", "deck2", "Deck 2 note")
        
        noteDao.insertNotes(listOf(note1, note2, note3))
        
        noteDao.deleteNotesByDeck("deck1")
        
        val deck1Notes = noteDao.getNotesByDeck("deck1")
        val deck2Notes = noteDao.getNotesByDeck("deck2")
        
        assertEquals(0, deck1Notes.size)
        assertEquals(1, deck2Notes.size)
        assertEquals("note3", deck2Notes[0].id)
    }
    
    @Test
    fun `get note count by deck`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val notes = listOf(
            createTestNote("note1", "deck1", "Note 1"),
            createTestNote("note2", "deck1", "Note 2"),
            createTestNote("note3", "deck1", "Note 3")
        )
        noteDao.insertNotes(notes)
        
        val count = noteDao.getNoteCountByDeck("deck1")
        assertEquals(3, count)
    }
    
    @Test
    fun `get notes by deck flow`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note = createTestNote("note1", "deck1", "Test content")
        noteDao.insertNote(note)
        
        val flow = noteDao.getNotesByDeckFlow("deck1")
        val notes = flow.first()
        
        assertEquals(1, notes.size)
        assertEquals("note1", notes[0].id)
    }
    
    @Test
    fun `get note by id flow`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note = createTestNote("note1", "deck1", "Test content")
        noteDao.insertNote(note)
        
        val flow = noteDao.getNoteByIdFlow("note1")
        val retrievedNote = flow.first()
        
        assertNotNull(retrievedNote)
        assertEquals("note1", retrievedNote!!.id)
        assertEquals("deck1", retrievedNote.deckId)
    }
    
    @Test
    fun `insert note with same id replaces existing`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note1 = createTestNote("note1", "deck1", "Original content")
        val note2 = createTestNote("note1", "deck1", "Updated content")
        
        noteDao.insertNote(note1)
        noteDao.insertNote(note2) // Should replace note1
        
        val retrievedNote = noteDao.getNoteById("note1")
        assertNotNull(retrievedNote)
        assertEquals("Updated content", retrievedNote!!.markdownBody)
    }
    
    @Test
    fun `delete all notes`() = runBlocking {
        val deck = createTestDeck("deck1")
        deckDao.insertDeck(deck)
        
        val note1 = createTestNote("note1", "deck1", "Note 1")
        val note2 = createTestNote("note2", "deck1", "Note 2")
        
        noteDao.insertNotes(listOf(note1, note2))
        
        noteDao.deleteAllNotes()
        
        val count = noteDao.getNoteCountByDeck("deck1")
        assertEquals(0, count)
    }
    
    private fun createTestNote(
        id: String,
        deckId: String,
        markdownBody: String
    ) = NoteEntity(
        id = id,
        deckId = deckId,
        markdownBody = markdownBody,
        updatedAt = System.currentTimeMillis()
    )
    
    private fun createTestDeck(id: String) = DeckEntity(
        id = id,
        name = "Test Deck $id",
        description = "Test description",
        createdAt = System.currentTimeMillis(),
        updatedAt = System.currentTimeMillis()
    )
}
