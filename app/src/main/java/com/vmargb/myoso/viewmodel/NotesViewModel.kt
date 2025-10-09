package com.vmargb.myoso.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vmargb.myoso.data.CitationEntity
import com.vmargb.myoso.data.NoteEntity
import com.vmargb.myoso.data.NoteRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.util.UUID

@kotlinx.serialization.ExperimentalSerializationApi
data class NotesUiState(
    val isLoading: Boolean = false,
    val notes: List<NoteEntity> = emptyList(),
    val currentNote: NoteEntity? = null,
    val text: String = "",
    val citations: List<CitationEntity> = emptyList()
)

class NotesViewModel(
    private val noteRepository: NoteRepository
) : ViewModel() {

    private val _uiState = MutableStateFlow(NotesUiState())
    val uiState: StateFlow<NotesUiState> = _uiState.asStateFlow()

    fun loadNotesForDeck(deckId: String) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true)
            val notes = noteRepository.getNotesByDeck(deckId)
            _uiState.value = _uiState.value.copy(isLoading = false, notes = notes)
        }
    }

    fun loadNote(noteId: String?) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true)
            val note = if (noteId != null) noteRepository.getNoteById(noteId) else null
            val citations = if (note != null) noteRepository.getCitationsByNote(note.id) else emptyList()
            _uiState.value = _uiState.value.copy(
                isLoading = false,
                currentNote = note,
                text = note?.markdownBody ?: "",
                citations = citations
            )
        }
    }

    fun updateText(text: String) {
        _uiState.value = _uiState.value.copy(text = text)
    }

    /**
     * Save current note and its citations. Uses NoteRepository which delegates to [`com.vmargb.myoso.data.NoteDao`](app/src/main/java/com/vmargb/myoso/data/NoteDao.kt)
     * and [`com.vmargb.myoso.data.CitationDao`](app/src/main/java/com/vmargb/myoso/data/CitationDao.kt).
     */
    fun saveNote(deckId: String) {
        viewModelScope.launch {
            val current = _uiState.value.currentNote
            val now = System.currentTimeMillis()
            val note = if (current != null) {
                current.copy(markdownBody = _uiState.value.text, updatedAt = now)
            } else {
                NoteEntity(
                    id = UUID.randomUUID().toString(),
                    deckId = deckId,
                    markdownBody = _uiState.value.text,
                    updatedAt = now
                )
            }

            // Insert/replace note
            noteRepository.insertNote(note)

            // Note: extracting citations is handled elsewhere (e.g. CitationUtils)
            // Here we just persist already prepared citations from UI state if any.
            val citations = _uiState.value.citations
            if (citations.isNotEmpty()) {
                noteRepository.insertCitations(citations)
            }

            // refresh state
            val refreshedCitations = noteRepository.getCitationsByNote(note.id)
            _uiState.value = _uiState.value.copy(currentNote = note, citations = refreshedCitations)
        }
    }

    fun insertCitation(citation: CitationEntity) {
        viewModelScope.launch {
            noteRepository.insertCitation(citation)
            val currentNoteId = _uiState.value.currentNote?.id
            if (currentNoteId != null) {
                val all = noteRepository.getCitationsByNote(currentNoteId)
                _uiState.value = _uiState.value.copy(citations = all)
            }
        }
    }

    fun deleteCitationsByNote(noteId: String) {
        viewModelScope.launch {
            noteRepository.deleteCitationsByNote(noteId)
            if (_uiState.value.currentNote?.id == noteId) {
                _uiState.value = _uiState.value.copy(citations = emptyList())
            }
        }
    }
}