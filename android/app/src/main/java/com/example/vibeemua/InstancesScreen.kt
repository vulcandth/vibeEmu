package com.example.vibeemua

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import java.text.DateFormat

@Composable
fun InstancesScreen(
    onPlayInstance: (GameInstance) -> Unit,
) {
    val context = LocalContext.current
    val repo = remember(context) { GameInstancesRepository(context) }

    fun refresh(): List<GameInstance> = repo.list()

    var instances by remember { mutableStateOf(refresh()) }

    var pendingRomUri by remember { mutableStateOf<Uri?>(null) }
    var pendingRomName by remember { mutableStateOf<String?>(null) }
    var nicknameDraft by remember { mutableStateOf("") }

    var renameTarget by remember { mutableStateOf<GameInstance?>(null) }
    var renameDraft by remember { mutableStateOf("") }

    var deleteTarget by remember { mutableStateOf<GameInstance?>(null) }

    var importSavTarget by remember { mutableStateOf<GameInstance?>(null) }
    var exportSavTarget by remember { mutableStateOf<GameInstance?>(null) }
    var replaceRomTarget by remember { mutableStateOf<GameInstance?>(null) }

    var status by remember { mutableStateOf<String?>(null) }

    val pickRom = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        val displayName = try {
            context.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        } catch (_: Throwable) {
            null
        } ?: (uri.lastPathSegment ?: "rom.gb")

        pendingRomUri = uri
        pendingRomName = displayName
        nicknameDraft = displayName.substringBeforeLast('.').ifBlank { displayName }
    }

    val pickSav = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        val target = importSavTarget
        importSavTarget = null
        if (uri == null || target == null) return@rememberLauncherForActivityResult
        val ok = repo.importSavFromUri(target.id, uri)
        status = if (ok) "Imported .sav for ${target.nickname}" else "Failed to import .sav"
        instances = refresh()
    }

    val createSavDoc = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri: Uri? ->
        val target = exportSavTarget
        exportSavTarget = null
        if (uri == null || target == null) return@rememberLauncherForActivityResult
        val ok = repo.exportSavToUri(target.id, uri)
        if (ok) repo.setLastSavExportNow(target.id)
        status = if (ok) "Exported .sav for ${target.nickname}" else "Failed to export .sav"
        instances = refresh()
    }

    val pickReplacementRom = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        val target = replaceRomTarget
        replaceRomTarget = null
        if (uri == null || target == null) return@rememberLauncherForActivityResult
        val displayName = try {
            context.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        } catch (_: Throwable) {
            null
        } ?: (uri.lastPathSegment ?: "rom.gb")

        val ok = repo.replaceRomFromUri(target.id, uri, displayName)
        status = if (ok) "Replaced ROM for ${target.nickname}" else "Failed to replace ROM"
        instances = refresh()
    }

    LaunchedEffect(Unit) {
        repo.ensureRoot()
        instances = refresh()
    }

    InstancesScaffold(
        status = status,
        onImportRom = { pickRom.launch("application/octet-stream") },
        instances = instances,
        savLastWriteMillis = { id -> repo.savLastWriteMillis(id) },
        lastSavExportMillis = { it.lastSavExportMillis },
        onPlay = { onPlayInstance(it) },
        onRename = {
            renameTarget = it
            renameDraft = it.nickname
        },
        onImportSav = {
            importSavTarget = it
            pickSav.launch("application/octet-stream")
        },
        onExportSav = {
            exportSavTarget = it
            createSavDoc.launch("${it.nickname}.sav")
        },
        onReplaceRom = {
            replaceRomTarget = it
            pickReplacementRom.launch("application/octet-stream")
        },
        onDelete = { deleteTarget = it },
    )

    val pendingUri = pendingRomUri
    val pendingName = pendingRomName
    if (pendingUri != null && pendingName != null) {
        SimpleTextInputDialog(
            title = "Name this instance",
            initialValue = nicknameDraft,
            confirmLabel = "Create",
            onConfirm = { name ->
                val nickname = name.trim().ifBlank { pendingName.substringBeforeLast('.') }
                val created = repo.createFromRomUri(pendingUri, pendingName, nickname)
                status = if (created != null) "Created instance: ${created.nickname}" else "Failed to import ROM"
                pendingRomUri = null
                pendingRomName = null
                instances = refresh()
            },
            onDismiss = {
                pendingRomUri = null
                pendingRomName = null
            }
        )
    }

    val r = renameTarget
    if (r != null) {
        SimpleTextInputDialog(
            title = "Rename instance",
            initialValue = renameDraft,
            confirmLabel = "Save",
            onConfirm = { name ->
                val ok = repo.rename(r.id, name.trim().ifBlank { r.nickname })
                status = if (ok) "Renamed instance" else "Failed to rename"
                renameTarget = null
                instances = refresh()
            },
            onDismiss = { renameTarget = null }
        )
    }

    val d = deleteTarget
    if (d != null) {
        AlertDialog(
            onDismissRequest = { deleteTarget = null },
            title = { Text("Delete instance?") },
            text = { Text("This deletes the ROM and save files for '${d.nickname}'.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        repo.deleteInstance(d.id)
                        deleteTarget = null
                        instances = refresh()
                        status = "Deleted instance"
                    }
                ) { Text("Delete") }
            },
            dismissButton = {
                TextButton(onClick = { deleteTarget = null }) { Text("Cancel") }
            },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun InstancesScaffold(
    status: String?,
    onImportRom: () -> Unit,
    instances: List<GameInstance>,
    savLastWriteMillis: (String) -> Long?,
    lastSavExportMillis: (GameInstance) -> Long?,
    onPlay: (GameInstance) -> Unit,
    onRename: (GameInstance) -> Unit,
    onImportSav: (GameInstance) -> Unit,
    onExportSav: (GameInstance) -> Unit,
    onReplaceRom: (GameInstance) -> Unit,
    onDelete: (GameInstance) -> Unit,
) {
    val df = remember { DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Instances") },
                actions = {
                    Button(onClick = onImportRom) { Text("Import ROM") }
                },
                scrollBehavior = androidx.compose.material3.TopAppBarDefaults.pinnedScrollBehavior(rememberTopAppBarState())
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            if (status != null) {
                Text(text = status)
            }

            if (instances.isEmpty()) {
                Text("No instances yet.")
                Text("Use 'Import ROM' to create one.")
                return@Column
            }

            LazyColumn(modifier = Modifier.fillMaxSize()) {
                items(instances, key = { it.id }) { inst ->
                    var menuExpanded by remember(inst.id) { mutableStateOf(false) }

                    val lastSavWrite = savLastWriteMillis(inst.id)
                    val lastExport = lastSavExportMillis(inst)

                    val lastSavWriteLabel = if (lastSavWrite != null) df.format(java.util.Date(lastSavWrite)) else "Never"
                    val lastExportLabel = if (lastExport != null) df.format(java.util.Date(lastExport)) else "Never"

                    ListItem(
                        headlineContent = { Text(inst.nickname, fontWeight = FontWeight.SemiBold) },
                        supportingContent = {
                            Column {
                                Text("ROM: ${inst.romDisplayName}")
                                Text("Last .sav write: $lastSavWriteLabel")
                                Text("Last export: $lastExportLabel")
                            }
                        },
                        trailingContent = {
                            Row {
                                OutlinedButton(onClick = { onPlay(inst) }) { Text("Play") }
                                Spacer(Modifier.padding(horizontal = 4.dp))
                                IconButton(onClick = { menuExpanded = true }) {
                                    Icon(Icons.Filled.MoreVert, contentDescription = "More")
                                }
                                DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
                                    DropdownMenuItem(text = { Text("Rename") }, onClick = { menuExpanded = false; onRename(inst) })
                                    DropdownMenuItem(text = { Text("Import .sav") }, onClick = { menuExpanded = false; onImportSav(inst) })
                                    DropdownMenuItem(text = { Text("Export .sav") }, onClick = { menuExpanded = false; onExportSav(inst) })
                                    DropdownMenuItem(text = { Text("Replace ROM") }, onClick = { menuExpanded = false; onReplaceRom(inst) })
                                    HorizontalDivider()
                                    DropdownMenuItem(text = { Text("Delete") }, onClick = { menuExpanded = false; onDelete(inst) })
                                }
                            }
                        },
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onPlay(inst) }
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}

@Composable
private fun SimpleTextInputDialog(
    title: String,
    initialValue: String,
    confirmLabel: String,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var value by remember { mutableStateOf(initialValue) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            androidx.compose.material3.OutlinedTextField(
                value = value,
                onValueChange = { value = it },
                singleLine = true,
                label = { Text("Name") },
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(value) }) { Text(confirmLabel) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        }
    )
}
