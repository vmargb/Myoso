package com.vmargb.myoso.data

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.Serializable

@Entity(
    tableName = "notes",
    indices = [Index(value = ["deckId"])]
)
@Serializable
data class NoteEntity(
    @PrimaryKey
    val id: String,
    val deckId: String,
    val markdownBody: String,
    val updatedAt: Long = System.currentTimeMillis()
)
