package com.example.vibeemua

// IDs are persisted and passed to JNI; never renumber existing entries.
enum class EmulationMode(val nativeId: Int, val label: String) {
    Auto(0, "Game Boy or GBC (no SGB)"),
    ForceDmg(1, "Force DMG"),
    ForceCgb(2, "Force CGB"),
    ForceMgb(3, "Force MGB"),
    ForceSgb(4, "Force SGB"),
    ForceSgb2(5, "Force SGB2"),
    ForceAgb0(6, "Force AGB0"),
    ForceAgb(7, "Force AGB"),
    CgbSgb(8, "SGB + GBC"),
    CgbInitialBorder(9, "GBC + initial SGB border"),
    AutoPreferCgb(10, "Automatic, prefer GBC"),
    AutoPreferSgb(11, "Automatic, prefer SGB"),
}

enum class BootRomMode(val nativeId: Int, val label: String, val color: Boolean = false) {
    Dmg(0, "DMG"), Cgb(1, "CGB", true), Mgb(2, "MGB"),
    Sgb(3, "SGB"), Sgb2(4, "SGB2"), Agb0(5, "AGB0", true), Agb(6, "AGB", true);

    val fileName: String get() = "bootrom_${name.lowercase()}.bin"
    val preferenceKey: String get() = "${name.lowercase()}_bootrom_enabled"
    fun acceptsSize(size: Int): Boolean = if (color) size == 0x800 || size == 0x900 else size == 0x100
}

data class FrameSize(val width: Int, val height: Int) {
    val aspect: Float get() = width.toFloat() / height
    companion object {
        const val MAX_PIXELS = 256 * 224
        fun fromPacked(value: Int): FrameSize? {
            val size = FrameSize(value ushr 16, value and 0xffff)
            return size.takeIf { it == FrameSize(160, 144) || it == FrameSize(256, 224) }
        }
    }
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
    external fun runFrame(handle: Long, buffer: IntArray): Int
    external fun setInput(handle: Long, state: Int)
    external fun setPlayerInput(handle: Long, player: Int, state: Int)
    external fun setShowBorder(handle: Long, show: Boolean)
    external fun clockHz(handle: Long): Int
    external fun sgbHost(handle: Long): Boolean
    external fun reset(handle: Long)
    external fun drainAudio(handle: Long, buffer: ShortArray): Int
    external fun saveRam(handle: Long)

    external fun setDmgNeutralPalette(handle: Long, enabled: Boolean)

    external fun setBootRom(handle: Long, mode: Int, data: ByteArray): Boolean
    external fun clearBootRom(handle: Long, mode: Int)

    external fun enableMobileAdapter(handle: Long, configPath: String): Boolean
    external fun disableMobileAdapter(handle: Long)
}

class Emulator(private val native: NativeBridge = NativeBridge()) {
    @Volatile var outputVolume: Int = 100
    @Volatile var monoOutput: Boolean = false
    @Volatile var speedPercent: Int = 100
    @Volatile private var handle: Long = 0
    @Volatile private var romLoaded: Boolean = false
    @Volatile var isSgbHost: Boolean = false
        private set
    @Volatile var frameDurationNs: Long = 70_224_000_000_000L / 4_194_304L
        private set

    private val nativeLock = Any()

    @Volatile
    private var paused: Boolean = false

    @Volatile private var foreground: Boolean = true

    fun isReady(): Boolean = handle != 0L && romLoaded

    fun isPaused(): Boolean = paused || !foreground

    fun setForeground(foreground: Boolean) {
        this.foreground = foreground
    }

    fun setPaused(paused: Boolean) {
        this.paused = paused
    }

    fun loadRomFromFile(
        path: String,
        emulationMode: EmulationMode,
        bootRoms: Map<BootRomMode, ByteArray>,
        beforeSwap: () -> Unit = {},
    ): Boolean {
        synchronized(nativeLock) {
            // Flush before the candidate reads the same cartridge's battery save.
            if (handle != 0L) native.saveRam(handle)
            val candidate = native.create(emulationMode.nativeId)
            if (candidate == 0L) return false
            var installed = false
            try {
                for ((mode, bytes) in bootRoms) {
                    if (!native.setBootRom(candidate, mode.nativeId, bytes)) return false
                }
                if (!native.loadRomFile(candidate, path)) return false
                val nextHost = native.sgbHost(candidate)
                val nextDuration = 70_224_000_000_000L / native.clockHz(candidate)
                beforeSwap()
                if (handle != 0L) native.destroy(handle)
                handle = candidate
                installed = true
                romLoaded = true
                isSgbHost = nextHost
                frameDurationNs = nextDuration
                return true
            } finally {
                if (!installed) native.destroy(candidate)
            }
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
                native.setBootRom(handle, mode.nativeId, data)
            }
        }
    }

    fun clearBootRom(mode: BootRomMode) {
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.clearBootRom(handle, mode.nativeId)
            }
        }
    }

    fun renderFrame(out: IntArray): FrameSize? {
        if (!isReady() || isPaused()) return null
        synchronized(nativeLock) {
            if (!isReady() || isPaused()) return null
            return FrameSize.fromPacked(native.runFrame(handle, out))
        }
    }

    fun updateInput(state: Int, player: Int = 0) {
        if (!isReady()) return
        synchronized(nativeLock) {
            if (handle != 0L) {
                native.setPlayerInput(handle, player, state)
            }
        }
    }

    fun setShowBorder(show: Boolean) {
        synchronized(nativeLock) {
            if (handle != 0L) native.setShowBorder(handle, show)
        }
    }

    fun reset() {
        if (!isReady()) return
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
        if (!isReady() || isPaused()) return 0
        synchronized(nativeLock) {
            if (!isReady() || isPaused()) return 0
            return native.drainAudio(handle, buffer)
        }
    }

    fun saveRam() {
        if (!isReady()) return
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
