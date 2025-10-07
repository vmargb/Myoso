package com.vmargb.myoso.io

import android.content.Context
import com.vmargb.myoso.data.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.encodeToString
import kotlinx.serialization.decodeFromString
import java.io.File
import java.io.FileOutputStream
import java.io.FileInputStream
import java.text.SimpleDateFormat
import java.util.*

@Serializable
data class BackupData(
    val version: String = "1.0",
    val timestamp: Long = System.currentTimeMillis(),
    val decks: List<DeckEntity>,
    val cards: List<CardEntity>,
    val notes: List<NoteEntity>,
    val citations: List<CitationEntity>,
    val reviewHistory: List<ReviewHistoryEntity>
)

class BackupManager(
    private val context: Context,
    private val deckDao: DeckDao,
    private val cardDao: CardDao,
    private val noteDao: NoteDao,
    private val citationDao: CitationDao,
    private val reviewHistoryDao: ReviewHistoryDao
) {
    
    private val json = Json {
        prettyPrint = true
        ignoreUnknownKeys = true
    }
    
    /**
     * Creates a full backup of all data
     */
    suspend fun createBackup(): BackupData = withContext(Dispatchers.IO) {
        val decks = deckDao.getAllDecks()
        val cards = mutableListOf<CardEntity>()
        val notes = mutableListOf<NoteEntity>()
        val citations = mutableListOf<CitationEntity>()
        val reviewHistory = mutableListOf<ReviewHistoryEntity>()
        
        // Get all cards, notes, citations, and review history
        decks.forEach { deck ->
            cards.addAll(cardDao.getCardsByDeck(deck.id))
            notes.addAll(noteDao.getNotesByDeck(deck.id))
        }
        
        // Get all citations and review history
        cards.forEach { card ->
            citations.addAll(citationDao.getCitationsByCard(card.id))
            reviewHistory.addAll(reviewHistoryDao.getReviewHistoryByCard(card.id))
        }
        
        BackupData(
            version = "1.0",
            timestamp = System.currentTimeMillis(),
            decks = decks,
            cards = cards,
            notes = notes,
            citations = citations,
            reviewHistory = reviewHistory
        )
    }
    
    /**
     * Exports backup to a JSON file
     */
    suspend fun exportBackupToFile(file: File): Result<Unit> = withContext(Dispatchers.IO) {
        try {
            val backupData = createBackup()
            val jsonString = json.encodeToString(backupData)
            
            file.writeText(jsonString)
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
    
    /**
     * Imports backup from a JSON file
     */
    suspend fun importBackupFromFile(file: File): Result<Unit> = withContext(Dispatchers.IO) {
        try {
            val jsonString = file.readText()
            val backupData = json.decodeFromString<BackupData>(jsonString)
            
            // Clear existing data
            clearAllData()
            
            // Import data in correct order (respecting foreign keys)
            deckDao.insertDecks(backupData.decks)
            cardDao.insertCards(backupData.cards)
            noteDao.insertNotes(backupData.notes)
            citationDao.insertCitations(backupData.citations)
            reviewHistoryDao.insertReviewHistories(backupData.reviewHistory)
            
            Result.success(Unit)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
    
    /**
     * Gets backup file info without importing
     */
    suspend fun getBackupInfo(file: File): Result<BackupInfo> = withContext(Dispatchers.IO) {
        try {
            val jsonString = file.readText()
            val backupData = json.decodeFromString<BackupData>(jsonString)
            
            val info = BackupInfo(
                version = backupData.version,
                timestamp = backupData.timestamp,
                deckCount = backupData.decks.size,
                cardCount = backupData.cards.size,
                noteCount = backupData.notes.size,
                citationCount = backupData.citations.size,
                reviewCount = backupData.reviewHistory.size
            )
            
            Result.success(info)
        } catch (e: Exception) {
            Result.failure(e)
        }
    }
    
    /**
     * Clears all data from the database
     */
    private suspend fun clearAllData() {
        // Clear in reverse order to respect foreign key constraints
        reviewHistoryDao.deleteAllReviewHistory()
        citationDao.deleteAllCitations()
        noteDao.deleteAllNotes()
        cardDao.deleteAllCards()
        deckDao.deleteAllDecks()
    }
    
    /**
     * Generates a backup filename with timestamp
     */
    fun generateBackupFilename(): String {
        val dateFormat = SimpleDateFormat("yyyyMMdd_HHmmss", Locale.getDefault())
        val timestamp = dateFormat.format(Date())
        return "myoso_backup_$timestamp.json"
    }
    
    /**
     * Gets the backup directory
     */
    fun getBackupDirectory(): File {
        val backupDir = File(context.getExternalFilesDir(null), "backups")
        if (!backupDir.exists()) {
            backupDir.mkdirs()
        }
        return backupDir
    }
}

data class BackupInfo(
    val version: String,
    val timestamp: Long,
    val deckCount: Int,
    val cardCount: Int,
    val noteCount: Int,
    val citationCount: Int,
    val reviewCount: Int
) {
    val formattedDate: String
        get() {
            val dateFormat = SimpleDateFormat("MMM dd, yyyy HH:mm", Locale.getDefault())
            return dateFormat.format(Date(timestamp))
        }
}
