package com.example.vibeemua

import android.net.Uri
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.concurrent.Executors

/** Real ContentResolver/JNI boundary; system picker interactions are in SaveStateWorkflowTest. */
class StateDocumentsTest {
    @get:Rule val compose = createComposeRule()

    @Test fun documentFailuresPreserveMachineAndRecoveryAndSuccessfulImportCanBeUndone() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "documents-${System.nanoTime()}").apply { mkdirs() }
        val dispatcher = Executors.newSingleThreadExecutor { r -> Thread(null, r, "DocumentsTest", 16L * 1024 * 1024) }.asCoroutineDispatcher()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val emulator = Emulator()
        val session = SaveStateSession(emulator, dispatcher, scope)
        try {
            val data = ByteArray(32768)
            data[0x100] = 0x18; data[0x101] = 0xfe.toByte()
            data[0x147] = 3; data[0x149] = 3
            val rom = File(root, "rom.gb").apply { writeBytes(data) }
            runBlocking { withContext(dispatcher) { assertTrue(emulator.loadRomFromFile(rom.path, EmulationMode.ForceDmg, emptyMap())) } }
            emulator.setPaused(true)
            compose.runOnIdle { session.bind("documents", root) }
            fun complete(import: Boolean, uri: Uri?) {
                compose.runOnIdle { assertTrue(session.beginDocument(import)); session.completeDocument(context, uri) }
                compose.waitUntil(20_000) { !session.busy }
            }
            val exported = File(root, "exported.vstate")
            complete(false, Uri.fromFile(exported))
            assertTrue(session.message, exported.length() > 44)
            val snapshot = exported.readBytes()
            complete(true, Uri.fromFile(exported))
            assertEquals(1, session.completedLoad)
            val recovery = root.walkTopDown().first { it.name == "recovery.vstate" }
            val previousRecovery = recovery.readBytes()
            val broken = File(root, "broken.vstate").apply { writeText("broken") }
            val huge = File(root, "huge.vstate").apply {
                java.io.RandomAccessFile(this, "rw").use { it.setLength(16L * 1024 * 1024 + 1) }
            }
            for (uri in listOf(Uri.fromFile(broken), Uri.fromFile(huge), Uri.parse("content://missing.provider/state"))) {
                complete(true, uri)
                assertEquals(1, session.completedLoad)
                assertArrayEquals(previousRecovery, recovery.readBytes())
                val inspection = File(root, "inspect.vstate")
                complete(false, Uri.fromFile(inspection))
                fun machine(bytes: ByteArray) = JSONObject(String(bytes.copyOfRange(44, bytes.size))).getJSONObject("machine").toString()
                assertEquals(machine(snapshot), machine(inspection.readBytes()))
            }
            complete(false, Uri.parse("content://missing.provider/state"))
            assertFalse(session.message.startsWith("Current state exported"))
            complete(true, null)
            assertEquals("Import canceled.", session.message)
            complete(false, null)
            assertEquals("Export canceled.", session.message)
            compose.runOnIdle {
                assertTrue(session.beginDocument(true))
                try { session.bind("another-instance", File(root, "another")); fail("Must bind picker to its original instance") }
                catch (_: IllegalStateException) { }
                session.completeDocument(context, null)
                session.operate(2, 12)
            }
            compose.waitUntil(20_000) { !session.busy }
            assertEquals(2, session.completedLoad)
        } finally {
            scope.cancel()
            runBlocking { withContext(dispatcher) { emulator.close() } }
            dispatcher.close()
            root.deleteRecursively()
        }
    }
}
