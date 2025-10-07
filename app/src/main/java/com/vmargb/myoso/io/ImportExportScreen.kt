package com.vmargb.myoso.io

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.vmargb.myoso.data.*
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ImportExportScreen(
    onNavigateBack: () -> Unit,
    modifier: Modifier = Modifier
) {
    val viewModel: ImportExportViewModel = viewModel()
    val uiState by viewModel.uiState.collectAsState()
    val scope = rememberCoroutineScope()
    
    LaunchedEffect(Unit) {
        viewModel.loadDecks()
    }
    
    Column(
        modifier = modifier.fillMaxSize()
    ) {
        // Top App Bar
        TopAppBar(
            title = { Text("Import & Export") },
            navigationIcon = {
                IconButton(onClick = onNavigateBack) {
                    Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                }
            }
        )
        
        // Content
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Export Section
            item {
                SectionCard(
                    title = "Export",
                    icon = Icons.Default.Upload,
                    content = {
                        ExportSection(
                            onExportBackup = {
                                scope.launch {
                                    viewModel.showBackupExportDialog()
                                }
                            },
                            onExportDeck = {
                                scope.launch {
                                    viewModel.showDeckExportDialog()
                                }
                            }
                        )
                    }
                )
            }
            
            // Import Section
            item {
                SectionCard(
                    title = "Import",
                    icon = Icons.Default.Download,
                    content = {
                        ImportSection(
                            onImportBackup = {
                                scope.launch {
                                    viewModel.showBackupImportDialog()
                                }
                            },
                            onImportDeck = {
                                scope.launch {
                                    viewModel.showDeckImportDialog()
                                }
                            }
                        )
                    }
                )
            }
            
            // Backup Management Section
            item {
                SectionCard(
                    title = "Backup Management",
                    icon = Icons.Default.Backup,
                    content = {
                        BackupManagementSection(
                            onCreateBackup = {
                                scope.launch {
                                    viewModel.createBackup()
                                }
                            },
                            onRestoreBackup = {
                                scope.launch {
                                    viewModel.showBackupRestoreDialog()
                                }
                            }
                        )
                    }
                )
            }
            
            // Recent Backups
            if (uiState.recentBackups.isNotEmpty()) {
                item {
                    SectionCard(
                        title = "Recent Backups",
                        icon = Icons.Default.History,
                        content = {
                            RecentBackupsSection(
                                backups = uiState.recentBackups,
                                onRestoreBackup = { backupFile ->
                                    scope.launch {
                                        viewModel.restoreBackup(backupFile)
                                    }
                                }
                            )
                        }
                    )
                )
            }
        }
    }
    
    // Dialogs
    if (uiState.showBackupExportDialog) {
        FileChooserDialog(
            title = "Export Backup",
            fileType = FileType.BACKUP,
            onFileSelected = { file ->
                scope.launch {
                    viewModel.exportBackup(file)
                }
            },
            onDismiss = {
                viewModel.hideBackupExportDialog()
            }
        )
    }
    
    if (uiState.showDeckExportDialog) {
        DeckSelectionDialog(
            decks = uiState.availableDecks,
            selectedDecks = uiState.selectedDecks,
            onDeckSelected = { deckId ->
                viewModel.toggleDeckSelection(deckId)
            },
            onConfirm = {
                scope.launch {
                    viewModel.showDeckExportFileDialog()
                }
            },
            onDismiss = {
                viewModel.hideDeckExportDialog()
            }
        )
    }
    
    if (uiState.showDeckExportFileDialog) {
        FileChooserDialog(
            title = "Export Deck",
            fileType = FileType.MARKDOWN,
            onFileSelected = { file ->
                scope.launch {
                    viewModel.exportSelectedDecks(file)
                }
            },
            onDismiss = {
                viewModel.hideDeckExportFileDialog()
            }
        )
    }
    
    if (uiState.showBackupImportDialog) {
        FileChooserDialog(
            title = "Import Backup",
            fileType = FileType.BACKUP,
            onFileSelected = { file ->
                scope.launch {
                    viewModel.importBackup(file)
                }
            },
            onDismiss = {
                viewModel.hideBackupImportDialog()
            }
        )
    }
    
    if (uiState.showDeckImportDialog) {
        FileChooserDialog(
            title = "Import Deck",
            fileType = FileType.MARKDOWN,
            onFileSelected = { file ->
                scope.launch {
                    viewModel.importDeck(file)
                }
            },
            onDismiss = {
                viewModel.hideDeckImportDialog()
            }
        )
    }
    
    if (uiState.showBackupRestoreDialog) {
        FileChooserDialog(
            title = "Restore Backup",
            fileType = FileType.BACKUP,
            onFileSelected = { file ->
                scope.launch {
                    viewModel.showBackupInfoDialog(file)
                }
            },
            onDismiss = {
                viewModel.hideBackupRestoreDialog()
            }
        )
    }
    
    if (uiState.showBackupInfoDialog && uiState.backupInfo != null) {
        BackupFileInfoDialog(
            backupInfo = uiState.backupInfo,
            onConfirm = {
                scope.launch {
                    viewModel.confirmBackupRestore()
                }
            },
            onDismiss = {
                viewModel.hideBackupInfoDialog()
            }
        )
    }
    
    if (uiState.showExportProgress) {
        ExportProgressDialog(
            isVisible = true,
            progress = uiState.exportProgress,
            message = uiState.exportMessage
        )
    }
}

