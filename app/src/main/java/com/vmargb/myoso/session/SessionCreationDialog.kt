package com.vmargb.myoso.session

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import com.vmargb.myoso.data.DeckEntity

@Composable
fun SessionCreationDialog(
    decks: List<DeckEntity>,
    onSessionCreated: (SessionSpec) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier
) {
    var sessionName by remember { mutableStateOf("") }
    var selectedDecks by remember { mutableStateOf(setOf<String>()) }
    var sessionType by remember { mutableStateOf(SessionType.DUE_CARDS) }
    var pinnedFilter by remember { mutableStateOf(PinnedFilter.DAILY) }
    var tagFilter by remember { mutableStateOf("") }
    var showAdvancedOptions by remember { mutableStateOf(false) }
    
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text("Create Review Session")
        },
        text = {
            LazyColumn(
                modifier = Modifier.heightIn(max = 500.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                // Session Name
                item {
                    OutlinedTextField(
                        value = sessionName,
                        onValueChange = { sessionName = it },
                        label = { Text("Session Name") },
                        placeholder = { Text("Enter session name") },
                        modifier = Modifier.fillMaxWidth(),
                        singleLine = true
                    )
                }
                
                // Deck Selection
                item {
                    Text(
                        text = "Select Decks",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Medium
                    )
                }
                
                items(decks) { deck ->
                    DeckSelectionItem(
                        deck = deck,
                        isSelected = selectedDecks.contains(deck.id),
                        onSelected = { deckId ->
                            selectedDecks = if (selectedDecks.contains(deckId)) {
                                selectedDecks - deckId
                            } else {
                                selectedDecks + deckId
                            }
                        }
                    )
                }
                
                // Session Type Selection
                item {
                    Text(
                        text = "Session Type",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Medium
                    )
                }
                
                item {
                    Column(
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        SessionType.values().forEach { type ->
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .selectable(
                                        selected = sessionType == type,
                                        onClick = { sessionType = type }
                                    )
                                    .padding(vertical = 4.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                RadioButton(
                                    selected = sessionType == type,
                                    onClick = { sessionType = type }
                                )
                                Spacer(modifier = Modifier.width(8.dp))
                                Column {
                                    Text(
                                        text = getSessionTypeTitle(type),
                                        style = MaterialTheme.typography.bodyLarge
                                    )
                                    Text(
                                        text = getSessionTypeDescription(type),
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant
                                    )
                                }
                            }
                        }
                    }
                }
                
                // Advanced Options
                item {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            text = "Advanced Options",
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Medium
                        )
                        Switch(
                            checked = showAdvancedOptions,
                            onCheckedChange = { showAdvancedOptions = it }
                        )
                    }
                }
                
                // Pinned Filter Options
                if (showAdvancedOptions && sessionType == SessionType.PINNED_ONLY) {
                    item {
                        Text(
                            text = "Pinned Filter",
                            style = MaterialTheme.typography.titleSmall
                        )
                        Column(
                            verticalArrangement = Arrangement.spacedBy(4.dp)
                        ) {
                            PinnedFilter.values().forEach { filter ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .selectable(
                                            selected = pinnedFilter == filter,
                                            onClick = { pinnedFilter = filter }
                                        )
                                        .padding(vertical = 2.dp),
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    RadioButton(
                                        selected = pinnedFilter == filter,
                                        onClick = { pinnedFilter = filter }
                                    )
                                    Spacer(modifier = Modifier.width(8.dp))
                                    Text(
                                        text = getPinnedFilterTitle(filter),
                                        style = MaterialTheme.typography.bodyMedium
                                    )
                                }
                            }
                        }
                    }
                }
                
                // Tag Filter
                if (showAdvancedOptions && sessionType == SessionType.TAG_FILTER) {
                    item {
                        OutlinedTextField(
                            value = tagFilter,
                            onValueChange = { tagFilter = it },
                            label = { Text("Tag Filter") },
                            placeholder = { Text("Enter tag to filter by") },
                            modifier = Modifier.fillMaxWidth(),
                            singleLine = true,
                            keyboardOptions = KeyboardOptions(
                                capitalization = KeyboardCapitalization.None
                            )
                        )
                    }
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    val sessionSpec = SessionSpec(
                        sessionId = "session_${System.currentTimeMillis()}",
                        name = sessionName.ifBlank { getDefaultSessionName(sessionType) },
                        deckIds = selectedDecks.toList(),
                        sessionType = sessionType,
                        pinnedFilter = if (sessionType == SessionType.PINNED_ONLY) pinnedFilter else null,
                        tagFilter = if (sessionType == SessionType.TAG_FILTER) tagFilter.ifBlank { null } else null
                    )
                    onSessionCreated(sessionSpec)
                },
                enabled = selectedDecks.isNotEmpty()
            ) {
                Text("Create Session")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel")
            }
        }
    )
}

@Composable
private fun DeckSelectionItem(
    deck: DeckEntity,
    isSelected: Boolean,
    onSelected: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .selectable(
                selected = isSelected,
                onClick = { onSelected(deck.id) }
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
                .padding(12.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Checkbox(
                checked = isSelected,
                onCheckedChange = { onSelected(deck.id) }
            )
            
            Spacer(modifier = Modifier.width(12.dp))
            
            Column(
                modifier = Modifier.weight(1f)
            ) {
                Text(
                    text = deck.name,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium
                )
                
                if (!deck.description.isNullOrBlank()) {
                    Text(
                        text = deck.description,
                        style = MaterialTheme.typography.bodySmall,
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

private fun getSessionTypeTitle(type: SessionType): String {
    return when (type) {
        SessionType.ALL_CARDS -> "All Cards"
        SessionType.DUE_CARDS -> "Due Cards"
        SessionType.PINNED_ONLY -> "Pinned Cards"
        SessionType.TAG_FILTER -> "Tag Filter"
    }
}

private fun getSessionTypeDescription(type: SessionType): String {
    return when (type) {
        SessionType.ALL_CARDS -> "Review all cards from selected decks"
        SessionType.DUE_CARDS -> "Review only cards that are due for review"
        SessionType.PINNED_ONLY -> "Review only pinned cards (daily/weekly)"
        SessionType.TAG_FILTER -> "Review cards with specific tags"
    }
}

private fun getPinnedFilterTitle(filter: PinnedFilter): String {
    return when (filter) {
        PinnedFilter.DAILY -> "Daily Pinned Cards"
        PinnedFilter.WEEKLY -> "Weekly Pinned Cards"
        PinnedFilter.ALL_PINNED -> "All Pinned Cards"
    }
}

private fun getDefaultSessionName(sessionType: SessionType): String {
    return when (sessionType) {
        SessionType.ALL_CARDS -> "All Cards Session"
        SessionType.DUE_CARDS -> "Due Cards Session"
        SessionType.PINNED_ONLY -> "Pinned Cards Session"
        SessionType.TAG_FILTER -> "Tag Filter Session"
    }
}
