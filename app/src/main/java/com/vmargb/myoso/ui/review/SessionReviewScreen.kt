package com.vmargb.myoso.ui.review

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.vmargb.myoso.data.*
import com.vmargb.myoso.scheduling.Confidence
import com.vmargb.myoso.scheduling.ReviewResult
import com.vmargb.myoso.scheduling.Scheduler
import com.vmargb.myoso.session.*
import kotlinx.coroutines.launch
import java.util.UUID

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewScreen(
    sessionSpec: SessionSpec? = null,
    onNavigateBack: () -> Unit,
    modifier: Modifier = Modifier
) {
    val viewModel: ReviewViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    val scope = rememberCoroutineScope()
    
    LaunchedEffect(sessionSpec) {
        if (sessionSpec != null) {
            viewModel.startSessionReview(sessionSpec)
        } else {
            viewModel.loadDecks()
        }
    }
    
    Column(
        modifier = modifier.fillMaxSize()
    ) {
        // Top App Bar
        TopAppBar(
            title = { 
                Text(
                    text = uiState.sessionSpec?.name ?: "Review Session"
                )
            },
            navigationIcon = {
                IconButton(onClick = onNavigateBack) {
                    Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                }
            },
            actions = {
                if (sessionSpec == null && uiState.selectedDecks.isNotEmpty()) {
                    TextButton(
                        onClick = { viewModel.showSessionCreationDialog() }
                    ) {
                        Text("Create Session")
                    }
                }
                
                if (uiState.isReviewing) {
                    IconButton(
                        onClick = { viewModel.finishReview() }
                    ) {
                        Icon(Icons.Default.Close, contentDescription = "Finish")
                    }
                }
            }
        )
        
        when {
            uiState.isLoading -> {
                Box(
                    modifier = Modifier.fillMaxSize(),
                    contentAlignment = Alignment.Center
                ) {
                    CircularProgressIndicator()
                }
            }
            
            uiState.isReviewing -> {
                SessionReviewView(
                    currentCard = uiState.currentCard,
                    cardsRemaining = uiState.cardsRemaining,
                    sessionSpec = uiState.sessionSpec,
                    onReviewComplete = { result ->
                        scope.launch {
                            viewModel.completeReview(result)
                        }
                    },
                    onFinishReview = {
                        viewModel.finishReview()
                    }
                )
            }
            
            else -> {
                if (sessionSpec == null) {
                    DeckSelectionView(
                        decks = uiState.availableDecks,
                        selectedDecks = uiState.selectedDecks,
                        onDeckSelected = { deckId ->
                            viewModel.toggleDeckSelection(deckId)
                        },
                        onCreateSession = {
                            viewModel.showSessionCreationDialog()
                        }
                    )
                } else {
                    // Session-based review - show session info
                    SessionInfoView(
                        sessionResult = uiState.sessionResult,
                        onStartReview = {
                            viewModel.startReview()
                        }
                    )
                }
            }
        }
    }
    
    // Session Creation Dialog
    if (uiState.showSessionCreationDialog) {
        SessionCreationDialog(
            decks = uiState.availableDecks,
            onSessionCreated = { spec ->
                scope.launch {
                    viewModel.createSession(spec)
                }
            },
            onDismiss = {
                viewModel.hideSessionCreationDialog()
            }
        )
    }
}

