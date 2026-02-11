package com.vmargb.myoso.data

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.Serializable

/*
 * everything is just a "Card"
 * pinned = null (standard spaced repetition)
 * 
 * pinned = "daily", "weekly":
 *   pinned q&a card, ignores all scheduling
 * 
 * pinned = "note"
 *  Note card that only uses 'front'
 *  ignores 'back' and all scheduling
 */


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
