package com.vmargb.myoso.ui.home

import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.vmargb.myoso.data.DeckEntity

@Composable
fun MainScreen(
    decks: List<DeckEntity>,
    modifier: Modifier = Modifier,
    onReview: (deckId: String) -> Unit = {},
    onPinned: (deckId: String) -> Unit = {},
    onNotes: (deckId: String) -> Unit = {},
    onCreateDeck: (name: String, colorHex: String) -> Unit,
    onSearchClick: () -> Unit = {},
    onSettingsClick: () -> Unit = {}
) {
    val showCreateAtEnd = true
    val pages = if (decks.isEmpty()) 1 else decks.size + if (showCreateAtEnd) 1 else 0
    val pagerState = rememberPagerState(initialPage = 0, pageCount = { pages })

    // apply safe drawing padding to the entire Surface
    Surface(
        modifier = modifier
            .fillMaxSize()
            // applies system window padding (status bar, navigation bar, etc.)
            .padding(WindowInsets.safeDrawing.asPaddingValues())
    ) {
        Column(modifier = Modifier.fillMaxSize()) {

            // custom Header Row for Search and Settings
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.End,
                verticalAlignment = Alignment.CenterVertically
            ) {

                // Search Button
                IconButton(onClick = onSearchClick) {
                    Icon(
                        Icons.Default.Search,
                        contentDescription = "Search",
                        modifier = Modifier.size(28.dp)
                    )
                }

                Spacer(modifier = Modifier.width(8.dp))

                // Settings Button
                IconButton(onClick = onSettingsClick) {
                    Icon(
                        Icons.Default.Settings,
                        contentDescription = "Settings",
                        modifier = Modifier.size(28.dp)
                    )
                }
            }

            // horizontal Pager
            HorizontalPager(
                state = pagerState,
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f)
            ) { page ->
                // ... (content remains the same)
                if (decks.isEmpty()) {
                    CreateDeckCard(
                        modifier = Modifier.fillMaxSize(),
                        onCreateDeck = onCreateDeck
                    )
                } else {
                    if (page < decks.size) {
                        val deck = decks[page]
                        DeckCard(
                            deck = deck,
                            modifier = Modifier.fillMaxSize(),
                            onReview = { onReview(deck.id) },
                            onPinned = { onPinned(deck.id) },
                            onNotes = { onNotes(deck.id) }
                        )
                    } else {
                        CreateDeckCard(
                            modifier = Modifier.fillMaxSize(),
                            onCreateDeck = onCreateDeck
                        )
                    }
                }
            }
        }
    }
}