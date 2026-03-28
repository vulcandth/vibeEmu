package com.example.vibeemua

import android.content.Context
import android.view.InputDevice
import android.view.KeyEvent

enum class InputAction(val label: String, val mask: Int) {
    Up("Up", 0x04),
    Down("Down", 0x08),
    Left("Left", 0x02),
    Right("Right", 0x01),
    A("A", 0x10),
    B("B", 0x20),
    Select("Select", 0x40),
    Start("Start", 0x80),
}

data class InputMappings(
    val keyboard: Map<InputAction, Int>,
    val controller: Map<InputAction, Int>,
) {
    fun maskForKeyEvent(event: KeyEvent, isController: Boolean): Int {
        val map = if (isController) controller else keyboard
        val keyCode = event.keyCode
        val action = map.entries.firstOrNull { it.value == keyCode }?.key
        return action?.mask ?: 0
    }
}

class InputMappingsRepository(context: Context) {
    private val prefs = context.getSharedPreferences("vibeEmuA_input_mappings", Context.MODE_PRIVATE)

    fun load(): InputMappings {
        val keyboard = InputAction.entries.associateWith { a ->
            prefs.getInt("kb_${a.name}", defaultKeyboard(a))
        }
        val controller = InputAction.entries.associateWith { a ->
            prefs.getInt("pad_${a.name}", defaultController(a))
        }
        return InputMappings(keyboard = keyboard, controller = controller)
    }

    fun save(mappings: InputMappings) {
        val editor = prefs.edit()
        mappings.keyboard.forEach { (a, code) -> editor.putInt("kb_${a.name}", code) }
        mappings.controller.forEach { (a, code) -> editor.putInt("pad_${a.name}", code) }
        editor.apply()
    }

    private fun defaultController(a: InputAction): Int {
        return when (a) {
            InputAction.Up -> KeyEvent.KEYCODE_DPAD_UP
            InputAction.Down -> KeyEvent.KEYCODE_DPAD_DOWN
            InputAction.Left -> KeyEvent.KEYCODE_DPAD_LEFT
            InputAction.Right -> KeyEvent.KEYCODE_DPAD_RIGHT
            InputAction.A -> KeyEvent.KEYCODE_BUTTON_A
            InputAction.B -> KeyEvent.KEYCODE_BUTTON_B
            InputAction.Start -> KeyEvent.KEYCODE_BUTTON_START
            InputAction.Select -> KeyEvent.KEYCODE_BUTTON_SELECT
        }
    }

    private fun defaultKeyboard(a: InputAction): Int {
        return when (a) {
            InputAction.Up -> KeyEvent.KEYCODE_DPAD_UP
            InputAction.Down -> KeyEvent.KEYCODE_DPAD_DOWN
            InputAction.Left -> KeyEvent.KEYCODE_DPAD_LEFT
            InputAction.Right -> KeyEvent.KEYCODE_DPAD_RIGHT
            InputAction.A -> KeyEvent.KEYCODE_X
            InputAction.B -> KeyEvent.KEYCODE_Z
            InputAction.Start -> KeyEvent.KEYCODE_ENTER
            InputAction.Select -> KeyEvent.KEYCODE_SHIFT_LEFT
        }
    }
}

object InputMappingStore {
    @Volatile
    private var current: InputMappings = InputMappings(
        keyboard = InputAction.entries.associateWith { KeyEvent.KEYCODE_UNKNOWN },
        controller = InputAction.entries.associateWith { KeyEvent.KEYCODE_UNKNOWN },
    )

    fun init(context: Context) {
        current = InputMappingsRepository(context).load()
    }

    fun get(): InputMappings = current

    fun set(context: Context, mappings: InputMappings) {
        current = mappings
        InputMappingsRepository(context).save(mappings)
    }
}

data class PendingKeyCapture(
    val action: InputAction,
    val forController: Boolean,
    val onCaptured: (Int) -> Unit,
)

object KeyCapture {
    @Volatile
    var pending: PendingKeyCapture? = null
        private set

    fun request(action: InputAction, forController: Boolean, onCaptured: (Int) -> Unit) {
        pending = PendingKeyCapture(action, forController, onCaptured)
    }

    fun cancel() {
        pending = null
    }

    fun isCapturing(): Boolean = pending != null

    fun maybeConsumeKeyDown(event: KeyEvent): Boolean {
        val p = pending ?: return false
        if (event.action != KeyEvent.ACTION_DOWN) return true

        val code = event.keyCode
        if (code == KeyEvent.KEYCODE_BACK || code == KeyEvent.KEYCODE_MENU) {
            return false
        }

        pending = null
        p.onCaptured(code)
        return true
    }
}

fun isGameControllerDevice(device: InputDevice?): Boolean {
    val sources = device?.sources ?: return false
    return (sources and InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD ||
        (sources and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK ||
        (sources and InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD
}
