package com.vmargb.myoso.data

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "decks")
data class DeckEntity(
    @PrimaryKey
    val id: String,
    val name: String,
    val description: String? = null,
    val createdAt: Long = System.currentTimeMillis(),
    val updatedAt: Long = System.currentTimeMillis()
)
