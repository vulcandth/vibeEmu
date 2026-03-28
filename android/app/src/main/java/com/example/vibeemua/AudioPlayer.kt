package com.example.vibeemua

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlin.math.max

class AudioPlayer(
    private val emulator: Emulator,
    private val scope: CoroutineScope,
) {
    private val sampleRate = 44_100
    private val channels = AudioFormat.CHANNEL_OUT_STEREO
    private val encoding = AudioFormat.ENCODING_PCM_16BIT
    private val frameBuf = ShortArray(512 * 2) // smaller buffer to reduce latency

    @Volatile
    private var track: AudioTrack? = null
    private var job: Job? = null

    fun start() {
        if (job != null) return
        job = scope.launch(Dispatchers.Default) {
            ensureTrack()
            while (isActive) {
                val t = track
                if (t == null) {
                    ensureTrack()
                    delay(20)
                    continue
                }
                if (!emulator.isReady()) {
                    delay(30)
                    continue
                }

                val frames = emulator.drainAudio(frameBuf)
                if (frames > 0) {
                    val samples = frames * 2
                    t.write(frameBuf, 0, samples, AudioTrack.WRITE_BLOCKING)
                } else {
                    // Keep the loop light if no samples available
                    delay(3)
                }
            }
        }
    }

    private fun ensureTrack() {
        if (track != null) return
        val minBuf = AudioTrack.getMinBufferSize(sampleRate, channels, encoding)
        val bufSize = max(minBuf, frameBuf.size * 2) // short -> bytes
        track = AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_GAME)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build()
            )
            .setAudioFormat(
                AudioFormat.Builder()
                    .setSampleRate(sampleRate)
                    .setChannelMask(channels)
                    .setEncoding(encoding)
                    .build()
            )
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setBufferSizeInBytes(bufSize)
            .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
            .build()
        track?.play()
    }

    fun stop() {
        job?.cancel()
        job = null
        track?.stop()
        track?.release()
        track = null
    }
}
