package com.vmargb.myoso.ui.home

import android.graphics.Color as AndroidColor
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.graphics.toColorInt

private val sampleColors = listOf(
    "#FF8A80", "#FFD180", "#FFF59D", "#C8E6C9", "#80D8FF", "#B39DDB", "#FFCC80"
)

@Composable
fun CreateDeckCard(
    modifier: Modifier = Modifier,
    onCreateDeck: (name: String, colorHex: String) -> Unit
) {
    var name by remember { mutableStateOf("") }
    var selectedColor by remember { mutableStateOf(sampleColors.first()) }

    val bg = try {
        Color(selectedColor.toColorInt())
    } catch (_: Exception) {
        Color(0xFF90CAF9)
    }

    val gradient = Brush.verticalGradient(listOf(bg, bg.copy(alpha = 0.9f)))

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
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.SpaceBetween
            ) {
                Column {
                    Text(
                        text = "Create a new deck",
                        fontSize = 32.sp,
                        color = Color.White
                    )
                    Spacer(modifier = Modifier.height(8.dp))
                    OutlinedTextField(
                        value = name,
                        onValueChange = { name = it },
                        label = { Text("Deck name") },
                        modifier = Modifier.fillMaxWidth()
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    Text(text = "Pick a color:", color = Color.White)
                    Spacer(modifier = Modifier.height(8.dp))
                    Row(modifier = Modifier.fillMaxWidth()) {
                        sampleColors.forEach { hex ->
                            val col = try {
                                Color(hex.toColorInt())
                            } catch (_: Exception) {
                                Color.Gray
                            }
                            Box(
                                modifier = Modifier
                                    .size(40.dp)
                                    .padding(4.dp)
                                    .background(col, shape = RoundedCornerShape(8.dp))
                                    .clickable { selectedColor = hex }
                            )
                        }
                    }
                }

                Button(
                    onClick = { if (name.isNotBlank()) onCreateDeck(name.trim(), selectedColor) },
                    modifier = Modifier.align(Alignment.End)
                ) {
                    Text(text = "Create")
                }
            }
        }
    }
}