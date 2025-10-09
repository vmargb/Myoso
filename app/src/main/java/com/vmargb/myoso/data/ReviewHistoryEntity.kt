package com.vmargb.myoso.data

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.Serializable

@Entity(
    tableName = "review_history",
    foreignKeys = [
        ForeignKey(
            entity = CardEntity::class,
            parentColumns = ["id"],
            childColumns = ["cardId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    indices = [
        Index(value = ["cardId"]),
        Index(value = ["reviewedAt"])
    ]
)
@Serializable
data class ReviewHistoryEntity(
    @PrimaryKey
    val id: String,
    val cardId: String,
    val reviewedAt: Long, // epoch ms
    val confidence: String, // "again", "hard", "good", "easy"
    val responseTimeMs: Long,
    val oldIntervalDays: Long,
    val newIntervalDays: Long
)