@Composable
private fun SessionInfoView(
    sessionResult: SessionResult?,
    onStartReview: () -> Unit,
    modifier: Modifier = Modifier
) {
    if (sessionResult == null) return
    
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // Session Info Card
        Card(
            modifier = Modifier.fillMaxWidth()
        ) {
            Column(
                modifier = Modifier.padding(16.dp)
            ) {
                Text(
                    text = sessionResult.sessionSpec.name,
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold
                )
                
                Spacer(modifier = Modifier.height(8.dp))
                
                Text(
                    text = getSessionTypeDescription(sessionResult.sessionSpec.sessionType),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                
                Spacer(modifier = Modifier.height(16.dp))
                
                // Statistics
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceEvenly
                ) {
                    StatItem(
                        label = "Total Cards",
                        value = sessionResult.totalCards.toString()
                    )
                    StatItem(
                        label = "Due Cards",
                        value = sessionResult.dueCards.toString()
                    )
                    StatItem(
                        label = "Pinned Cards",
                        value = sessionResult.pinnedCards.toString()
                    )
                }
            }
        }
        
        // Deck List
        Card(
            modifier = Modifier.fillMaxWidth()
        ) {
            Column(
                modifier = Modifier.padding(16.dp)
            ) {
                Text(
                    text = "Selected Decks",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Medium
                )
                
                Spacer(modifier = Modifier.height(8.dp))
                
                sessionResult.sessionSpec.deckIds.forEach { deckId ->
                    Text(
                        text = "• Deck ID: $deckId",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }
        
        // Start Review Button
        Button(
            onClick = onStartReview,
            modifier = Modifier.fillMaxWidth(),
            enabled = sessionResult.cards.isNotEmpty()
        ) {
            Icon(Icons.Default.PlayArrow, contentDescription = null)
            Spacer(modifier = Modifier.width(8.dp))
            Text("Start Review")
        }
        
        if (sessionResult.cards.isEmpty()) {
            Card(
                modifier = Modifier.fillMaxWidth(),
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.errorContainer
                )
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    Icon(
                        Icons.Default.Warning,
                        contentDescription = "Warning",
                        tint = MaterialTheme.colorScheme.onErrorContainer
                    )
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(
                        text = "No cards found for this session",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onErrorContainer
                    )
                }
            }
        }
    }
}

@Composable
private fun StatItem(
    label: String,
    value: String,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(
            text = value,
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold
        )
        Text(
            text = label,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Composable
private fun DeckSelectionView(
    decks: List<DeckEntity>,
    selectedDecks: Set<String>,
    onDeckSelected: (String) -> Unit,
    onCreateSession: () -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = "Select Decks to Review",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold
            )
            
            Button(
                onClick = onCreateSession,
                enabled = selectedDecks.isNotEmpty()
            ) {
                Icon(Icons.Default.Add, contentDescription = null)
                Spacer(modifier = Modifier.width(4.dp))
                Text("Create Session")
            }
        }
        
        Spacer(modifier = Modifier.height(16.dp))
        
        Text(
            text = "Choose one or more decks to create a review session",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        
        Spacer(modifier = Modifier.height(24.dp))
        
        LazyColumn(
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            items(decks) { deck ->
                DeckSelectionItem(
                    deck = deck,
                    isSelected = selectedDecks.contains(deck.id),
                    onSelected = { onDeckSelected(deck.id) }
                )
            }
        }
    }
}

@Composable
private fun SessionReviewView(
    currentCard: CardEntity?,
    cardsRemaining: Int,
    sessionSpec: SessionSpec?,
    onReviewComplete: (ReviewResult) -> Unit,
    onFinishReview: () -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier.fillMaxSize(),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // Session Progress
        Card(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp)
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                Text(
                    text = sessionSpec?.name ?: "Review Session",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Medium
                )
                
                Spacer(modifier = Modifier.height(8.dp))
                
                Text(
                    text = "Cards Remaining",
                    style = MaterialTheme.typography.labelMedium
                )
                Text(
                    text = cardsRemaining.toString(),
                    style = MaterialTheme.typography.headlineLarge,
                    fontWeight = FontWeight.Bold
                )
            }
        }
        
        // Card view
        if (currentCard != null) {
            CardViewComposable(
                card = currentCard,
                onReviewComplete = onReviewComplete,
                useResponseTime = true
            )
        } else {
            // No more cards
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp)
            ) {
                Column(
                    modifier = Modifier.padding(32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    Text(
                        text = "🎉",
                        style = MaterialTheme.typography.displayLarge
                    )
                    Spacer(modifier = Modifier.height(16.dp))
                    Text(
                        text = "Session Complete!",
                        style = MaterialTheme.typography.headlineMedium,
                        fontWeight = FontWeight.Bold
                    )
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(
                        text = "Great job! You've completed all cards in this session.",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(modifier = Modifier.height(24.dp))
                    Button(
                        onClick = onFinishReview
                    ) {
                        Text("Finish Session")
                    }
                }
            }
        }
    }
}

private fun getSessionTypeDescription(sessionType: SessionType): String {
    return when (sessionType) {
        SessionType.ALL_CARDS -> "Review all cards from selected decks"
        SessionType.DUE_CARDS -> "Review only cards that are due for review"
        SessionType.PINNED_ONLY -> "Review only pinned cards"
        SessionType.TAG_FILTER -> "Review cards with specific tags"
    }
}