@Composable
private fun SectionCard(
    title: String,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    content: @Composable () -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        elevation = CardDefaults.cardElevation(defaultElevation = 4.dp)
    ) {
        Column(
            modifier = Modifier.padding(16.dp)
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically
            ) {
                Icon(
                    icon,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary
                )
                Spacer(modifier = Modifier.width(12.dp))
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold
                )
            }
            
            Spacer(modifier = Modifier.height(16.dp))
            
            content()
        }
    }
}

@Composable
private fun ExportSection(
    onExportBackup: () -> Unit,
    onExportDeck: () -> Unit
) {
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        ActionButton(
            title = "Export Full Backup",
            description = "Export all data as JSON backup",
            icon = Icons.Default.Backup,
            onClick = onExportBackup
        )
        
        ActionButton(
            title = "Export Deck",
            description = "Export selected decks as markdown files",
            icon = Icons.Default.Upload,
            onClick = onExportDeck
        )
    }
}

@Composable
private fun ImportSection(
    onImportBackup: () -> Unit,
    onImportDeck: () -> Unit
) {
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        ActionButton(
            title = "Import Backup",
            description = "Restore from JSON backup file",
            icon = Icons.Default.Restore,
            onClick = onImportBackup
        )
        
        ActionButton(
            title = "Import Deck",
            description = "Import deck from markdown file",
            icon = Icons.Default.Download,
            onClick = onImportDeck
        )
    }
}

@Composable
private fun BackupManagementSection(
    onCreateBackup: () -> Unit,
    onRestoreBackup: () -> Unit
) {
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        ActionButton(
            title = "Create Backup",
            description = "Create a new backup of all data",
            icon = Icons.Default.Backup,
            onClick = onCreateBackup
        )
        
        ActionButton(
            title = "Restore Backup",
            description = "Restore from a backup file",
            icon = Icons.Default.Restore,
            onClick = onRestoreBackup
        )
    }
}

@Composable
private fun RecentBackupsSection(
    backups: List<File>,
    onRestoreBackup: (File) -> Unit
) {
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        backups.forEach { backup ->
            BackupItem(
                backupFile = backup,
                onRestore = { onRestoreBackup(backup) }
            )
        }
    }
}

