package com.vmargb.myoso.ui.notes

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.vmargb.myoso.data.*
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NoteEditorScreen(
    noteId: String?,
    deckId: String,
    onNavigateBack: () -> Unit,
    modifier: Modifier = Modifier
) {
    val viewModel: NoteEditorViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    val scope = rememberCoroutineScope()
    
    LaunchedEffect(noteId, deckId) {
        viewModel.loadNote(noteId, deckId)
        viewModel.loadCards(deckId)
    }
    
    Column(
        modifier = modifier.fillMaxSize()
    ) {
        // Top App Bar
        TopAppBar(
            title = { 
                Text(
                    text = if (noteId == null) "New Note" else "Edit Note"
                )
            },
            navigationIcon = {
                IconButton(onClick = onNavigateBack) {
                    Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                }
            },
            actions = {
                IconButton(
                    onClick = { 
                        scope.launch {
                            viewModel.toggleEditMode()
                        }
                    }
                ) {
                    Icon(
                        if (uiState.isEditing) Icons.Default.Visibility else Icons.Default.Edit,
                        contentDescription = if (uiState.isEditing) "Preview" else "Edit"
                    )
                }
                
                IconButton(
                    onClick = { 
                        scope.launch {
                            viewModel.saveNote()
                        }
                    }
                ) {
                    Icon(Icons.Default.Save, contentDescription = "Save")
                }
            }
        )
        
        // Content
        when {
            uiState.isLoading -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center
                ) {
                    CircularProgressIndicator()
                }
            }
            
            uiState.isEditing -> {
                NoteEditView(
                    text = uiState.text,
                    onTextChange = viewModel::updateText,
                    onSelectionChange = viewModel::updateSelection,
                    onCiteClick = {
                        scope.launch {
                            viewModel.showCardPicker()
                        }
                    },
                    modifier = Modifier.fillMaxSize()
                )
            }
            
            else -> {
                NotePreviewView(
                    markdownText = uiState.text,
                    cards = uiState.cards,
                    onCitationClick = { cardId ->
                        scope.launch {
                            viewModel.showCardPreview(cardId)
                        }
                    },
                    modifier = Modifier.fillMaxSize()
                )
            }
        }
    }
    
    // Card Picker Dialog
    if (uiState.showCardPicker) {
        CardPickerDialog(
            cards = uiState.availableCards,
            onCardSelected = { card ->
                scope.launch {
                    viewModel.insertCitation(card.id)
                }
            },
            onDismiss = {
                viewModel.hideCardPicker()
            }
        )
    }
    
    // Card Preview Dialog
    if (uiState.showCardPreview) {
        CardPreviewDialog(
            card = uiState.previewCard,
            onDismiss = {
                viewModel.hideCardPreview()
            },
            onJumpToAnchor = {
                scope.launch {
                    viewModel.jumpToAnchor()
                }
            }
        )
    }
}

