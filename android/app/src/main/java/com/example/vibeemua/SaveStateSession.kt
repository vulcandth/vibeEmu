package com.example.vibeemua

import android.content.Context
import android.net.Uri
import androidx.compose.runtime.*
import kotlinx.coroutines.*
import org.json.JSONObject
import java.io.File
import java.text.DateFormat
import java.util.Date

data class StateSlot(val id: Int, val label: String, val description: String, val available: Boolean,
                     val present: Boolean, val created: Long) {
    val date: String get() = if (created > 0) DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT)
        .format(Date(created * 1000)) else ""
}

internal fun stateSlots(reply: JSONObject): List<StateSlot> {
    val array = reply.optJSONArray("rows") ?: return emptyList()
    return (0 until array.length()).map { index ->
        val row = array.getJSONObject(index)
        StateSlot(row.getInt("slot"), row.getString("label"), row.getString("description"),
            row.getBoolean("available"), row.getBoolean("present"), row.getLong("created_unix"))
    }.sortedBy { if (it.id == 11) 0 else it.id }
}

internal fun suggestedState(rows: List<StateSlot>): StateSlot? = rows.filter { it.available && it.id != 12 }
    .sortedWith(compareByDescending<StateSlot> { it.created }.thenBy { if (it.id == 11) 0 else it.id }).firstOrNull()

data class PreparedLaunch(val instance: GameInstance, val options: AppOptions, val candidate: Emulator,
                          val root: File, val settings: MachineSettingsStore, val rows: List<StateSlot>)
data class LaunchedGame(val instance: GameInstance, val options: AppOptions)
data class StateConfirmation(val operation: Int, val slot: StateSlot)
data class StateDocument(val instanceId: String, val generation: Long, val import: Boolean)

