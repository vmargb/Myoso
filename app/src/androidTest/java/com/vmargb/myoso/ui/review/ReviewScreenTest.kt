package com.vmargb.myoso.ui.review

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.vmargb.myoso.data.CardEntity
import com.vmargb.myoso.data.DeckEntity
import com.vmargb.myoso.scheduling.Confidence
import com.vmargb.myoso.scheduling.ReviewResult
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ReviewScreenTest {
    
    @get:Rule
    val composeTestRule = createComposeRule()
    
    @Test
    fun reviewScreen_displaysCardCorrectly() {
        val testCard = createTestCard(
            id = "card1",
            deckId = "deck1",
            front = "Hello",
            back = "Hola"
        )
        
        composeTestRule.setContent {
            CardViewComposable(
                card = testCard,
                onReviewComplete = { },
                useResponseTime = true
            )
        }
        
        // Verify front of card is displayed
        composeTestRule.onNodeWithText("Hello").assertIsDisplayed()
        composeTestRule.onNodeWithText("Front").assertIsDisplayed()
        
        // Tap to flip card
        composeTestRule.onNodeWithText("Hello").performClick()
        
        // Verify back of card is displayed
        composeTestRule.onNodeWithText("Hola").assertIsDisplayed()
        composeTestRule.onNodeWithText("Back").assertIsDisplayed()
        
        // Verify confidence buttons are shown
        composeTestRule.onNodeWithText("How well did you know this?").assertIsDisplayed()
        composeTestRule.onNodeWithText("Didn't know").assertIsDisplayed()
        composeTestRule.onNodeWithText("Barely").assertIsDisplayed()
        composeTestRule.onNodeWithText("Knew it").assertIsDisplayed()
        composeTestRule.onNodeWithText("Knew instantly").assertIsDisplayed()
    }
    
    @Test
    fun reviewScreen_confidenceButtonInteraction() {
        var reviewResult: ReviewResult? = null
        
        val testCard = createTestCard(
            id = "card1",
            deckId = "deck1",
            front = "Test",
            back = "Answer"
        )
        
        composeTestRule.setContent {
            CardViewComposable(
                card = testCard,
                onReviewComplete = { result -> reviewResult = result },
                useResponseTime = true
            )
        }
        
        // Flip card to show confidence buttons
        composeTestRule.onNodeWithText("Test").performClick()
        
        // Click "Knew it" button
        composeTestRule.onNodeWithText("Knew it").performClick()
        
        // Verify callback was called with correct confidence
        assert(reviewResult != null)
        assert(reviewResult!!.confidence == Confidence.KNEW)
    }
    
    @Test
    fun reviewScreen_deckSelection() {
        val testDecks = listOf(
            createTestDeck("deck1", "Spanish Basics"),
            createTestDeck("deck2", "French Basics"),
            createTestDeck("deck3", "German Basics")
        )
        
        composeTestRule.setContent {
            DeckSelectionView(
                decks = testDecks,
                selectedDecks = setOf("deck1"),
                onDeckSelected = { },
                onCreateSession = { }
            )
        }
        
        // Verify deck selection UI is displayed
        composeTestRule.onNodeWithText("Select Decks to Review").assertIsDisplayed()
        composeTestRule.onNodeWithText("Spanish Basics").assertIsDisplayed()
        composeTestRule.onNodeWithText("French Basics").assertIsDisplayed()
        composeTestRule.onNodeWithText("German Basics").assertIsDisplayed()
        
        // Verify first deck is selected
        composeTestRule.onNodeWithText("Spanish Basics").assertIsSelected()
    }
    
    @Test
    fun reviewScreen_sessionInfo() {
        val testSessionResult = createTestSessionResult()
        
        composeTestRule.setContent {
            SessionInfoView(
                sessionResult = testSessionResult,
                onStartReview = { }
            )
        }
        
        // Verify session info is displayed
        composeTestRule.onNodeWithText("Test Session").assertIsDisplayed()
        composeTestRule.onNodeWithText("Review all cards from selected decks").assertIsDisplayed()
        composeTestRule.onNodeWithText("Total Cards").assertIsDisplayed()
        composeTestRule.onNodeWithText("Due Cards").assertIsDisplayed()
        composeTestRule.onNodeWithText("Pinned Cards").assertIsDisplayed()
        
        // Verify statistics
        composeTestRule.onNodeWithText("10").assertIsDisplayed() // Total cards
        composeTestRule.onNodeWithText("5").assertIsDisplayed()  // Due cards
        composeTestRule.onNodeWithText("2").assertIsDisplayed()  // Pinned cards
        
        // Verify start review button
        composeTestRule.onNodeWithText("Start Review").assertIsDisplayed()
    }
    
    @Test
    fun reviewScreen_sessionReview() {
        val testCard = createTestCard("card1", "deck1", "Question", "Answer")
        val testSessionSpec = createTestSessionSpec()
        
        composeTestRule.setContent {
            SessionReviewView(
                currentCard = testCard,
                cardsRemaining = 5,
                sessionSpec = testSessionSpec,
                onReviewComplete = { },
                onFinishReview = { }
            )
        }
        
        // Verify session review UI
        composeTestRule.onNodeWithText("Test Session").assertIsDisplayed()
        composeTestRule.onNodeWithText("Cards Remaining").assertIsDisplayed()
        composeTestRule.onNodeWithText("5").assertIsDisplayed()
        
        // Verify card is displayed
        composeTestRule.onNodeWithText("Question").assertIsDisplayed()
    }
    
    @Test
    fun reviewScreen_sessionComplete() {
        composeTestRule.setContent {
            SessionReviewView(
                currentCard = null,
                cardsRemaining = 0,
                sessionSpec = createTestSessionSpec(),
                onReviewComplete = { },
                onFinishReview = { }
            )
        }
        
        // Verify completion UI
        composeTestRule.onNodeWithText("🎉").assertIsDisplayed()
        composeTestRule.onNodeWithText("Session Complete!").assertIsDisplayed()
        composeTestRule.onNodeWithText("Great job! You've completed all cards in this session.").assertIsDisplayed()
        composeTestRule.onNodeWithText("Finish Session").assertIsDisplayed()
    }
    
    @Test
    fun reviewScreen_emptySession() {
        val emptySessionResult = createTestSessionResult(cardCount = 0)
        
        composeTestRule.setContent {
            SessionInfoView(
                sessionResult = emptySessionResult,
                onStartReview = { }
            )
        }
        
        // Verify empty session warning
        composeTestRule.onNodeWithText("No cards found for this session").assertIsDisplayed()
        
        // Verify start review button is disabled
        composeTestRule.onNodeWithText("Start Review").assertIsNotEnabled()
    }
    
    private fun createTestCard(
        id: String,
        deckId: String,
        front: String,
        back: String
    ) = CardEntity(
        id = id,
        deckId = deckId,
        front = front,
        back = back,
        isReversible = false,
        intervalDays = 1,
        easeFactor = 2.3,
        reviewCount = 0,
        consecutiveFails = 0,
        lastReviewedAt = 0L,
        nextDueAt = System.currentTimeMillis(),
        createdAt = System.currentTimeMillis()
    )
    
    private fun createTestDeck(id: String, name: String) = DeckEntity(
        id = id,
        name = name,
        description = "Test description",
        createdAt = System.currentTimeMillis(),
        updatedAt = System.currentTimeMillis()
    )
    
    private fun createTestSessionSpec() = com.vmargb.myoso.session.SessionSpec(
        sessionId = "session1",
        name = "Test Session",
        deckIds = listOf("deck1", "deck2"),
        sessionType = com.vmargb.myoso.session.SessionType.ALL_CARDS
    )
    
    private fun createTestSessionResult(cardCount: Int = 10) = com.vmargb.myoso.session.SessionResult(
        sessionSpec = createTestSessionSpec(),
        cards = emptyList(), // We don't need actual cards for UI tests
        totalCards = cardCount,
        dueCards = cardCount / 2,
        pinnedCards = cardCount / 5
    )
}
