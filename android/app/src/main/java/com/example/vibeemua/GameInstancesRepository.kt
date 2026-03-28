package com.example.vibeemua

import android.content.Context
import android.net.Uri
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream

class GameInstancesRepository(private val context: Context) {
    private val rootDir: File = File(context.filesDir, "instances")
    private val indexFile: File = File(rootDir, "instances.json")

    fun list(): List<GameInstance> {
        val items = readIndex()
        return items.sortedByDescending { it.createdAtMillis }
    }

    fun get(id: String): GameInstance? = readIndex().firstOrNull { it.id == id }

    fun instanceDir(id: String): File = File(rootDir, id)

    fun romFile(id: String): File = File(instanceDir(id), ROM_FILENAME)

    fun savFile(id: String): File = File(instanceDir(id), SAV_FILENAME)

    fun rtcFile(id: String): File = File(instanceDir(id), RTC_FILENAME)

    fun ensureRoot() {
        if (!rootDir.exists()) rootDir.mkdirs()
    }

    fun createFromRomUri(uri: Uri, romDisplayName: String, nickname: String): GameInstance? {
        ensureRoot()
        val instance = GameInstance(
            nickname = nickname,
            romDisplayName = romDisplayName,
        )
        val dir = instanceDir(instance.id)
        if (!dir.mkdirs() && !dir.exists()) return null

        val romBytes = try {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (_: Throwable) {
            null
        } ?: return null

        try {
            romFile(instance.id).writeBytes(romBytes)
        } catch (_: Throwable) {
            return null
        }

        val next = readIndex().toMutableList().apply { add(instance) }
        writeIndex(next)
        return instance
    }

    fun rename(id: String, nickname: String): Boolean {
        val cur = readIndex().toMutableList()
        val idx = cur.indexOfFirst { it.id == id }
        if (idx < 0) return false
        cur[idx] = cur[idx].copy(nickname = nickname)
        writeIndex(cur)
        return true
    }

    fun setLastSavExportNow(id: String): Boolean {
        val cur = readIndex().toMutableList()
        val idx = cur.indexOfFirst { it.id == id }
        if (idx < 0) return false
        cur[idx] = cur[idx].copy(lastSavExportMillis = System.currentTimeMillis())
        writeIndex(cur)
        return true
    }

    fun replaceRomFromUri(id: String, uri: Uri, romDisplayName: String): Boolean {
        val romBytes = try {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (_: Throwable) {
            null
        } ?: return false

        try {
            romFile(id).writeBytes(romBytes)
        } catch (_: Throwable) {
            return false
        }

        val cur = readIndex().toMutableList()
        val idx = cur.indexOfFirst { it.id == id }
        if (idx < 0) return false
        cur[idx] = cur[idx].copy(romDisplayName = romDisplayName)
        writeIndex(cur)
        return true
    }

    fun importSavFromUri(id: String, uri: Uri): Boolean {
        val savBytes = try {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (_: Throwable) {
            null
        } ?: return false

        try {
            savFile(id).writeBytes(savBytes)
        } catch (_: Throwable) {
            return false
        }
        return true
    }

    fun exportSavToUri(id: String, uri: Uri): Boolean {
        val sav = savFile(id)
        if (!sav.exists()) return false

        try {
            context.contentResolver.openOutputStream(uri, "wt")?.use { out ->
                sav.inputStream().use { it.copyTo(out) }
            } ?: return false
        } catch (_: Throwable) {
            return false
        }
        return true
    }

    fun deleteInstance(id: String): Boolean {
        val dir = instanceDir(id)
        if (dir.exists()) {
            dir.deleteRecursively()
        }

        val next = readIndex().filterNot { it.id == id }
        writeIndex(next)
        return true
    }

    fun savLastWriteMillis(id: String): Long? {
        val f = savFile(id)
        if (!f.exists()) return null
        val t = f.lastModified()
        return if (t > 0L) t else null
    }

    private fun readIndex(): List<GameInstance> {
        ensureRoot()
        if (!indexFile.exists()) return emptyList()
        val raw = try {
            indexFile.readText()
        } catch (_: Throwable) {
            return emptyList()
        }
        val arr = try {
            JSONArray(raw)
        } catch (_: Throwable) {
            return emptyList()
        }
        val out = ArrayList<GameInstance>(arr.length())
        for (i in 0 until arr.length()) {
            val obj = arr.optJSONObject(i) ?: continue
            val id = obj.optString("id", "")
            val nickname = obj.optString("nickname", "")
            val romDisplayName = obj.optString("romDisplayName", "")
            if (id.isBlank() || nickname.isBlank() || romDisplayName.isBlank()) continue
            val createdAtMillis = obj.optLong("createdAtMillis", 0L)
            val lastSavExportMillis = if (obj.has("lastSavExportMillis") && !obj.isNull("lastSavExportMillis")) {
                obj.optLong("lastSavExportMillis")
            } else {
                null
            }
            out.add(
                GameInstance(
                    id = id,
                    nickname = nickname,
                    romDisplayName = romDisplayName,
                    createdAtMillis = createdAtMillis,
                    lastSavExportMillis = lastSavExportMillis,
                )
            )
        }
        return out
    }

    private fun writeIndex(instances: List<GameInstance>) {
        ensureRoot()
        val arr = JSONArray()
        for (inst in instances) {
            val obj = JSONObject()
            obj.put("id", inst.id)
            obj.put("nickname", inst.nickname)
            obj.put("romDisplayName", inst.romDisplayName)
            obj.put("createdAtMillis", inst.createdAtMillis)
            if (inst.lastSavExportMillis != null) {
                obj.put("lastSavExportMillis", inst.lastSavExportMillis)
            } else {
                obj.put("lastSavExportMillis", JSONObject.NULL)
            }
            arr.put(obj)
        }

        val tmp = File(rootDir, "instances.json.tmp")
        FileOutputStream(tmp).use { it.write(arr.toString().toByteArray()) }
        if (indexFile.exists()) indexFile.delete()
        tmp.renameTo(indexFile)
    }

    companion object {
        private const val ROM_FILENAME = "rom.gb"
        private const val SAV_FILENAME = "rom.sav"
        private const val RTC_FILENAME = "rom.rtc"
    }
}
