package com.example.vibeemua

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.concurrent.Executors

class SaveStatesTest {
    @get:Rule val compose = createComposeRule()

    @Test fun quickSaveLoadUndoAndReopenKeepDurableSlots() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "state-ui-test-${System.nanoTime()}").apply { mkdirs() }
        val dispatcher = Executors.newSingleThreadExecutor { r -> Thread(null, r, "StateTest", 16L * 1024 * 1024) }.asCoroutineDispatcher()
        val emulator = Emulator()
        try {
            val rom = File(root, "test.gb")
            val bytes = ByteArray(32768)
            bytes[0x100] = 0x18; bytes[0x101] = 0xfe.toByte()
            bytes[0x147] = 3; bytes[0x149] = 3
            rom.writeBytes(bytes)
            runBlocking { withContext(dispatcher) { assertTrue(emulator.loadRomFromFile(rom.absolutePath, EmulationMode.ForceDmg, emptyMap())) } }
            emulator.setPaused(true)
            var visible by mutableStateOf(true)
            compose.setContent { MaterialTheme { if (visible) SaveStatesDialog(emulator, dispatcher, root) { visible = false } } }
            fun ready(tag: String) {
                compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag(tag) and isEnabled()).fetchSemanticsNodes().isNotEmpty() }
            }
            ready("save-state-11")
            compose.onNodeWithTag("save-state-11").performScrollTo().performClick()
            ready("load-state-11")
            compose.onNodeWithTag("load-state-11").performScrollTo().performClick()
            ready("load-state-12")
            repeat(2) {
                compose.onNodeWithTag("load-state-12").performScrollTo().performClick()
                ready("load-state-12")
            }
            compose.onNodeWithText("Done").performClick()
            compose.runOnIdle { visible = true }
            ready("load-state-11")
            compose.onNodeWithTag("load-state-11").performScrollTo()
            compose.onAllNodes(isRoot()).onLast().captureToImage().asAndroidBitmap().let { bitmap ->
                File(context.getExternalFilesDir(null), "save-states.png").outputStream().use {
                    bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it)
                }
            }
        } finally {
            runBlocking { withContext(dispatcher) { emulator.close() } }
            dispatcher.close()
            root.deleteRecursively()
        }
    }
}