/** Owned by the activity ViewModel: JNI completions and picker results survive rotation. */
class SaveStateSession(private val emulator: Emulator, private val dispatcher: CoroutineDispatcher,
                       private val scope: CoroutineScope) {
    var busy by mutableStateOf(false); private set
    var preparing by mutableStateOf(false); private set
    var message by mutableStateOf(""); private set
    var rows by mutableStateOf<List<StateSlot>>(emptyList()); private set
    var prepared by mutableStateOf<PreparedLaunch?>(null); private set
    var launched by mutableStateOf<LaunchedGame?>(null); private set
    var error by mutableStateOf<String?>(null)
    var confirmation by mutableStateOf<StateConfirmation?>(null)
    var document by mutableStateOf<StateDocument?>(null); private set
    var completedLoad by mutableIntStateOf(0); private set
    var completedQuickSave by mutableIntStateOf(0); private set
    private var root: File? = null
    private var instanceId: String? = null
    private var generation = 0L

    private fun parse(reply: String): JSONObject = JSONObject(reply).also {
        check(it.getBoolean("ok")) { it.getString("message") }
    }

    fun bind(id: String, directory: File) {
        if (instanceId == id && root == directory) return
        check(!busy && document == null)
        generation++
        instanceId = id
        root = directory
        rows = emptyList()
        message = ""
    }

    fun prepare(context: Context, instance: GameInstance, options: AppOptions, settings: MachineSettingsStore) {
        if (busy || prepared != null || document != null) return
        val app = context.applicationContext
        busy = true
        preparing = true
        error = null
        emulator.setPaused(true)
        scope.launch {
            val candidate = Emulator().apply { setPaused(true) }
            var startImmediately = false
            try {
                val next = withContext(dispatcher) {
                    emulator.saveRam()
                    val repository = GameInstancesRepository(app)
                    check(candidate.loadRomFromFile(repository.romFile(instance.id).absolutePath,
                        options.emulationMode, settings.readBootRoms(options))) { "Failed to load instance or boot ROM" }
                    val directory = File(repository.instanceDir(instance.id), "states")
                    val slots = stateSlots(parse(candidate.stateOperation(directory.absolutePath, 0)))
                    PreparedLaunch(instance, options, candidate, directory, settings, slots)
                }
                prepared = next
                startImmediately = next.rows.none { it.present }
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { error = e.message ?: "Cannot prepare game" }
            finally {
                if (prepared?.candidate !== candidate) candidate.close(save = false)
                busy = false
                preparing = false
            }
            if (startImmediately) finishLaunch(app, null)
        }
    }

    fun cancelLaunch() {
        if (busy) return
        prepared?.candidate?.close(save = false)
        prepared = null
        error = null
    }

    fun finishLaunch(context: Context, slot: Int?) {
        val next = prepared ?: return
        if (busy) return
        busy = true
        error = null
        val app = context.applicationContext
        scope.launch {
            try {
                withContext(dispatcher) {
                    // Revalidate on load: a file may have disappeared or changed since listing.
                    next.settings.commit()
                    if (slot != null) parse(next.candidate.stateOperation(next.root.absolutePath, 2, slot))
                    emulator.adoptPrepared(next.candidate)
                }
                generation++
                instanceId = next.instance.id
                root = next.root
                rows = emptyList()
                message = if (slot == null) "Started without loading a state." else "Save state restored, including cartridge SRAM."
                prepared = null
                launched = LaunchedGame(next.instance, next.options)
                runCatching { GameInstancesRepository(app).markPlayed(next.instance.id) }
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { error = e.message ?: "Cannot launch game" }
            finally { busy = false }
        }
    }

    fun consumeLaunch() { launched = null }

    fun refresh() = operate(0)

    fun request(operation: Int, slot: StateSlot, quick: Boolean = false) {
        if (busy || document != null) return
        if (operation == 2 || slot.present) confirmation = StateConfirmation(operation, slot)
        else operate(operation, slot.id, quick)
    }

    fun operate(operation: Int, slot: Int = 11, quick: Boolean = false) {
        val directory = root ?: return
        if (busy || document != null) return
        busy = true
        confirmation = null
        scope.launch {
            try {
                val reply = withContext(dispatcher) { parse(emulator.stateOperation(directory.absolutePath, operation, slot)) }
                rows = stateSlots(reply)
                message = when (operation) {
                    1 -> "${rows.first { it.id == slot }.label} saved."
                    2 -> "State restored. Undo load is available in Save states."
                    else -> "Choose a slot. Loading also restores cartridge SRAM."
                }
                if (operation == 2) completedLoad++
                if (operation == 1 && quick) completedQuickSave++
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "State operation failed" }
            finally { busy = false }
        }
    }

    fun beginDocument(import: Boolean): Boolean {
        if (busy || document != null || instanceId == null) return false
        document = StateDocument(instanceId!!, generation, import)
        return true
    }

    fun completeDocument(context: Context, uri: Uri?) {
        val ticket = document ?: return
        document = null
        if (uri == null) { message = if (ticket.import) "Import canceled." else "Export canceled."; return }
        if (ticket.instanceId != instanceId || ticket.generation != generation) {
            message = "The game changed. Open the document again for this instance."; return
        }
        val directory = root ?: return
        val app = context.applicationContext
        busy = true
        scope.launch {
            try {
                val reply = withContext(dispatcher) {
                    val temp = File.createTempFile("save-state-", ".vstate", app.cacheDir)
                    try {
                        if (ticket.import) {
                            app.contentResolver.openInputStream(uri)?.use { input ->
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
                            parse(emulator.stateOperation(directory.absolutePath, 3, path = temp.absolutePath))
                        } else {
                            val result = parse(emulator.stateOperation(directory.absolutePath, 4, path = temp.absolutePath))
                            app.contentResolver.openOutputStream(uri, "wt")?.use { output -> temp.inputStream().use { it.copyTo(output) } }
                                ?: error("Cannot write state document")
                            result
                        }
                    } finally { temp.delete() }
                }
                rows = stateSlots(reply)
                message = if (ticket.import) "Imported state restored. Undo load is available in Save states." else "Current state exported."
                if (ticket.import) completedLoad++
            } catch (e: CancellationException) { throw e }
            catch (e: Exception) { message = e.message ?: "Document operation failed" }
            finally { busy = false }
        }
    }

    fun close() { prepared?.candidate?.close(save = false); prepared = null }
}
