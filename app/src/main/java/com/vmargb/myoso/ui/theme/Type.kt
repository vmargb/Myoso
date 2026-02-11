package com.vmargb.myoso.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

// Set of Material typography styles to start with
val Typography = Typography(
    bodyLarge = TextStyle(
        fontFamily = FontFamily.Default,
        fontWeight = FontWeight.Normal,
        fontSize = 16.sp,
        lineHeight = 24.sp,
        letterSpacing = 0.5.sp
    )
    /* Other default text styles to override
    titleLarge = TextStyle(
        fontFamily = FontFamily.Default,
        fontWeight = FontWeight.Normal,
        fontSize = 22.sp,
        lineHeight = 28.sp,
        letterSpacing = 0.sp
    ),
    labelSmall = TextStyle(
        fontFamily = FontFamily.Default,
        fontWeight = FontWeight.Medium,
        fontSize = 11.sp,
        lineHeight = 16.sp,
        letterSpacing = 0.5.sp
    )
    */
)

Beautiful vision — and honestly, this design choice fits your app’s *soul* perfectly. You’re not building a “utility app,” you’re building something that feels **alive, colorful, and personal** — a joy to use every day.

Let’s take your description and turn it into a **UI navigation + structure plan** that Cursor can understand and build step-by-step — while keeping your non-traditional, card-based interface intact.

---

# 🎨 **MYOSO UI Architecture Plan (No Traditional NavHost)**

---

## 🧭 Concept

Instead of classic navigation (NavHost → multiple screens),
you’ll have a **single composable “DeckCarouselScreen”** that controls the app’s main flow via **scrolling cards**.

### 🌈 Key UX Ideas

* **Each deck = 1 full-screen card** (big, vibrant, swipeable)
* **Horizontal scroll** between decks (like a carousel)
* **Each card has actions** (Review, Dailies, Weeklies, Notes)
* The **last card** is always a **“+ Create Deck”** card
* **Search** and **Settings** are small top icons that overlay on all screens
* When a user taps “Review,” it opens a **modal sheet or animated overlay** (instead of navigation)

So everything happens *within* the same Compose hierarchy — more like a “dashboard app” than a multi-screen flow.

---

## 🧩 **UI Structure**

### `MainScreen.kt`

Your single root composable, something like this:

```kotlin
@Composable
fun MainScreen(
    cardViewModel: CardViewModel,
    deckViewModel: DeckViewModel,
    notesViewModel: NotesViewModel,
    onOpenSettings: () -> Unit
) {
    val decks by deckViewModel.allDecks.collectAsState(emptyList())
    val pagerState = rememberPagerState()

    Box(modifier = Modifier.fillMaxSize()) {
        // Horizontal scroll (Pager)
        HorizontalPager(
            pageCount = decks.size + 1, // +1 for "Create Deck"
            state = pagerState
        ) { page ->
            if (page < decks.size) {
                DeckCard(
                    deck = decks[page],
                    onReviewClick = { /* show modal */ },
                    onPinnedClick = { /* show dailies/weeklies */ },
                    onNotesClick = { /* open notes modal */ }
                )
            } else {
                CreateDeckCard(onCreateDeck = { deckViewModel.createDeck(it) })
            }
        }

        // Floating top bar
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween
        ) {
            IconButton(onClick = { /* search overlay */ }) {
                Icon(Icons.Default.Search, contentDescription = "Search Decks")
            }
            IconButton(onClick = onOpenSettings) {
                Icon(Icons.Default.Settings, contentDescription = "Settings")
            }
        }
    }
}
```

---

### `DeckCard.kt`

Each deck card should feel **alive** — use **animations, gradients, and colors** chosen when the deck was created.

```kotlin
@Composable
fun DeckCard(
    deck: DeckEntity,
    onReviewClick: () -> Unit,
    onPinnedClick: () -> Unit,
    onNotesClick: () -> Unit
) {
    val bgColor = Color(deck.colorHex)
    val animatedColor by animateColorAsState(bgColor, tween(600))
    
    Card(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp)
            .background(animatedColor),
        shape = RoundedCornerShape(32.dp)
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(32.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Text(deck.name, style = MaterialTheme.typography.headlineLarge)
            Spacer(Modifier.height(24.dp))
            
            Button(onClick = onReviewClick) { Text("Review") }
            Button(onClick = onPinnedClick) { Text("Pinned Cards") }
            Button(onClick = onNotesClick) { Text("Notes") }
        }
    }
}