package com.example.vibeemua

enum class EmulationMode(val label: String) {
    Auto("Auto"),
    ForceDmg("Force DMG"),
    ForceCgb("Force CGB"),
}

enum class BootRomMode {
    Dmg,
    Cgb,
}

class NativeBridge {
    companion object {
        init {
            System.loadLibrary("vibe_emu_android")
        }
    }

    external fun create(emulationMode: Int): Long
    external fun destroy(handle: Long)
    external fun loadRom(handle: Long, rom: ByteArray): Boolean
    external fun loadRomFile(handle: Long, path: String): Boolean
    external fun runFrame(handle: Long, buffer: IntArray): Boolean
    external fun setInput(handle: Long, state: Int)
    external fun reset(handle: Long)
    external fun drainAudio(handle: Long, buffer: ShortArray): Int
    external fun saveRam(handle: Long)

    external fun setDmgNeutralPalette(handle: Long, enabled: Boolean)

    external fun setBootRom(handle: Long, mode: Int, data: ByteArray)
    external fun clearBootRom(handle: Long, mode: Int)

    external fun enableMobileAdapter(handle: Long, configPath: String): Boolean
    external fun disableMobileAdapter(handle: Long)
}

class Emulator(private val native: NativeBridge = NativeBridge()) {
    private var handle: Long = 0
    private var romLoaded: Boolean = false

    private val nativeLock = Any()

    @Volatile
    private var paused: Boolean = false

    fun isReady(): Boolean = handle != 0L && romLoaded

    fun isPaused(): Boolean = paused

    fun setPaused(paused: Boolean) {
        this.paused = paused
    }

    fun loadRomFromFile(
        path: String,
        emulationMode: EmulationMode,
        dmgBootRom: ByteArray?,
        cgbBootRom: ByteArray?,
    ): Boolean {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.destroy(handle)
                handle = 0
            }

            handle = native.create(emulationMode.ordinal)
            if (handle == 0L) {
                romLoaded = false
                return false
            }

            // Configure boot ROM(s) before loading the game cartridge.
            native.clearBootRom(handle, BootRomMode.Dmg.ordinal)
            native.clearBootRom(handle, BootRomMode.Cgb.ordinal)
            if (dmgBootRom != null) {
                native.setBootRom(handle, BootRomMode.Dmg.ordinal, dmgBootRom)
            }
            if (cgbBootRom != null) {
                native.setBootRom(handle, BootRomMode.Cgb.ordinal, cgbBootRom)
            }

            romLoaded = native.loadRomFile(handle, path)
            if (!romLoaded) {
                native.destroy(handle)
                handle = 0
            }
            return romLoaded
        }
    }

    fun setDmgNeutralPalette(enabled: Boolean) {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.setDmgNeutralPalette(handle, enabled)
            }
        }
    }

    fun setBootRom(mode: BootRomMode, data: ByteArray) {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.setBootRom(handle, mode.ordinal, data)
            }
        }
    }

    fun clearBootRom(mode: BootRomMode) {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.clearBootRom(handle, mode.ordinal)
            }
        }
    }

    fun renderFrame(out: IntArray): Boolean {
        if (!isReady() || paused) return false
        synchronized(nativeLock) {
            if (!isReady() || paused) return false
            return native.runFrame(handle, out)
        }
    }

    fun updateInput(state: Int) {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.setInput(handle, state)
            }
        }
    }

    fun reset() {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.reset(handle)
            }
        }
    }

    fun close() {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.saveRam(handle)
                native.destroy(handle)
                handle = 0
            }
            romLoaded = false
        }
    }

    fun drainAudio(buffer: ShortArray): Int {
        if (!isReady() || paused) return 0
        synchronized(nativeLock) {
            if (!isReady() || paused) return 0
            return native.drainAudio(handle, buffer)
        }
    }

    fun saveRam() {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.saveRam(handle)
            }
        }
    }

    fun enableMobileAdapter(configPath: String): Boolean {
        synchronized(nativeLock) {
            if (handle == 0L) return false
            return native.enableMobileAdapter(handle, configPath)
        }
    }

    fun disableMobileAdapter() {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.disableMobileAdapter(handle)
            }
        }
    }
}
