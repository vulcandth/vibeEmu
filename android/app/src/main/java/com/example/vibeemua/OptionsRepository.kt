package com.example.vibeemua

import android.content.Context

data class AppOptions(
    val emulationMode: EmulationMode = EmulationMode.Auto,
    val dmgNeutralPalette: Boolean = false,
    val serialPeripheral: SerialPeripheral = SerialPeripheral.None,
    val dmgBootRomEnabled: Boolean = false,
    val cgbBootRomEnabled: Boolean = false,
)

class OptionsRepository(context: Context) {
    private val prefs = context.getSharedPreferences("vibeEmuA_options", Context.MODE_PRIVATE)

    fun load(): AppOptions {
        val modeOrdinal = prefs.getInt(KEY_EMULATION_MODE, EmulationMode.Auto.ordinal)
        val mode = EmulationMode.entries.getOrNull(modeOrdinal) ?: EmulationMode.Auto
        val dmgNeutral = prefs.getBoolean(KEY_DMG_NEUTRAL, false)

        val serialOrdinal = prefs.getInt(KEY_SERIAL_PERIPHERAL, SerialPeripheral.None.ordinal)
        val serial = SerialPeripheral.entries.getOrNull(serialOrdinal) ?: SerialPeripheral.None

        val dmgBootRomEnabled = prefs.getBoolean(KEY_DMG_BOOTROM_ENABLED, false)
        val cgbBootRomEnabled = prefs.getBoolean(KEY_CGB_BOOTROM_ENABLED, false)

        return AppOptions(
            emulationMode = mode,
            dmgNeutralPalette = dmgNeutral,
            serialPeripheral = serial,
            dmgBootRomEnabled = dmgBootRomEnabled,
            cgbBootRomEnabled = cgbBootRomEnabled,
        )
    }

    fun save(options: AppOptions) {
        prefs.edit()
            .putInt(KEY_EMULATION_MODE, options.emulationMode.ordinal)
            .putBoolean(KEY_DMG_NEUTRAL, options.dmgNeutralPalette)
            .putInt(KEY_SERIAL_PERIPHERAL, options.serialPeripheral.ordinal)
            .putBoolean(KEY_DMG_BOOTROM_ENABLED, options.dmgBootRomEnabled)
            .putBoolean(KEY_CGB_BOOTROM_ENABLED, options.cgbBootRomEnabled)
            .apply()
    }

    private companion object {
        const val KEY_EMULATION_MODE = "emulation_mode"
        const val KEY_DMG_NEUTRAL = "dmg_neutral_palette"
        const val KEY_SERIAL_PERIPHERAL = "serial_peripheral"
        const val KEY_DMG_BOOTROM_ENABLED = "dmg_bootrom_enabled"
        const val KEY_CGB_BOOTROM_ENABLED = "cgb_bootrom_enabled"
    }
}
