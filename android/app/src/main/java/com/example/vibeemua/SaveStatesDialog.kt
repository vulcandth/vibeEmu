package com.example.vibeemua

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

/** Operations share the emulator lock and execute on its large-stack worker. */
@Composable
fun SaveStatesDialog(emulator: Emulator, dispatcher: CoroutineDispatcher, root: File, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var message by remember { mutableStateOf("Reading slots…") }
    var rows by remember { mutableStateOf<List<Triple<String, String, Boolean>>>(emptyList()) }

    fun accept(reply: String): Boolean {
        val json = JSONObject(reply)
        message = json.getString("message")
        json.optJSONArray("rows")?.let { array ->
            rows = (0 until array.length()).map { index ->
                val row = array.getJSONObject(index)
                Triple(row.getString("label"), row.getString("description"), row.getBoolean("available"))
            }
        }
        return json.getBoolean("ok")
    }
    fun operate(operation: Int, slot: Int = 11) {
        if (busy) return
        busy = true
        scope.launch {
            try { accept(withContext(dispatcher) { emulator.stateOperation(root.absolutePath, operation, slot) }) }
            catch (e: kotlinx.coroutines.CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "State operation failed" }
            finally { busy = false }
        }
    }

    val importState = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null && !busy) {
            busy = true
            scope.launch {
                try {
                    val reply = withContext(dispatcher) {
                        val temp = File.createTempFile("state-import-", ".vstate", context.cacheDir)
                        try {
                            context.contentResolver.openInputStream(uri)?.use { input ->
                                temp.outputStream().use { output ->
                                    val buffer = ByteArray(8192)
                                    var total = 0
                                    while (true) {
                                        val count = input.read(buffer)
                                        if (count < 0) break
                                        total += count
                                        check(total <= 16 * 1024 * 1024) { "Save state exceeds size limit" }
                                        output.write(buffer, 0, count)
                                    }
                                }
                            } ?: error("Cannot open state document")
                            emulator.stateOperation(root.absolutePath, 3, path = temp.absolutePath)
                        } finally { temp.delete() }
                    }
                    accept(reply)
                } catch (e: kotlinx.coroutines.CancellationException) { throw e }
                catch (e: Exception) { message = e.message ?: "Import failed" }
                finally { busy = false }
            }
        }
    }
    val exportState = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        if (uri != null && !busy) {
            busy = true
            scope.launch {
                try {
                    val reply = withContext(dispatcher) {
                        val temp = File.createTempFile("state-export-", ".vstate", context.cacheDir)
                        try {
                            val result = emulator.stateOperation(root.absolutePath, 4, path = temp.absolutePath)
                            if (JSONObject(result).getBoolean("ok")) {
                                context.contentResolver.openOutputStream(uri, "wt")?.use { output ->
                                    temp.inputStream().use { it.copyTo(output) }
                                } ?: error("Cannot write state document")
                            }
                            result
                        } finally { temp.delete() }
                    }
                    if (accept(reply)) message = "State exported."
                } catch (e: kotlinx.coroutines.CancellationException) { throw e }
                catch (e: Exception) { message = e.message ?: "Export failed" }
                finally { busy = false }
            }
        }
    }
    LaunchedEffect(root) { operate(0) }
    BackHandler { if (!busy) onDismiss() }
    AlertDialog(
        onDismissRequest = { if (!busy) onDismiss() },
        title = { Text("Save states") },
        text = {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Loads replace cartridge SRAM. Recovery / undo load restores the previous machine and SRAM. Disconnect Mobile Adapter first.")
                Text(message)
                if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(enabled = !busy, onClick = { importState.launch(arrayOf("*/*")) }) { Text("Import") }
                    TextButton(enabled = !busy, onClick = { exportState.launch("game.vstate") }) { Text("Export") }
                    TextButton(enabled = !busy, onClick = { operate(0) }) { Text("Refresh") }
                }
                rows.forEachIndexed { index, row ->
                    Text(row.first, style = MaterialTheme.typography.titleSmall)
                    Text(row.second, style = MaterialTheme.typography.bodySmall)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (index != 11) TextButton(modifier = Modifier.testTag("save-state-${index + 1}"), enabled = !busy, onClick = { operate(1, index + 1) }) { Text("Save / overwrite") }
                        TextButton(modifier = Modifier.testTag("load-state-${index + 1}"), enabled = !busy && row.third, onClick = { operate(2, index + 1) }) { Text("Load") }
                    }
                }
            }
        },
        confirmButton = { TextButton(enabled = !busy, onClick = onDismiss) { Text("Done") } },
    )
}
