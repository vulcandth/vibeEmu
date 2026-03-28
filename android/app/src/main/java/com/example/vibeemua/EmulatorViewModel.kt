package com.example.vibeemua

import androidx.lifecycle.ViewModel
import kotlinx.coroutines.ExecutorCoroutineDispatcher
import kotlinx.coroutines.asCoroutineDispatcher
import java.util.concurrent.Executors

class EmulatorViewModel : ViewModel() {
    val emulator: Emulator = Emulator()

    val emuDispatcher: ExecutorCoroutineDispatcher = Executors
        .newSingleThreadExecutor { r -> Thread(r, "EmuThread").apply { isDaemon = true } }
        .asCoroutineDispatcher()

    override fun onCleared() {
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
