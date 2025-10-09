package com.vmargb.myoso.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vmargb.myoso.io.BackupManager
import com.vmargb.myoso.io.DeckExporter
import com.vmargb.myoso.data.DeckMarkdownParser
import com.vmargb.myoso.data.DeckEntity
import com.vmargb.myoso.data.DeckRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import java.io.File

data class ImportExportUiState(
    val isLoading: Boolean = false,
    val availableDecks: List<DeckEntity> = emptyList(),
    val recentBackups: List<File> = emptyList(),
    val exportMessage: String = ""
)

class ImportExportViewModel(
    private val deckRepository: DeckRepository,
    private val backupManager: BackupManager,
    private val deckExporter: DeckExporter,
    private val deckMarkdownParser: DeckMarkdownParser
) : ViewModel() {

    private val _uiState = MutableStateFlow(ImportExportUiState())
    val uiState: StateFlow<ImportExportUiState> = _uiState

    fun loadDecksAndBackups() {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true)
            val decks = deckRepository.getAllDecks()
            val backupsDir = backupManager.getBackupDirectory()
            val backups = backupsDir.listFiles()?.toList() ?: emptyList()
            _uiState.value = _uiState.value.copy(isLoading = false, availableDecks = decks, recentBackups = backups)
        }
    }

    fun exportDecksToMarkdown(deckIds: List<String>, outputDir: File, onComplete: (Result<List<File>>) -> Unit) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, exportMessage = "Exporting decks...")
            val result = deckExporter.exportDecksToMarkdown(deckIds, outputDir)
            _uiState.value = _uiState.value.copy(isLoading = false, exportMessage = "")
            onComplete(result)
        }
    }

    fun exportBackup(file: File, onComplete: (Result<Unit>) -> Unit) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, exportMessage = "Exporting backup...")
            val result = backupManager.exportBackupToFile(file)
            _uiState.value = _uiState.value.copy(isLoading = false, exportMessage = "")
            onComplete(result)
        }
    }

    fun importBackup(file: File, onComplete: (Result<Unit>) -> Unit) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, exportMessage = "Importing backup...")
            val result = backupManager.importBackupFromFile(file)
            _uiState.value = _uiState.value.copy(isLoading = false, exportMessage = "")
            onComplete(result)
        }
    }

    fun importDeckMarkdown(filePath: String, onComplete: (Result<*>) -> Unit) {
        viewModelScope.launch {
            _uiState.value = _uiState.value.copy(isLoading = true, exportMessage = "Importing deck...")
            val result = deckMarkdownParser.parseAndImport(filePath)
            _uiState.value = _uiState.value.copy(isLoading = false, exportMessage = "")
            onComplete(Result.success(result))
        }
    }
}