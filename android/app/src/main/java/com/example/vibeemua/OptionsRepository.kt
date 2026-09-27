package com.example.vibeemua

import android.content.Context

data class AppOptions(
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
            .putInt("emulation_mode", options.emulationMode.nativeId)
            .putBoolean("dmg_neutral_palette", options.dmgNeutralPalette)
            .putInt("serial_peripheral", options.serialPeripheral.ordinal)
            .putBoolean("show_sgb_border", options.showSgbBorder)
        BootRomMode.entries.forEach { editor.putBoolean(it.preferenceKey, it in options.enabledBootRoms) }
        editor.apply()
    }
}
