package com.example.vibeemua

import android.util.AtomicFile
import java.io.File
import java.security.MessageDigest
import java.util.Properties

/** Machine changes are durable drafts. Existing boot files and battery saves stay in place. */
class MachineSettingsStore(private val directory: File) {
    private val committed = AtomicFile(File(directory, "machine-settings.properties"))
    private val draft = AtomicFile(File(directory, "machine-settings-pending.properties"))

    private fun read(file: AtomicFile): Properties? = try {
        file.openRead().use { stream ->
            Properties().apply {
                load(stream)
                require(getProperty("version") == "1") { "Unsupported machine settings version" }
            }
        }
    } catch (_: java.io.FileNotFoundException) { null }

    private fun write(file: AtomicFile, properties: Properties) {
        val stream = file.startWrite()
        try {
            properties.store(stream, "vibeEmu machine settings")
            file.finishWrite(stream)
        } catch (error: Exception) {
            file.failWrite(stream)
            throw error
        }
    }

    private fun properties(options: AppOptions) = Properties().apply {
        setProperty("version", "1")
        setProperty("model", options.emulationMode.nativeId.toString())
        setProperty("enabled", options.enabledBootRoms.sortedBy { it.nativeId }.joinToString(",") { it.nativeId.toString() })
    }

    private fun selected(options: AppOptions) = read(draft) ?: read(committed) ?: properties(options)

    private fun apply(properties: Properties?, options: AppOptions): AppOptions {
        if (properties == null) return options
        val model = EmulationMode.entries.firstOrNull { it.nativeId.toString() == properties.getProperty("model") }
            ?: error("Invalid hardware model in machine settings")
        val enabled = properties.getProperty("enabled", "").split(",").filter { it.isNotEmpty() }.map { id ->
            BootRomMode.entries.firstOrNull { it.nativeId.toString() == id } ?: error("Invalid boot ROM slot")
        }.toSet()
        return options.copy(emulationMode = model, enabledBootRoms = enabled)
    }

    fun load(options: AppOptions): AppOptions = apply(read(draft) ?: read(committed), options)
    fun hasPendingChanges(): Boolean = read(draft) != null

    private fun preserveCommitted(options: AppOptions) {
        if (read(committed) == null) write(committed, properties(options))
    }

    fun stage(before: AppOptions, after: AppOptions) {
        if (before.emulationMode == after.emulationMode && before.enabledBootRoms == after.enabledBootRoms) return
        preserveCommitted(before)
        val next = selected(before)
        next.putAll(properties(after))
        write(draft, next)
    }

    /** Immutable, content-addressed imports let Discard recover even a replaced boot image. */
    fun stageBoot(options: AppOptions, mode: BootRomMode, bytes: ByteArray?): AppOptions {
        require(bytes == null || mode.acceptsSize(bytes.size)) { "Invalid ${mode.label} boot ROM size" }
        preserveCommitted(options)
        val next = selected(options)
        val name = if (bytes == null) "cleared" else {
            val hash = MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }
            val name = "boot-$hash.bin"
            val blob = AtomicFile(File(directory, name))
            val stream = blob.startWrite()
            try { stream.write(bytes); blob.finishWrite(stream) }
            catch (error: Exception) { blob.failWrite(stream); throw error }
            name
        }
        next.setProperty("boot.${mode.nativeId}", name)
        val updated = options.copy(enabledBootRoms = if (bytes == null) options.enabledBootRoms - mode else options.enabledBootRoms + mode)
        next.putAll(properties(updated))
        write(draft, next)
        return updated
    }

    fun bootFile(options: AppOptions, mode: BootRomMode): File? {
        val name = selected(options).getProperty("boot.${mode.nativeId}") ?: mode.fileName
        if (name == "cleared") return null
        require(name == mode.fileName || Regex("boot-[0-9a-f]{64}\\.bin").matches(name)) { "Invalid boot ROM reference" }
        return File(directory, name).takeIf { it.isFile }
    }

    fun readBootRoms(options: AppOptions): Map<BootRomMode, ByteArray> = options.enabledBootRoms.associateWith { mode ->
        val bytes = bootFile(options, mode)?.readBytes() ?: error("${mode.label} boot ROM is missing")
        require(mode.acceptsSize(bytes.size)) { "Invalid ${mode.label} boot ROM size" }
        bytes
    }

    fun discard(options: AppOptions): AppOptions {
        val restored = apply(read(committed), options)
        draft.delete()
        return restored
    }

    /** Called only after a replacement machine has successfully loaded. */
    fun commit() {
        read(draft)?.let { write(committed, it) }
        draft.delete()
    }
}
