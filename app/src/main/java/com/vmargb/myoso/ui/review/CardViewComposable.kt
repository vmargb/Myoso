package com.vmargb.myoso.ui.review

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.vmargb.myoso.data.CardEntity
import com.vmargb.myoso.scheduling.Confidence
import com.vmargb.myoso.scheduling.ReviewResult
import kotlinx.coroutines.delay

@Composable
fun CardViewComposable(
    card: CardEntity,
    onReviewComplete: (ReviewResult) -> Unit,
    useResponseTime: Boolean = true,
    modifier: Modifier = Modifier
) {
    var isFlipped by remember { mutableStateOf(false) }
    var showConfidenceButtons by remember { mutableStateOf(false) }
    var startTime by remember { mutableStateOf(0L) }
    var responseTime by remember { mutableStateOf<Long?>(null) }
    
    val rotation by animateFloatAsState(
        targetValue = if (isFlipped) 180f else 0f,
        animationSpec = tween(600),
        label = "card_rotation"
    )
    
    val alpha by animateFloatAsState(
        targetValue = if (isFlipped) 0f else 1f,
        animationSpec = tween(300),
        label = "card_alpha"
    )
    
    LaunchedEffect(card.id) {
        // Reset state when card changes
        isFlipped = false
        showConfidenceButtons = false
        startTime = System.currentTimeMillis()
        responseTime = null
    }
    
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(300.dp)
            .padding(16.dp)
    ) {
        // Front of card
        if (!isFlipped) {
            Card(
                modifier = Modifier
                    .fillMaxSize()
                    .clickable {
                        isFlipped = true
                        responseTime = System.currentTimeMillis() - startTime
                        showConfidenceButtons = true
                    }
                    .graphicsLayer {
                        rotationY = rotation
                        alpha = alpha
                    },
                elevation = CardDefaults.cardElevation(defaultElevation = 8.dp)
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(MaterialTheme.colorScheme.primaryContainer)
                        .padding(24.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        Text(
                            text = "Front",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.7f)
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = card.front,
                            style = MaterialTheme.typography.headlineMedium,
                            textAlign = TextAlign.Center,
                            color = MaterialTheme.colorScheme.onPrimaryContainer,
                            fontWeight = FontWeight.Medium
                        )
                    }
                }
            }
        }
        
        // Back of card
        if (isFlipped) {
            Card(
                modifier = Modifier
                    .fillMaxSize()
                    .graphicsLayer {
                        rotationY = rotation
                        alpha = 1f - alpha
                    },
                elevation = CardDefaults.cardElevation(defaultElevation = 8.dp)
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .background(MaterialTheme.colorScheme.secondaryContainer)
                        .padding(24.dp),
                    contentAlignment = Alignment.Center
                ) {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        Text(
                            text = "Back",
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSecondaryContainer.copy(alpha = 0.7f)
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = card.back,
                            style = MaterialTheme.typography.headlineMedium,
                            textAlign = TextAlign.Center,
                            color = MaterialTheme.colorScheme.onSecondaryContainer,
                            fontWeight = FontWeight.Medium
                        )
                    }
                }
            }
        }
        
        // Confidence buttons (shown after flip)
        if (showConfidenceButtons) {
            ConfidenceButtons(
                modifier = Modifier.align(Alignment.BottomCenter),
                onConfidenceSelected = { confidence ->
                    val result = ReviewResult(
                        confidence = confidence,
                        responseTimeMs = if (useResponseTime) responseTime else null
                    )
                    onReviewComplete(result)
                }
            )
        }
    }
}

@Composable
private fun ConfidenceButtons(
    onConfidenceSelected: (Confidence) -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        Text(
            text = "How well did you know this?",
            style = MaterialTheme.typography.titleMedium,
            textAlign = TextAlign.Center,
            modifier = Modifier.fillMaxWidth()
        )
        
        Spacer(modifier = Modifier.height(8.dp))
        
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            ConfidenceButton(
                text = "Didn't know",
                confidence = Confidence.DIDNT_KNOW,
                onClick = { onConfidenceSelected(Confidence.DIDNT_KNOW) },
                modifier = Modifier.weight(1f)
            )
            
            ConfidenceButton(
                text = "Barely",
                confidence = Confidence.BARELY,
                onClick = { onConfidenceSelected(Confidence.BARELY) },
                modifier = Modifier.weight(1f)
            )
        }
        
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            ConfidenceButton(
                text = "Knew it",
                confidence = Confidence.KNEW,
                onClick = { onConfidenceSelected(Confidence.KNEW) },
                modifier = Modifier.weight(1f)
            )
            
            ConfidenceButton(
                text = "Knew instantly",
                confidence = Confidence.INSTANT,
                onClick = { onConfidenceSelected(Confidence.INSTANT) },
                modifier = Modifier.weight(1f)
            )
        }
    }
}

@Composable
private fun ConfidenceButton(
    text: String,
    confidence: Confidence,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val backgroundColor = when (confidence) {
        Confidence.DIDNT_KNOW -> MaterialTheme.colorScheme.errorContainer
        Confidence.BARELY -> MaterialTheme.colorScheme.tertiaryContainer
        Confidence.KNEW -> MaterialTheme.colorScheme.primaryContainer
        Confidence.INSTANT -> MaterialTheme.colorScheme.secondaryContainer
    }
    
    val contentColor = when (confidence) {
        Confidence.DIDNT_KNOW -> MaterialTheme.colorScheme.onErrorContainer
        Confidence.BARELY -> MaterialTheme.colorScheme.onTertiaryContainer
        Confidence.KNEW -> MaterialTheme.colorScheme.onPrimaryContainer
        Confidence.INSTANT -> MaterialTheme.colorScheme.onSecondaryContainer
    }
    
    Button(
        onClick = onClick,
        modifier = modifier.height(48.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = backgroundColor,
            contentColor = contentColor
        ),
        shape = RoundedCornerShape(8.dp)
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.labelMedium,
            fontSize = 12.sp
        )
    }
}