@Composable
private fun NoteEditView(
    text: String,
    onTextChange: (String) -> Unit,
    onSelectionChange: (Int, Int) -> Unit,
    onCiteClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val focusRequester = remember { FocusRequester() }
    var textFieldValue by remember { 
        mutableStateOf(TextFieldValue(text = text))
    }
    
    LaunchedEffect(text) {
        if (textFieldValue.text != text) {
            textFieldValue = textFieldValue.copy(text = text)
        }
    }
    
    Column(
        modifier = modifier.fillMaxSize()
    ) {
        // Toolbar
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surfaceVariant
            )
        ) {
            Row(
                modifier = Modifier.padding(8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                TextButton(
                    onClick = onCiteClick,
                    enabled = true
                ) {
                    Icon(
                        Icons.Default.Link,
                        contentDescription = "Cite",
                        modifier = Modifier.size(16.dp)
                    )
                    Spacer(modifier = Modifier.width(4.dp))
                    Text("Cite")
                }
                
                Spacer(modifier = Modifier.weight(1f))
                
                Text(
                    text = "Markdown Editor",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
        
        // Text Editor
        SelectionContainer {
            OutlinedTextField(
                value = textFieldValue,
                onValueChange = { newValue ->
                    textFieldValue = newValue
                    onTextChange(newValue.text)
                    onSelectionChange(newValue.selection.start, newValue.selection.end)
                },
                modifier = Modifier
                    .fillMaxSize()
                    .padding(16.dp)
                    .focusRequester(focusRequester),
                placeholder = {
                    Text("Start writing your note...")
                },
                minLines = 10,
                maxLines = Int.MAX_VALUE
            )
        }
    }
    
    LaunchedEffect(Unit) {
        focusRequester.requestFocus()
    }
}

@Composable
private fun NotePreviewView(
    markdownText: String,
    cards: Map<String, CardEntity>,
    onCitationClick: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier.fillMaxSize()
    ) {
        // Preview Header
        Card(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.surfaceVariant
            )
        ) {
            Row(
                modifier = Modifier.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    Icons.Default.Visibility,
                    contentDescription = "Preview",
                    modifier = Modifier.size(20.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = "Preview Mode",
                    style = MaterialTheme.typography.titleMedium
                )
                
                Spacer(modifier = Modifier.weight(1f))
                
                Text(
                    text = "Click citations to preview cards",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
        
        // Markdown Content
        Card(
            modifier = Modifier
                .fillMaxSize()
                .padding(16.dp)
        ) {
            Column(
                modifier = Modifier.padding(16.dp)
            ) {
                if (markdownText.isBlank()) {
                    Box(
                        modifier = Modifier.fillMaxSize(),
                        contentAlignment = Alignment.Center
                    ) {
                        Text(
                            text = "No content to preview",
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            textAlign = TextAlign.Center
                        )
                    }
                } else {
                    NoteMarkdownRenderer(
                        markdownText = markdownText,
                        cards = cards,
                        onCitationClick = onCitationClick
                    )
                }
            }
        }
    }
}

// ViewModel for NoteEditorScreen
@Stable
data class NoteEditorUiState(
    val isLoading: Boolean = false,
    val isEditing: Boolean = true,
    val text: String = "",
    val selectionStart: Int = 0,
    val selectionEnd: Int = 0,
    val cards: Map<String, CardEntity> = emptyMap(),
    val availableCards: List<CardEntity> = emptyList(),
    val showCardPicker: Boolean = false,
    val showCardPreview: Boolean = false,
    val previewCard: CardEntity? = null,
    val currentNote: NoteEntity? = null
)

class NoteEditorViewModel(
    private val noteDao: NoteDao,
    private val cardDao: CardDao,
    private val citationDao: CitationDao
) : androidx.lifecycle.ViewModel() {
    
    private val _uiState = MutableStateFlow(NoteEditorUiState())
    val uiState: StateFlow<NoteEditorUiState> = _uiState.asStateFlow()
    
    suspend fun loadNote(noteId: String?, deckId: String) {
        _uiState.value = _uiState.value.copy(isLoading = true)
        
        val note = if (noteId != null) {
            noteDao.getNoteById(noteId)
        } else {
            null
        }
        
        _uiState.value = _uiState.value.copy(
            currentNote = note,
            text = note?.markdownBody ?: "",
            isLoading = false
        )
    }
    
    suspend fun loadCards(deckId: String) {
        val cards = cardDao.getCardsByDeck(deckId)
        val cardsMap = cards.associateBy { it.id }
        
        _uiState.value = _uiState.value.copy(
            availableCards = cards,
            cards = cardsMap
        )
    }
    
    fun updateText(text: String) {
        _uiState.value = _uiState.value.copy(text = text)
    }
    
    fun updateSelection(start: Int, end: Int) {
        _uiState.value = _uiState.value.copy(
            selectionStart = start,
            selectionEnd = end
        )
    }
    
    fun toggleEditMode() {
        _uiState.value = _uiState.value.copy(
            isEditing = !_uiState.value.isEditing
        )
    }
    
    fun showCardPicker() {
        _uiState.value = _uiState.value.copy(showCardPicker = true)
    }
    
    fun hideCardPicker() {
        _uiState.value = _uiState.value.copy(showCardPicker = false)
    }
    
    suspend fun insertCitation(cardId: String) {
        val currentText = _uiState.value.text
        val selectionStart = _uiState.value.selectionStart
        val selectionEnd = _uiState.value.selectionEnd
        
        val newText = CitationUtils.insertCitation(
            currentText,
            cardId,
            selectionStart,
            selectionEnd
        )
        
        _uiState.value = _uiState.value.copy(
            text = newText,
            showCardPicker = false
        )
    }
    
    fun showCardPreview(cardId: String) {
        val card = _uiState.value.cards[cardId]
        _uiState.value = _uiState.value.copy(
            showCardPreview = true,
            previewCard = card
        )
    }
    
    fun hideCardPreview() {
        _uiState.value = _uiState.value.copy(
            showCardPreview = false,
            previewCard = null
        )
    }
    
    suspend fun jumpToAnchor() {
        // This would scroll to the citation in edit mode
        // For now, just switch to edit mode
        _uiState.value = _uiState.value.copy(
            isEditing = true,
            showCardPreview = false,
            previewCard = null
        )
    }
    
    suspend fun saveNote() {
        val currentNote = _uiState.value.currentNote
        val text = _uiState.value.text
        val deckId = currentNote?.deckId ?: return
        
        val note = if (currentNote != null) {
            currentNote.copy(
                markdownBody = text,
                updatedAt = System.currentTimeMillis()
            )
        } else {
            NoteEntity(
                id = java.util.UUID.randomUUID().toString(),
                deckId = deckId,
                markdownBody = text,
                updatedAt = System.currentTimeMillis()
            )
        }
        
        noteDao.insertNote(note)
        
        // Extract and save citations
        val citations = CitationUtils.extractCitations(note.id, text)
        citationDao.insertCitations(citations)
        
        _uiState.value = _uiState.value.copy(currentNote = note)
    }
}
