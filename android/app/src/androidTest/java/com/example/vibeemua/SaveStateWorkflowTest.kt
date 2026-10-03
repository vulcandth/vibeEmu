package com.example.vibeemua

import android.graphics.Bitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File

/** Full activity, real JNI/frame loop and injected touch events (no semantic OnClick shortcut). */
class SaveStateWorkflowTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun quickLoadFromRunningGameRestoresStateAndReturnsToGameplay() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val repository = GameInstancesRepository(context)
        repository.ensureRoot()
        val index = File(context.filesDir, "instances/instances.json")
        val previousIndex = index.takeIf { it.exists() }?.readBytes()
        val id = "workflow-test-${System.nanoTime()}"
        val dir = repository.instanceDir(id).apply { mkdirs() }
        val data = ByteArray(32768)
        data[0x100] = 0xc3.toByte(); data[0x101] = 0x50; data[0x102] = 1
        data[0x147] = 3; data[0x149] = 3
        // A tiny generated ROM: advance a counter in WRAM continuously.
        byteArrayOf(0x21, 0, 0xc0.toByte(), 0x34, 0x18, 0xfd.toByte()).copyInto(data, 0x150)
        repository.romFile(id).writeBytes(data)
        index.writeText(JSONArray().put(JSONObject().put("id", id).put("nickname", "State workflow test")
            .put("romDisplayName", "generated.gb").put("createdAtMillis", 1)).toString())
        var activity: ActivityScenario<MainActivity>? = null
        lateinit var vm: EmulatorViewModel
        try {
            activity = ActivityScenario.launch(MainActivity::class.java)
            activity.onActivity { vm = ViewModelProvider(it)[EmulatorViewModel::class.java] }
            val states = File(dir, "states")
            fun exportCycles(): Long = runBlocking {
                withContext(vm.emuDispatcher) {
                    val file = File(dir, "inspection.vstate")
                    val reply = JSONObject(vm.emulator.stateOperation(states.absolutePath, 4, path = file.absolutePath))
                    assertTrue(reply.toString(), reply.getBoolean("ok"))
                    JSONObject(String(file.readBytes().drop(44).toByteArray())).getJSONObject("metadata").getLong("cycles")
                }
            }
            fun ready(tag: String) {
                compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag(tag) and isEnabled()).fetchSemanticsNodes().isNotEmpty() }
            }
            fun cancelPicker() {
                var nextBack = 0L
                compose.waitUntil(15_000) {
                    if (vm.states.document == null) true else {
                        val automation = InstrumentationRegistry.getInstrumentation().uiAutomation
                        // The create-document keyboard can consume the first Back.
                        // Never send another Back after focus returns to the app.
                        if (automation.rootInActiveWindow?.packageName.toString().contains("documentsui") &&
                            System.currentTimeMillis() >= nextBack) {
                            InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
                            nextBack = System.currentTimeMillis() + 1000
                        }
                        false
                    }
                }
            }
            fun capture(name: String) {
                compose.waitForIdle()
                Thread.sleep(250)
                InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot().let { bitmap ->
                    val values = android.content.ContentValues().apply {
                        put(android.provider.MediaStore.Images.Media.DISPLAY_NAME, name)
                        put(android.provider.MediaStore.Images.Media.MIME_TYPE, "image/png")
                        put(android.provider.MediaStore.Images.Media.RELATIVE_PATH, "Pictures/vibeemu-tests")
                    }
                    val uri = context.contentResolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)!!
                    context.contentResolver.openOutputStream(uri)!!.use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
                }
            }
            compose.onNodeWithText("State workflow test").performClick()
            compose.waitUntil(20_000) { vm.emulator.isReady() }
            compose.onNodeWithContentDescription("Menu").performClick()
            compose.onNodeWithText("Save states").performClick()
            ready("save-state-11")
            compose.onNodeWithTag("save-state-11").performScrollTo().performClick()
            ready("load-state-11")
            val savedCycles = exportCycles()
            compose.onNodeWithText("Done").performClick()
            compose.onNodeWithText("Resume").performClick()
            capture("quick-load-before-resume.png")
            compose.waitUntil(10_000) { !vm.emulator.isPaused() }
            Thread.sleep(1500)
            compose.onNodeWithContentDescription("Menu").performClick()
            compose.onNodeWithText("Save states").performClick()
            ready("load-state-11")
            val laterCycles = exportCycles()
            assertTrue("Game must advance: saved=$savedCycles later=$laterCycles paused=${vm.emulator.isPaused()}", laterCycles > savedCycles)
            compose.onNodeWithTag("load-state-11").performScrollTo().performClick()
            compose.onNodeWithTag("confirm-state").performClick()
            compose.waitUntil(20_000) { vm.states.completedLoad == 1 && !vm.states.busy }
            val restoredCycles = exportCycles()
            capture("quick-load-baseline.png")
            assertTrue("Load must rewind the running machine: saved=$savedCycles loaded=$restoredCycles later=$laterCycles", restoredCycles < laterCycles)
            // A successful Load must visibly return to the restored game, not
            // leave an off-screen status message behind the same paused dialog.
            compose.onNodeWithText("Save states").assertDoesNotExist()
            compose.waitUntil(5_000) { !vm.emulator.isPaused() }
            fun menu() {
                compose.onNodeWithContentDescription("Menu").performClick()
                compose.waitUntil(20_000) { !vm.states.busy && vm.states.rows.isNotEmpty() }
            }
            fun menuItem(label: String) { compose.onNodeWithText(label).performScrollTo().performClick() }
            fun instances() { menu(); menuItem("Return to instances") }
            val quickFile = states.walkTopDown().first { it.name == "quick.vstate" }
            var quickBytes = quickFile.readBytes()
            menu()
            capture("save-states-android-menu.png")
            menuItem("Quick save")
            compose.onNodeWithText("Cancel").performClick()
            assertArrayEquals("Cancel must preserve Quick", quickBytes, quickFile.readBytes())
            menuItem("Quick load")
            compose.onNodeWithText("Cancel").performClick()
            assertEquals(1, vm.states.completedLoad)
            menuItem("Quick save")
            val workerStarted = java.util.concurrent.CountDownLatch(1)
            val releaseWorker = java.util.concurrent.CountDownLatch(1)
            val blocker = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.Dispatchers.Default).launch {
                withContext(vm.emuDispatcher) {
                    workerStarted.countDown()
                    releaseWorker.await(10, java.util.concurrent.TimeUnit.SECONDS)
                }
            }
            assertTrue(workerStarted.await(5, java.util.concurrent.TimeUnit.SECONDS))
            try {
                compose.onNodeWithTag("confirm-state").performClick()
                compose.waitUntil(5_000) { vm.states.busy }
                activity.recreate()
                assertTrue("Rotation must retain pending work", vm.states.busy)
            } finally { releaseWorker.countDown() }
            runBlocking { blocker.join() }
            compose.waitUntil(20_000) { vm.states.completedQuickSave == 1 && !vm.states.busy }
            compose.waitUntil(5_000) { !vm.emulator.isPaused() }
            quickBytes = quickFile.readBytes()
            menu()

            menuItem("Save states")
            ready("save-state-1")
            compose.onNodeWithTag("save-state-1").performScrollTo().performClick()
            ready("load-state-1")
            activity.recreate()
            ready("load-state-11")
            assertArrayEquals(quickBytes, quickFile.readBytes())
            capture("save-states-android-slots.png")
            compose.onNodeWithText("Export current state").performScrollTo().performClick()
            compose.waitUntil(10_000) { vm.states.document != null }
            InstrumentationRegistry.getInstrumentation().uiAutomation.setRotation(android.app.UiAutomation.ROTATION_FREEZE_90)
            Thread.sleep(500)
            cancelPicker()
            compose.onNodeWithText("Export canceled.").assertIsDisplayed()
            InstrumentationRegistry.getInstrumentation().uiAutomation.setRotation(android.app.UiAutomation.ROTATION_FREEZE_0)
            compose.waitForIdle()
            compose.onNodeWithText("Import and load a state").performScrollTo().performClick()
            compose.onNodeWithText("Choose state file").performClick()
            compose.waitUntil(10_000) { vm.states.document != null }
            cancelPicker()
            compose.onNodeWithText("Import canceled.").assertIsDisplayed()

            // Undo swaps the pre-load state repeatedly, then visibly resumes each time.
            repeat(2) { undo ->
                if (undo > 0) { menu(); menuItem("Save states") }
                ready("load-state-12")
                compose.onNodeWithTag("load-state-12").performScrollTo().performClick()
                compose.onNodeWithTag("confirm-state").performClick()
                compose.waitUntil(20_000) { vm.states.completedLoad == undo + 2 && !vm.states.busy }
                compose.onNodeWithText("Save states").assertDoesNotExist()
            }
            instances()
            val battery = repository.savFile(id).readBytes()
            val recoveryFile = states.walkTopDown().first { it.name == "recovery.vstate" }
            val recoveryBytes = recoveryFile.readBytes()
            compose.onNodeWithText("State workflow test").performClick()
            compose.waitUntil(20_000) { vm.states.prepared != null && !vm.states.busy }
            compose.onNodeWithText("Resume State workflow test?").assertIsDisplayed()
            capture("save-states-android-resume.png")
            activity.recreate()
            compose.onNodeWithText("Resume State workflow test?").assertIsDisplayed()
            compose.onNodeWithText("Cancel").performClick()
            assertArrayEquals("Cancel must not write candidate SRAM", battery, repository.savFile(id).readBytes())
            assertArrayEquals(recoveryBytes, recoveryFile.readBytes())
            assertArrayEquals(quickBytes, quickFile.readBytes())
            compose.onNodeWithText("State workflow test").performClick()
            compose.waitUntil(20_000) { vm.states.prepared != null && !vm.states.busy }
            compose.waitForIdle()
            InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
            compose.waitUntil(10_000) { vm.states.prepared == null }
            assertArrayEquals(battery, repository.savFile(id).readBytes())
            assertArrayEquals(recoveryBytes, recoveryFile.readBytes())
            // A disappearing selected file reports failure and leaves the launch choice open.
            compose.onNodeWithText("State workflow test").performClick()
            compose.waitUntil(20_000) { vm.states.prepared != null && !vm.states.busy }
            compose.onNodeWithTag("resume-slot-11").performClick()
            quickFile.delete()
            compose.onNodeWithText("Resume saved game").performClick()
            compose.waitUntil(20_000) { vm.states.error != null && !vm.states.busy }
            compose.onNodeWithText("Resume State workflow test?").assertIsDisplayed()
            assertArrayEquals(recoveryBytes, recoveryFile.readBytes())
            quickFile.writeBytes(quickBytes)
            compose.onNodeWithText("Start without loading").assertIsDisplayed().performClick()
            compose.waitUntil(20_000) { vm.states.prepared == null && !vm.states.busy }
            compose.waitUntil(5_000) { !vm.emulator.isPaused() }
            instances()
            // A new activity and ViewModel must discover durable slots without auto-loading.
            activity.close()
            activity = ActivityScenario.launch(MainActivity::class.java)
            activity.onActivity { vm = ViewModelProvider(it)[EmulatorViewModel::class.java] }
            assertFalse(vm.emulator.isReady())
            compose.onNodeWithText("State workflow test").performClick()
            compose.waitUntil(20_000) { vm.states.prepared != null && !vm.states.busy }
            compose.onNodeWithTag("resume-slot-11").performClick()
            compose.onNodeWithText("Resume saved game").performClick()
            compose.waitUntil(20_000) { vm.emulator.isReady() && vm.states.prepared == null && !vm.states.busy }
            compose.waitUntil(5_000) { !vm.emulator.isPaused() }
            capture("save-states-android-restored.png")

        } catch (failure: Throwable) {
            runCatching { println(compose.onAllNodes(isRoot(), useUnmergedTree = true).printToString()) }
            println("FILES: " + dir.walkTopDown().filter { it.isFile }.map { "${it.relativeTo(dir)}=${it.length()}" }.toList())
            println("SLOTS: " + runBlocking { withContext(vm.emuDispatcher) { vm.emulator.stateOperation(File(dir, "states").absolutePath, 0) } })
            InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot().let { bitmap ->
                File(context.getExternalFilesDir(null), "quick-load-failure.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            }
            throw failure
        } finally {
            InstrumentationRegistry.getInstrumentation().uiAutomation.setRotation(android.app.UiAutomation.ROTATION_UNFREEZE)
            activity?.close()
            if (previousIndex == null) index.delete() else index.writeBytes(previousIndex)
            dir.deleteRecursively()
        }
    }
}
