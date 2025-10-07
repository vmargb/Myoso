package com.vmargb.myoso.ui.notes

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.*
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.vmargb.myoso.data.CardEntity

@Composable
fun NoteMarkdownRenderer(
    markdownText: String,
    cards: Map<String, CardEntity>,
    onCitationClick: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    val annotatedString = buildAnnotatedString {
        val citationMatches = CitationUtils.findCitationMatches(markdownText)
        var lastIndex = 0
        
        citationMatches.forEach { match ->
            // Add text before citation
            if (match.start > lastIndex) {
                append(markdownText.substring(lastIndex, match.start))
            }
            
            // Add clickable citation
            val citationText = markdownText.substring(match.start, match.end)
            val card = cards[match.cardId]
            val displayText = if (card != null) {
                "📎 ${card.front}"
            } else {
                "📎 [Card not found]"
            }
            
            pushStringAnnotation(
                tag = "citation",
                annotation = match.cardId
            )
            withStyle(
                style = SpanStyle(
                    color = MaterialTheme.colorScheme.primary,
                    fontWeight = FontWeight.Medium,
                    textDecoration = TextDecoration.Underline
                )
            ) {
                append(displayText)
            }
            pop()
            
            lastIndex = match.end
        }
        
        // Add remaining text
        if (lastIndex < markdownText.length) {
            append(markdownText.substring(lastIndex))
        }
    }
    
    Text(
        text = annotatedString,
        modifier = modifier.clickable { },
        style = MaterialTheme.typography.bodyLarge,
        lineHeight = 24.sp
    )
}

@Composable
fun CardPreviewDialog(
    card: CardEntity?,
    onDismiss: () -> Unit,
    onJumpToAnchor: () -> Unit,
    modifier: Modifier = Modifier
) {
    if (card != null) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = {
                Text("Card Preview")
            },
            text = {
                Column(
                    verticalArrangement = Arrangement.spacedBy(16.dp)
                ) {
                    // Front of card
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.primaryContainer
                        )
                    ) {
                        Column(
                            modifier = Modifier.padding(16.dp)
                        ) {
                            Text(
                                text = "Front",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.7f)
                            )
                            Spacer(modifier = Modifier.height(8.dp))
                            Text(
                                text = card.front,
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                        }
                    }
                    
                    // Back of card
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        colors = CardDefaults.cardColors(
                            containerColor = MaterialTheme.colorScheme.secondaryContainer
                        )
                    ) {
                        Column(
                            modifier = Modifier.padding(16.dp)
                        ) {
                            Text(
                                text = "Back",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSecondaryContainer.copy(alpha = 0.7f)
                            )
                            Spacer(modifier = Modifier.height(8.dp))
                            Text(
                                text = card.back,
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onSecondaryContainer
                            )
                        }
                    }
                    
                    // Card metadata
                    if (!card.tags.isNullOrBlank()) {
                        Text(
                            text = "Tags: ${card.tags}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = onJumpToAnchor) {
                    Text("Jump to Anchor")
                }
            },
            dismissButton = {
                TextButton(onClick = onDismiss) {
                    Text("Close")
                }
            }
        )
    }
}

@Composable
fun CardPickerDialog(
    cards: List<CardEntity>,
    onCardSelected: (CardEntity) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text("Select Card to Cite")
        },
        text = {
            Column(
                modifier = Modifier.heightIn(max = 400.dp)
            ) {
                if (cards.isEmpty()) {
                    Text(
                        text = "No cards available in this deck",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                } else {
                    cards.forEach { card ->
                        Card(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(vertical = 4.dp)
                                .clickable { onCardSelected(card) },
                            colors = CardDefaults.cardColors(
                                containerColor = MaterialTheme.colorScheme.surfaceVariant
                            )
                        ) {
                            Column(
                                modifier = Modifier.padding(12.dp)
                            ) {
                                Text(
                                    text = card.front,
                                    style = MaterialTheme.typography.bodyMedium,
                                    fontWeight = FontWeight.Medium
                                )
                                if (card.back.isNotEmpty()) {
                                    Text(
                                        text = card.back,
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                }
                                if (!card.tags.isNullOrBlank()) {
                                    Text(
                                        text = "Tags: ${card.tags}",
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                }
                            }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel")
            }
        }
    )
}
