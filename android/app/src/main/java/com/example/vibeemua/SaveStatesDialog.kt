package com.example.vibeemua

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
fun StateConfirmationDialog(session: SaveStateSession, quick: Boolean = false) {
    session.confirmation?.let { choice ->
        val saving = choice.operation == 1
        AlertDialog(onDismissRequest = { session.confirmation = null },
            title = { Text(if (saving) "Replace ${choice.slot.label}?" else "Load ${choice.slot.label}?") },
            text = { Text(if (saving) "Replace the save from ${choice.slot.date.ifEmpty { "this slot" }}? This cannot be undone."
                else "Restore this machine and its cartridge SRAM. Your current machine and SRAM will be kept in Undo load.") },
            confirmButton = { TextButton(modifier = Modifier.testTag("confirm-state"), onClick = {
                session.operate(choice.operation, choice.slot.id, quick)
            }) { Text(if (saving) "Replace save" else "Load state") } },
            dismissButton = { TextButton(onClick = { session.confirmation = null }) { Text("Cancel") } })
    }
}

@Composable
fun SaveStatesDialog(session: SaveStateSession, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val locked = session.busy || session.document != null
    var importConfirmation by rememberSaveable { mutableStateOf(false) }
    val importState = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { session.completeDocument(context, it) }
    val exportState = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { session.completeDocument(context, it) }
    LaunchedEffect(session) { if (!locked) session.refresh() }
    BackHandler { if (!locked) onDismiss() }
    AlertDialog(
        onDismissRequest = { if (!locked) onDismiss() },
        title = { Text("Save states") },
        text = {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                // Feedback stays visible even when a numbered slot is scrolled into view.
                Text(session.message, Modifier.testTag("state-message"))
                if (locked) LinearProgressIndicator(Modifier.fillMaxWidth())
                Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Quick save uses its own slot. Numbered saves are separate. Loading restores cartridge SRAM. Disconnect Mobile Adapter before using states.")
                    session.rows.forEach { row ->
                        HorizontalDivider()
                        Text(if (row.id == 12) "Undo load" else row.label, style = MaterialTheme.typography.titleSmall)
                        if (row.date.isNotEmpty()) Text(row.date, style = MaterialTheme.typography.bodySmall)
                        Text(row.description, style = MaterialTheme.typography.bodySmall)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            if (row.id != 12) TextButton(modifier = Modifier.testTag("save-state-${row.id}"), enabled = !locked,
                                onClick = { session.request(1, row) }) { Text(if (row.present) "Replace save" else "Save here") }
                            TextButton(modifier = Modifier.testTag("load-state-${row.id}"), enabled = !locked && row.available,
                                onClick = { session.request(2, row) }) { Text(if (row.id == 12) "Undo load" else "Load") }
                        }
                    }
                    HorizontalDivider()
                    TextButton(enabled = !locked, onClick = { importConfirmation = true }) { Text("Import and load a state") }
                    TextButton(enabled = !locked, onClick = {
                        if (session.beginDocument(false)) exportState.launch("game.vstate")
                    }) { Text("Export current state") }
                    TextButton(enabled = !locked, onClick = { session.refresh() }) { Text("Refresh slots") }
                }
            }
        },
        confirmButton = { TextButton(enabled = !locked, onClick = onDismiss) { Text("Done") } },
    )
    StateConfirmationDialog(session)
    if (importConfirmation) AlertDialog(onDismissRequest = { importConfirmation = false },
        title = { Text("Import and load?") },
        text = { Text("The selected file will replace the current machine and cartridge SRAM. Undo load keeps your pre-import state.") },
        confirmButton = { TextButton(onClick = {
            importConfirmation = false
            if (session.beginDocument(true)) importState.launch(arrayOf("*/*"))
        }) { Text("Choose state file") } },
        dismissButton = { TextButton(onClick = { importConfirmation = false }) { Text("Cancel") } })
}

@Composable
fun ResumeStateDialog(session: SaveStateSession) {
    val next = session.prepared ?: return
    val context = LocalContext.current
    val suggested = suggestedState(next.rows)
    var selected by rememberSaveable(next.instance.id) { mutableIntStateOf(suggested?.id ?: -1) }
    AlertDialog(onDismissRequest = { session.cancelLaunch() },
        title = { Text("Resume ${next.instance.nickname}?") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(session.error ?: "Load a saved machine, or start the ROM using its current battery save. Loading a state also restores its cartridge SRAM.")
                if (session.busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState())) {
                    next.rows.filter { it.present }.forEach { row ->
                        Row {
                            RadioButton(selected = selected == row.id, enabled = row.available && !session.busy,
                                modifier = Modifier.testTag("resume-slot-${row.id}"), onClick = { selected = row.id })
                            Column {
                                Text(row.label + if (row.id == suggested?.id) " · latest save" else "")
                                Text(row.date, style = MaterialTheme.typography.bodySmall)
                                Text(row.description, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                    }
                }
                TextButton(enabled = !session.busy, onClick = { session.finishLaunch(context, null) }) { Text("Start without loading") }
            }
        },
        confirmButton = { TextButton(enabled = !session.busy && next.rows.any { it.id == selected && it.available },
            onClick = { session.finishLaunch(context, selected) }) { Text("Resume saved game") } },
        dismissButton = { TextButton(enabled = !session.busy, onClick = { session.cancelLaunch() }) { Text("Cancel") } })
}
