package com.example.vibeemua

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.ExecutorCoroutineDispatcher
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import java.util.concurrent.Executors

class EmulatorViewModel : ViewModel() {
    val emulator: Emulator = Emulator()

    val emuDispatcher: ExecutorCoroutineDispatcher = Executors
        .newSingleThreadExecutor { r -> Thread(null, r, "EmuThread", 8L * 1024 * 1024).apply { isDaemon = true } }
        .asCoroutineDispatcher()

    private val sessionScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    val states = SaveStateSession(emulator, emuDispatcher, sessionScope)

    override fun onCleared() {
        sessionScope.cancel()
        states.close()
        try {
            emulator.saveRam()
        } catch (_: Throwable) {
        }
        try {
            emulator.close()
        } catch (_: Throwable) {
        }
        emuDispatcher.close()
    }
}
