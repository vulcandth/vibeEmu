package com.example.vibeemua

import org.junit.Assert.*
import org.junit.Test

class HardwareOptionsTest {
    @Test fun persistedIdsAndBootFilesRemainCompatible() {
        assertEquals(0, EmulationMode.Auto.nativeId)
        assertEquals(1, EmulationMode.ForceDmg.nativeId)
        assertEquals(2, EmulationMode.ForceCgb.nativeId)
        assertEquals(12, EmulationMode.entries.map { it.nativeId }.distinct().size)
        assertEquals("bootrom_dmg.bin", BootRomMode.Dmg.fileName)
        assertEquals("bootrom_cgb.bin", BootRomMode.Cgb.fileName)
        assertEquals("dmg_bootrom_enabled", BootRomMode.Dmg.preferenceKey)
        assertEquals("cgb_bootrom_enabled", BootRomMode.Cgb.preferenceKey)
        assertEquals(3, BootRomMode.Sgb.nativeId)
        assertEquals(1, BootRomMode.Cgb.nativeId)
    }

    @Test fun bootSizesDistinguishColorAndMonochromeImages() {
        for (mode in BootRomMode.entries) {
            assertEquals(!mode.color, mode.acceptsSize(256))
            assertEquals(mode.color, mode.acceptsSize(2048))
            assertEquals(mode.color, mode.acceptsSize(2304))
            assertFalse(mode.acceptsSize(0))
            assertFalse(mode.acceptsSize(512))
        }
    }

    @Test fun nativeDimensionsOnlyAcceptSupportedFrames() {
        assertEquals(FrameSize(160, 144), FrameSize.fromPacked((160 shl 16) or 144))
        assertEquals(FrameSize(256, 224), FrameSize.fromPacked((256 shl 16) or 224))
        assertNull(FrameSize.fromPacked(0))
        assertNull(FrameSize.fromPacked(-1))
        assertNull(FrameSize.fromPacked((256 shl 16) or 144))
        assertEquals(256f / 224f, FrameSize(256, 224).aspect)
        assertTrue(FrameSize.MAX_PIXELS >= 256 * 224)
    }

    @Test fun controllerDisconnectDoesNotRenumberOtherPlayers() {
        val slots = ControllerSlots()
        for (id in 10..13) assertEquals(id - 10, slots.assign(id))
        assertEquals(-1, slots.assign(14))
        slots.setButton(11, 0x10, true)
        slots.setButton(12, 0x20, true)
        slots.remove(11)
        assertEquals(listOf(10, null, 12, 13), slots.deviceIds())
        assertEquals(listOf(0, 0, 0x20, 0), slots.pressedMasks())
        assertEquals(1, slots.assign(14))
        assertEquals(0, slots.pressedMasks()[1])
    }

    @Test fun analogAndButtonDirectionsDoNotReleaseEachOther() {
        val slots = ControllerSlots()
        slots.setButton(20, 0x01, true)
        slots.setButton(20, 0x10, true)
        slots.setDirections(20, 0x04)
        assertEquals(0x15, slots.pressedMasks()[0])
        slots.setDirections(20, 0)
        assertEquals(0x11, slots.pressedMasks()[0])
        slots.setButton(20, 0x01, false)
        assertEquals(0x10, slots.pressedMasks()[0])
        slots.releaseButtons()
        assertEquals(listOf(0, 0, 0, 0), slots.pressedMasks())
        assertEquals(0, slots.assign(20))
    }
}