// Updated ViewModel for session support
@Stable
data class ReviewUiState(
    val isLoading: Boolean = false,
    val isReviewing: Boolean = false,
    val availableDecks: List<DeckEntity> = emptyList(),
    val selectedDecks: Set<String> = emptySet(),
    val sessionSpec: SessionSpec? = null,
    val sessionResult: SessionResult? = null,
    val currentCard: CardEntity? = null,
    val cardsRemaining: Int = 0,
    val showSessionCreationDialog: Boolean = false
)

class ReviewViewModel(
    private val deckDao: DeckDao,
    private val cardDao: CardDao,
    private val reviewHistoryDao: ReviewHistoryDao,
    private val sessionManager: SessionManager
) : androidx.lifecycle.ViewModel() {
    
    private val _uiState = MutableStateFlow(ReviewUiState())
    val uiState: StateFlow<ReviewUiState> = _uiState.asStateFlow()
    
    private var sessionCards = mutableListOf<CardEntity>()
    private var currentCardIndex = 0
    
    suspend fun loadDecks() {
        _uiState.value = _uiState.value.copy(isLoading = true)
        val decks = deckDao.getAllDecks()
        _uiState.value = _uiState.value.copy(
            availableDecks = decks,
            isLoading = false
        )
    }
    
    fun toggleDeckSelection(deckId: String) {
        val currentSelected = _uiState.value.selectedDecks.toMutableSet()
        if (currentSelected.contains(deckId)) {
            currentSelected.remove(deckId)
        } else {
            currentSelected.add(deckId)
        }
        _uiState.value = _uiState.value.copy(selectedDecks = currentSelected)
    }
    
    fun showSessionCreationDialog() {
        _uiState.value = _uiState.value.copy(showSessionCreationDialog = true)
    }
    
    fun hideSessionCreationDialog() {
        _uiState.value = _uiState.value.copy(showSessionCreationDialog = false)
    }
    
    suspend fun createSession(sessionSpec: SessionSpec) {
        val sessionResult = sessionManager.getCardsForSession(sessionSpec)
        _uiState.value = _uiState.value.copy(
            sessionSpec = sessionSpec,
            sessionResult = sessionResult,
            showSessionCreationDialog = false
        )
    }
    
    suspend fun startSessionReview(sessionSpec: SessionSpec) {
        val sessionResult = sessionManager.getCardsForSession(sessionSpec)
        sessionCards = sessionResult.cards.toMutableList()
        
        _uiState.value = _uiState.value.copy(
            sessionSpec = sessionSpec,
            sessionResult = sessionResult,
            isReviewing = true,
            currentCard = sessionCards.firstOrNull(),
            cardsRemaining = sessionCards.size
        )
        currentCardIndex = 0
    }
    
    suspend fun startReview() {
        val sessionResult = _uiState.value.sessionResult ?: return
        sessionCards = sessionResult.cards.toMutableList()
        
        _uiState.value = _uiState.value.copy(
            isReviewing = true,
            currentCard = sessionCards.firstOrNull(),
            cardsRemaining = sessionCards.size
        )
        currentCardIndex = 0
    }
    
    suspend fun completeReview(result: ReviewResult) {
        val currentCard = _uiState.value.currentCard ?: return
        
        // Compute next review using scheduler
        val (updatedCard, nextDueAt) = Scheduler.computeNextReview(
            currentCard,
            result,
            useResponseTime = true
        )
        
        // Update card in database
        cardDao.updateCardAfterReview(updatedCard)
        
        // Insert review history
        val reviewHistory = ReviewHistoryEntity(
            id = UUID.randomUUID().toString(),
            cardId = currentCard.id,
            reviewedAt = System.currentTimeMillis(),
            confidence = result.confidence.name,
            responseTimeMs = result.responseTimeMs ?: 0L,
            oldIntervalDays = currentCard.intervalDays,
            newIntervalDays = updatedCard.intervalDays
        )
        reviewHistoryDao.insertReviewHistory(reviewHistory)
        
        // Move to next card
        currentCardIndex++
        val nextCard = if (currentCardIndex < sessionCards.size) {
            sessionCards[currentCardIndex]
        } else {
            null
        }
        
        _uiState.value = _uiState.value.copy(
            currentCard = nextCard,
            cardsRemaining = sessionCards.size - currentCardIndex
        )
    }
    
    fun finishReview() {
        _uiState.value = _uiState.value.copy(
            isReviewing = false,
            currentCard = null,
            cardsRemaining = 0,
            sessionSpec = null,
            sessionResult = null
        )
        sessionCards.clear()
        currentCardIndex = 0
    }
}
