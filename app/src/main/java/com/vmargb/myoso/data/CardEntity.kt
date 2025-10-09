package com.vmargb.myoso.data

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.Serializable


@Entity(
    tableName = "cards",
    indices = [Index(value = ["deckId"])]
)
@Serializable
data class CardEntity(
    @PrimaryKey
    val id: String,
    val deckId: String,
    val front: String,
    val back: String,
    val tags: String? = null, // CSV format
    val isReversible: Boolean = false,
    val pinned: String? = null, // "daily" | "weekly" | null
    val intervalDays: Long = 1,
    val easeFactor: Double = 2.3,
    val reviewCount: Int = 0,
    val consecutiveFails: Int = 0,
    val lastReviewedAt: Long = 0L, // epoch ms
    val nextDueAt: Long = 0L, // epoch ms
    val createdAt: Long = System.currentTimeMillis()
)
