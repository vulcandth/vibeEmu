package com.example.vibeemua

import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.junit.Assert.*
import org.junit.Test

class MachineSettingsTest {
    private fun inDirectory(test: (File) -> Unit) {
        val directory = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "machine-test-${UUID.randomUUID()}")
        check(directory.mkdirs())
        try { test(directory) } finally { directory.deleteRecursively() }
    }

    @Test fun discardRecoversReplacedBootAndModelWhileKeepingImmediateSettings() = inDirectory { directory ->
        val original = ByteArray(256) { 1 }
        File(directory, BootRomMode.Dmg.fileName).writeBytes(original)
        val initial = AppOptions(enabledBootRoms = setOf(BootRomMode.Dmg))
        var store = MachineSettingsStore(directory)
        var pending = store.stageBoot(initial, BootRomMode.Dmg, ByteArray(256) { 2 })
        val next = pending.copy(emulationMode = EmulationMode.ForceSgb, volume = 42)
        store.stage(pending, next)
        // Recreate the store as after process death: both choices and imported bytes survive.
        store = MachineSettingsStore(directory)
        pending = store.load(next)
        assertEquals(EmulationMode.ForceSgb, pending.emulationMode)
        assertArrayEquals(ByteArray(256) { 2 }, store.readBootRoms(pending)[BootRomMode.Dmg])
        assertArrayEquals(original, File(directory, BootRomMode.Dmg.fileName).readBytes())
        val restored = store.discard(pending)
        assertEquals(initial.emulationMode, restored.emulationMode)
        assertEquals(42, restored.volume)
        assertArrayEquals(original, store.readBootRoms(restored)[BootRomMode.Dmg])
        assertFalse(store.hasPendingChanges())
    }

    @Test fun committedImportsAndClearsSurviveRestartAndDiscard() = inDirectory { directory ->
        var store = MachineSettingsStore(directory)
        val imported = store.stageBoot(AppOptions(), BootRomMode.Cgb, ByteArray(2048) { 3 })
        store.commit()
        store = MachineSettingsStore(directory)
        val committed = store.load(AppOptions())
        assertEquals(imported, committed)
        val cleared = store.stageBoot(committed, BootRomMode.Cgb, null)
        assertNull(store.bootFile(cleared, BootRomMode.Cgb))
        val restored = store.discard(cleared)
        assertArrayEquals(ByteArray(2048) { 3 }, store.readBootRoms(restored)[BootRomMode.Cgb])
        store.stageBoot(restored, BootRomMode.Cgb, null)
        store.commit()
        assertTrue(store.load(AppOptions()).enabledBootRoms.isEmpty())
        assertNull(store.bootFile(store.load(AppOptions()), BootRomMode.Cgb))
    }

    @Test fun rejectedReloadKeepsRunningMachineAndPendingConfiguration() = inDirectory { directory ->
        val rom = ByteArray(32768)
        rom[0x100] = 0x18; rom[0x101] = 0xfe.toByte() // JR -2, no copyrighted ROM required.
        val romFile = File(directory, "test.gb").apply { writeBytes(rom) }
        val emulator = Emulator()
        try {
            assertTrue(emulator.loadRomFromFile(romFile.path, EmulationMode.ForceDmg, emptyMap()))
            assertFalse(emulator.loadRomFromFile(File(directory, "missing.gb").path, EmulationMode.ForceSgb, emptyMap()))
            assertTrue(emulator.isReady())
            assertFalse(emulator.isSgbHost)
            assertNotNull(emulator.renderFrame(IntArray(FrameSize.MAX_PIXELS)))
            assertFalse(emulator.loadRomFromFile(romFile.path, EmulationMode.ForceSgb, mapOf(BootRomMode.Sgb to ByteArray(3))))
            assertTrue(emulator.isReady())
            val store = MachineSettingsStore(directory)
            store.stage(AppOptions(), AppOptions(emulationMode = EmulationMode.ForceCgb))
            try {
                emulator.loadRomFromFile(romFile.path, EmulationMode.ForceCgb, emptyMap()) { error("Simulated settings write failure") }
                fail("Expected failed commit")
            } catch (_: IllegalStateException) { }
            assertTrue(store.hasPendingChanges())
            assertTrue(emulator.isReady())
            assertNotNull(emulator.renderFrame(IntArray(FrameSize.MAX_PIXELS)))
            emulator.setPaused(true)
            assertTrue(emulator.loadRomFromFile(romFile.path, EmulationMode.ForceCgb, emptyMap(), store::commit))
            assertTrue(emulator.isPaused())
            assertFalse(store.hasPendingChanges())
        } finally { emulator.close() }
    }
}
