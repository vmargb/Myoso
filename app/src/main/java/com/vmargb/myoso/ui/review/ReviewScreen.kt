package com.vmargb.myoso.ui.review

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
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
import kotlinx.coroutines.launch
import java.util.UUID

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReviewScreen(
    onNavigateBack: () -> Unit,
    modifier: Modifier = Modifier
) {
    val viewModel: ReviewViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    val scope = rememberCoroutineScope()
    
    LaunchedEffect(Unit) {
        viewModel.loadDecks()
    }
    
    Column(
        modifier = modifier.fillMaxSize()
    ) {
        // Top App Bar
        TopAppBar(
            title = { Text("Review Session") },
            navigationIcon = {
                IconButton(onClick = onNavigateBack) {
                    Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                }
            },
            actions = {
                if (uiState.selectedDecks.isNotEmpty()) {
                    TextButton(
                        onClick = { viewModel.startReview() }
                    ) {
                        Text("Start Review")
                    }
                }
            }
        )
        
        when {
            uiState.isReviewing -> {
                ReviewSession(
                    currentCard = uiState.currentCard,
                    cardsRemaining = uiState.cardsRemaining,
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
                DeckSelection(
                    decks = uiState.availableDecks,
                    selectedDecks = uiState.selectedDecks,
                    onDeckSelected = { deckId ->
                        viewModel.toggleDeckSelection(deckId)
                    }
                )
            }
        }
    }
}

@Composable
private fun DeckSelection(
    decks: List<DeckEntity>,
    selectedDecks: Set<String>,
    onDeckSelected: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp)
    ) {
        Text(
            text = "Select Decks to Review",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold
        )
        
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
private fun DeckSelectionItem(
    deck: DeckEntity,
    isSelected: Boolean,
    onSelected: () -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .selectable(
                selected = isSelected,
                onClick = onSelected
            ),
        colors = CardDefaults.cardColors(
            containerColor = if (isSelected) {
                MaterialTheme.colorScheme.primaryContainer
            } else {
                MaterialTheme.colorScheme.surface
            }
        )
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            RadioButton(
                selected = isSelected,
                onClick = onSelected
            )
            
            Spacer(modifier = Modifier.width(16.dp))
            
            Column(
                modifier = Modifier.weight(1f)
            ) {
                Text(
                    text = deck.name,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Medium
                )
                
                if (!deck.description.isNullOrBlank()) {
                    Text(
                        text = deck.description,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
            
            if (isSelected) {
                Icon(
                    Icons.Default.Check,
                    contentDescription = "Selected",
                    tint = MaterialTheme.colorScheme.primary
                )
            }
        }
    }
}

@Composable
private fun ReviewSession(
    currentCard: CardEntity?,
    cardsRemaining: Int,
    onReviewComplete: (ReviewResult) -> Unit,
    onFinishReview: () -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier.fillMaxSize(),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // Progress indicator
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
                    text = "Cards Remaining",
                    style = MaterialTheme.typography.labelMedium
                )
                Text(
                    text = cardsRemaining.toString(),
                    style = MaterialTheme.typography.headlineMedium,
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
                        text = "Review Complete!",
                        style = MaterialTheme.typography.headlineMedium,
                        fontWeight = FontWeight.Bold
                    )
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(
                        text = "Great job! You've completed all due cards.",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(modifier = Modifier.height(24.dp))
                    Button(
                        onClick = onFinishReview
                    ) {
                        Text("Finish Review")
                    }
                }
            }
        }
    }
}

// ViewModel for ReviewScreen
@Stable
data class ReviewUiState(
    val availableDecks: List<DeckEntity> = emptyList(),
    val selectedDecks: Set<String> = emptySet(),
    val isReviewing: Boolean = false,
    val currentCard: CardEntity? = null,
    val cardsRemaining: Int = 0,
    val isLoading: Boolean = false
)

class ReviewViewModel(
    private val deckDao: DeckDao,
    private val cardDao: CardDao,
    private val reviewHistoryDao: ReviewHistoryDao
) : androidx.lifecycle.ViewModel() {
    
    private val _uiState = MutableStateFlow(ReviewUiState())
    val uiState: StateFlow<ReviewUiState> = _uiState.asStateFlow()
    
    private var dueCards = mutableListOf<CardEntity>()
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
    
    suspend fun startReview() {
        val selectedDecks = _uiState.value.selectedDecks
        if (selectedDecks.isEmpty()) return
        
        val now = System.currentTimeMillis()
        dueCards = cardDao.getDueCards(selectedDecks.toList(), now).toMutableList()
        
        _uiState.value = _uiState.value.copy(
            isReviewing = true,
            currentCard = dueCards.firstOrNull(),
            cardsRemaining = dueCards.size
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
        val nextCard = if (currentCardIndex < dueCards.size) {
            dueCards[currentCardIndex]
        } else {
            null
        }
        
        _uiState.value = _uiState.value.copy(
            currentCard = nextCard,
            cardsRemaining = dueCards.size - currentCardIndex
        )
    }
    
    fun finishReview() {
        _uiState.value = _uiState.value.copy(
            isReviewing = false,
            currentCard = null,
            cardsRemaining = 0,
            selectedDecks = emptySet()
        )
        dueCards.clear()
        currentCardIndex = 0
    }
}
