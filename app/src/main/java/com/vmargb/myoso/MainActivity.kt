package com.vmargb.myoso

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController

import com.vmargb.myoso.ui.home.MainScreen
import com.vmargb.myoso.ui.theme.MyosoTheme
import com.vmargb.myoso.data.DeckEntity
import dagger.hilt.android.AndroidEntryPoint

@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MyosoTheme {
                MyApp()
            }
        }
    }
}

@Composable
fun MyApp() {
    // TEMPORARY MOCK DATA
    val mockDecks = listOf(
        DeckEntity(
            id = "math_id",
            name = "Calculus Review",
            description = "Key concepts for final exam.",
            color = "#FF5733" // Red
        ),
        DeckEntity(
            id = "lang_id",
            name = "Japanese Kanji N3",
            description = "Daily practice for intermediate level.",
            color = "#33FF57" // Green
        ),
        DeckEntity(
            id = "spanish_id",
            name = "Spanish",
            description = "Spanish vocab practice",
            color = "#3357FF" // Blue
        ),
    )

    val navController = rememberNavController()
    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        NavHost(
            navController = navController,
            startDestination = "main_screen"
        ) {
            composable("main_screen") {
                MainScreen(
                    decks = mockDecks, // ⚠️ PASS TEMPORARY MOCK DATA

                    onCreateDeck = { name, colorHex ->
                        println("MOCK: Creating deck: $name with color $colorHex")
                    },
                    onReview = { deckId -> println("MOCK: Review deck: $deckId") },
                    onPinned = { deckId -> println("MOCK: Pinned deck: $deckId") },
                    onNotes = { deckId -> println("MOCK: Notes for deck: $deckId") },
                    onSearchClick = { println("MOCK: Search clicked") },
                    onSettingsClick = { println("MOCK: Settings clicked") }
                )
            }
        }
    }
}

@Preview(showBackground = true)
@Composable
fun MyAppPreview() {
    MyosoTheme {
        MyApp()
    }
}