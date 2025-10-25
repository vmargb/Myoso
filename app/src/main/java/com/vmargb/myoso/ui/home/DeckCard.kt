package com.vmargb.myoso.ui.home

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.vmargb.myoso.data.DeckEntity
import androidx.core.graphics.toColorInt
import androidx.compose.material3.ButtonDefaults // button styling

private fun parseHexColor(hex: String?, fallback: Color = Color(0xFF7C4DFF)): Color {
    if (hex.isNullOrBlank()) return fallback
    return try {
        Color(hex.toColorInt())
    } catch (_: Exception) {
        fallback
    }
}

@Composable
fun DeckCard(
    deck: DeckEntity,
    modifier: Modifier = Modifier,
    onReview: () -> Unit = {},
    onPinned: () -> Unit = {},
    onNotes: () -> Unit = {}
) {
    val baseColor = parseHexColor(deck.color)
    val animatedBg by animateColorAsState(targetValue = baseColor)
    // use a fixed white color for text/buttons that guarantees visibility over the colored gradient
    val contentColor = Color.White

    // gradient background
    val gradient = Brush.verticalGradient(
        colors = listOf(animatedBg, animatedBg.copy(alpha = 0.88f))
    )

    Card(
        shape = RoundedCornerShape(32.dp),
        modifier = modifier
            .padding(16.dp)
            .fillMaxSize()
    ) {
        Box(
            modifier = Modifier
                .background(gradient)
                .fillMaxSize()
                .padding(24.dp)
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize(),
                verticalArrangement = Arrangement.SpaceBetween
            ) {
                // Deck title / subtitle
                Column {
                    Text(
                        text = deck.name,
                        style = MaterialTheme.typography.headlineLarge.copy(fontSize = 36.sp, fontWeight = FontWeight.ExtraBold),
                        color = contentColor
                    )
                    Spacer(modifier = Modifier.height(6.dp))
                    Text(
                        text = deck.description ?: "",
                        style = MaterialTheme.typography.bodyLarge,
                        color = contentColor.copy(alpha = 0.9f)
                    )
                }

                // Actions row
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        // increased bottom padding for better spacing from the edge
                        .padding(bottom = 20.dp),
                    // changed from SpaceEvenly to Start for a left-aligned
                    horizontalArrangement = Arrangement.Start,
                    verticalAlignment = Alignment.CenterVertically
                ) {

                    TextButton(
                        onClick = onReview,
                        colors = ButtonDefaults.textButtonColors(contentColor = contentColor)
                    ) {
                        Text(
                            text = "Review",
                            fontWeight = FontWeight.SemiBold
                        )
                    }
                    Spacer(modifier = Modifier.width(16.dp))

                    TextButton(
                        onClick = onPinned,
                        colors = ButtonDefaults.textButtonColors(contentColor = contentColor)
                    ) {
                        Text(
                            text = "Pinned",
                            fontWeight = FontWeight.SemiBold
                        )
                    }
                    Spacer(modifier = Modifier.width(16.dp))

                    TextButton(
                        onClick = onNotes,
                        colors = ButtonDefaults.textButtonColors(contentColor = contentColor)
                    ) {
                        Text(
                            text = "Notes",
                            fontWeight = FontWeight.SemiBold
                        )
                    }
                }
            }
        }
    }
}