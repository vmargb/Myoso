package com.vmargb.myoso.data

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index
import androidx.room.PrimaryKey
import kotlinx.serialization.Serializable

@Entity(
    tableName = "citations",
    foreignKeys = [
        ForeignKey(
            entity = NoteEntity::class,
            parentColumns = ["id"], // primary key columns
            childColumns = ["noteId"], // foreign key columns
            onDelete = ForeignKey.CASCADE
        ),
        ForeignKey(
            entity = CardEntity::class,
            parentColumns = ["id"],
            childColumns = ["cardId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    indices = [
        Index(value = ["noteId"]),
        Index(value = ["cardId"])
    ]
)
@Serializable
data class CitationEntity(
    @PrimaryKey
    val id: String,
    val noteId: String,
    val cardId: String,
    val anchorText: String,
    val startIndex: Int,
    val endIndex: Int
)
