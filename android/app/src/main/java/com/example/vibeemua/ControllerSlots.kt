package com.example.vibeemua

/** Stable SGB player slots. Keyboard/touch input is merged into player 1 separately. */
class ControllerSlots {
    private val devices = arrayOfNulls<Int>(4)
    private val buttons = IntArray(4)
    private val directions = IntArray(4)

    fun assign(deviceId: Int): Int {
        val existing = devices.indexOf(deviceId)
        if (existing >= 0) return existing
        val free = devices.indexOf(null)
        if (free >= 0) devices[free] = deviceId
        return free
    }

    fun setButton(deviceId: Int, mask: Int, pressed: Boolean) {
        val slot = assign(deviceId)
        if (slot < 0) return
        buttons[slot] = if (pressed) buttons[slot] or mask else buttons[slot] and mask.inv()
    }

    fun setDirections(deviceId: Int, mask: Int) {
        val slot = assign(deviceId)
        if (slot >= 0) directions[slot] = mask and 15
    }

    fun remove(deviceId: Int) {
        val slot = devices.indexOf(deviceId)
        if (slot >= 0) {
            devices[slot] = null
            buttons[slot] = 0
            directions[slot] = 0
        }
    }

    fun releaseButtons() { buttons.fill(0); directions.fill(0) }
    fun pressedMasks(): List<Int> = List(4) { buttons[it] or directions[it] }
    fun deviceIds(): List<Int?> = devices.toList()
}
