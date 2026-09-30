package com.example.vibeemua

import android.content.Context

data class AppOptions(
    val soundEnabled: Boolean = true,
    val volume: Int = 100,
    val mono: Boolean = false,
    val speedPercent: Int = 100,
    val hideTouchWithController: Boolean = true,
    val emulationMode: EmulationMode = EmulationMode.Auto,
    val dmgNeutralPalette: Boolean = false,
    val serialPeripheral: SerialPeripheral = SerialPeripheral.None,
    val enabledBootRoms: Set<BootRomMode> = emptySet(),
    val showSgbBorder: Boolean = true,
)

class OptionsRepository(context: Context) {
    private val prefs = context.getSharedPreferences("vibeEmuA_options", Context.MODE_PRIVATE)

    fun load(): AppOptions {
        val modeId = prefs.getInt("emulation_mode", EmulationMode.Auto.nativeId)
        val serialOrdinal = prefs.getInt("serial_peripheral", SerialPeripheral.None.ordinal)
        return AppOptions(
            soundEnabled = prefs.getBoolean("sound_enabled", true),
            volume = prefs.getInt("volume", 100).coerceIn(0, 100),
            mono = prefs.getBoolean("mono", false),
            speedPercent = prefs.getInt("speed_percent", 100).coerceIn(1, 400),
            hideTouchWithController = prefs.getBoolean("hide_touch_with_controller", true),
            emulationMode = EmulationMode.entries.firstOrNull { it.nativeId == modeId } ?: EmulationMode.Auto,
            dmgNeutralPalette = prefs.getBoolean("dmg_neutral_palette", false),
            serialPeripheral = SerialPeripheral.entries.getOrNull(serialOrdinal) ?: SerialPeripheral.None,
            // DMG/CGB keys and filenames remain compatible with existing installs.
            enabledBootRoms = BootRomMode.entries.filterTo(mutableSetOf()) {
                prefs.getBoolean(it.preferenceKey, false)
            },
            showSgbBorder = prefs.getBoolean("show_sgb_border", true),
        )
    }

    fun save(options: AppOptions) {
        val editor = prefs.edit()
            .putBoolean("sound_enabled", options.soundEnabled)
            .putInt("volume", options.volume.coerceIn(0, 100))
            .putBoolean("mono", options.mono)
            .putInt("speed_percent", options.speedPercent.coerceIn(1, 400))
            .putBoolean("hide_touch_with_controller", options.hideTouchWithController)
            .putInt("emulation_mode", options.emulationMode.nativeId)
            .putBoolean("dmg_neutral_palette", options.dmgNeutralPalette)
            .putInt("serial_peripheral", options.serialPeripheral.ordinal)
            .putBoolean("show_sgb_border", options.showSgbBorder)
        BootRomMode.entries.forEach { editor.putBoolean(it.preferenceKey, it in options.enabledBootRoms) }
        editor.apply()
    }
}