@Composable
private fun ActionButton(
    title: String,
    description: String,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable { onClick() },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant
        )
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Icon(
                icon,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant
            )
            
            Spacer(modifier = Modifier.width(16.dp))
            
            Column(
                modifier = Modifier.weight(1f)
            ) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Medium
                )
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            
            Icon(
                Icons.Default.ChevronRight,
                contentDescription = "Action",
                tint = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

@Composable
private fun BackupItem(
    backupFile: File,
    onRestore: () -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable { onRestore() },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant
        )
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(12.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Icon(
                Icons.Default.Backup,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant
            )
            
            Spacer(modifier = Modifier.width(12.dp))
            
            Column(
                modifier = Modifier.weight(1f)
            ) {
                Text(
                    text = backupFile.name,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium
                )
                Text(
                    text = "Modified: ${formatFileDate(backupFile.lastModified())}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            
            IconButton(onClick = onRestore) {
                Icon(
                    Icons.Default.Restore,
                    contentDescription = "Restore",
                    tint = MaterialTheme.colorScheme.primary
                )
            }
        }
    }
}

private fun formatFileDate(timestamp: Long): String {
    val dateFormat = java.text.SimpleDateFormat("MMM dd, yyyy HH:mm", java.util.Locale.getDefault())
    return dateFormat.format(java.util.Date(timestamp))
}

// ViewModel for ImportExportScreen
@Stable
data class ImportExportUiState(
    val isLoading: Boolean = false,
    val availableDecks: List<DeckEntity> = emptyList(),
    val selectedDecks: Set<String> = emptySet(),
    val recentBackups: List<File> = emptyList(),
    val showBackupExportDialog: Boolean = false,
    val showDeckExportDialog: Boolean = false,
    val showDeckExportFileDialog: Boolean = false,
    val showBackupImportDialog: Boolean = false,
    val showDeckImportDialog: Boolean = false,
    val showBackupRestoreDialog: Boolean = false,
    val showBackupInfoDialog: Boolean = false,
    val backupInfo: BackupInfo? = null,
    val pendingBackupFile: File? = null,
    val showExportProgress: Boolean = false,
    val exportProgress: Float = 0f,
    val exportMessage: String = ""
)

class ImportExportViewModel(
    private val deckDao: DeckDao,
    private val backupManager: BackupManager,
    private val deckExporter: DeckExporter,
    private val deckMarkdownParser: com.vmargb.myoso.data.DeckMarkdownParser
) : androidx.lifecycle.ViewModel() {
    
    private val _uiState = MutableStateFlow(ImportExportUiState())
    val uiState: StateFlow<ImportExportUiState> = _uiState.asStateFlow()
    
    suspend fun loadDecks() {
        _uiState.value = _uiState.value.copy(isLoading = true)
        val decks = deckDao.getAllDecks()
        val recentBackups = backupManager.getBackupDirectory().listFiles()?.toList() ?: emptyList()
        
        _uiState.value = _uiState.value.copy(
            availableDecks = decks,
            recentBackups = recentBackups.sortedByDescending { it.lastModified() },
            isLoading = false
        )
    }
    
    fun toggleDeckSelection(deckId: String) {
        val currentSelected = _uiState.value.selectedDecks.toMutableSet()
        if (currentSelected.contains(deckId)) {
            currentSelected.remove(deckId)
        } else {
            currentSelected.add(deckId)
        }
        _uiState.value = _uiState.value.copy(selectedDecks = currentSelected)
    }
    
    fun showBackupExportDialog() {
        _uiState.value = _uiState.value.copy(showBackupExportDialog = true)
    }
    
    fun hideBackupExportDialog() {
        _uiState.value = _uiState.value.copy(showBackupExportDialog = false)
    }
    
    fun showDeckExportDialog() {
        _uiState.value = _uiState.value.copy(showDeckExportDialog = true)
    }
    
    fun hideDeckExportDialog() {
        _uiState.value = _uiState.value.copy(showDeckExportDialog = false)
    }
    
    fun showDeckExportFileDialog() {
        _uiState.value = _uiState.value.copy(showDeckExportFileDialog = true)
    }
    
    fun hideDeckExportFileDialog() {
        _uiState.value = _uiState.value.copy(showDeckExportFileDialog = false)
    }
    
    fun showBackupImportDialog() {
        _uiState.value = _uiState.value.copy(showBackupImportDialog = true)
    }
    
    fun hideBackupImportDialog() {
        _uiState.value = _uiState.value.copy(showBackupImportDialog = false)
    }
    
    fun showDeckImportDialog() {
        _uiState.value = _uiState.value.copy(showDeckImportDialog = true)
    }
    
    fun hideDeckImportDialog() {
        _uiState.value = _uiState.value.copy(showDeckImportDialog = false)
    }
    
    fun showBackupRestoreDialog() {
        _uiState.value = _uiState.value.copy(showBackupRestoreDialog = true)
    }
    
    fun hideBackupRestoreDialog() {
        _uiState.value = _uiState.value.copy(showBackupRestoreDialog = false)
    }
    
    fun showBackupInfoDialog(file: File) {
        _uiState.value = _uiState.value.copy(
            showBackupInfoDialog = true,
            pendingBackupFile = file
        )
    }
    
    fun hideBackupInfoDialog() {
        _uiState.value = _uiState.value.copy(
            showBackupInfoDialog = false,
            pendingBackupFile = null,
            backupInfo = null
        )
    }
    
    suspend fun exportBackup(file: File) {
        _uiState.value = _uiState.value.copy(
            showExportProgress = true,
            exportProgress = 0f,
            exportMessage = "Creating backup..."
        )
        
        val result = backupManager.exportBackupToFile(file)
        
        _uiState.value = _uiState.value.copy(
            showExportProgress = false,
            showBackupExportDialog = false
        )
        
        // Handle result (success/error)
    }
    
    suspend fun exportSelectedDecks(file: File) {
        _uiState.value = _uiState.value.copy(
            showExportProgress = true,
            exportProgress = 0f,
            exportMessage = "Exporting decks..."
        )
        
        val selectedDecks = _uiState.value.selectedDecks.toList()
        val result = deckExporter.exportDecksToMarkdown(selectedDecks, file.parentFile ?: file)
        
        _uiState.value = _uiState.value.copy(
            showExportProgress = false,
            showDeckExportFileDialog = false,
            showDeckExportDialog = false,
            selectedDecks = emptySet()
        )
        
        // Handle result (success/error)
    }
    
    suspend fun importBackup(file: File) {
        _uiState.value = _uiState.value.copy(
            showExportProgress = true,
            exportProgress = 0f,
            exportMessage = "Importing backup..."
        )
        
        val result = backupManager.importBackupFromFile(file)
        
        _uiState.value = _uiState.value.copy(
            showExportProgress = false,
            showBackupImportDialog = false
        )
        
        // Handle result (success/error)
    }
    
    suspend fun importDeck(file: File) {
        _uiState.value = _uiState.value.copy(
            showExportProgress = true,
            exportProgress = 0f,
            exportMessage = "Importing deck..."
        )
        
        val result = deckMarkdownParser.parseAndImport(file.absolutePath)
        
        _uiState.value = _uiState.value.copy(
            showExportProgress = false,
            showDeckImportDialog = false
        )
        
        // Handle result (success/error)
    }
    
    suspend fun createBackup() {
        _uiState.value = _uiState.value.copy(
            showExportProgress = true,
            exportProgress = 0f,
            exportMessage = "Creating backup..."
        )
        
        val backupDir = backupManager.getBackupDirectory()
        val backupFile = File(backupDir, backupManager.generateBackupFilename())
        val result = backupManager.exportBackupToFile(backupFile)
        
        _uiState.value = _uiState.value.copy(
            showExportProgress = false
        )
        
        // Refresh recent backups
        loadDecks()
    }
    
    suspend fun restoreBackup(backupFile: File) {
        _uiState.value = _uiState.value.copy(
            showExportProgress = true,
            exportProgress = 0f,
            exportMessage = "Restoring backup..."
        )
        
        val result = backupManager.importBackupFromFile(backupFile)
        
        _uiState.value = _uiState.value.copy(
            showExportProgress = false
        )
        
        // Handle result (success/error)
    }
    
    suspend fun showBackupInfoDialog(file: File) {
        val backupInfoResult = backupManager.getBackupInfo(file)
        if (backupInfoResult.isSuccess) {
            _uiState.value = _uiState.value.copy(
                showBackupInfoDialog = true,
                backupInfo = backupInfoResult.getOrNull(),
                pendingBackupFile = file
            )
        }
    }
    
    suspend fun confirmBackupRestore() {
        val file = _uiState.value.pendingBackupFile ?: return
        restoreBackup(file)
        hideBackupInfoDialog()
    }
}
