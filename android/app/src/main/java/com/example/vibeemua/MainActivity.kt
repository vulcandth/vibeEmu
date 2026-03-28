package com.example.vibeemua

import android.graphics.Bitmap
import android.graphics.Paint
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.WindowManager
import android.view.SurfaceHolder
import android.view.SurfaceView
import java.io.File
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import com.example.vibeemua.AudioPlayer
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.RadioButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.AlertDialog
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.foundation.clickable
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.viewinterop.AndroidView
import com.example.vibeemua.ui.theme.VibeEmuATheme
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Menu
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExecutorCoroutineDispatcher
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext
import android.net.Uri
import android.provider.OpenableColumns
import java.util.concurrent.locks.LockSupport
import androidx.compose.runtime.mutableIntStateOf
import kotlin.math.abs
import android.content.res.Configuration
import androidx.compose.ui.graphics.RectangleShape

private fun computeDestRect(canvasWidth: Int, canvasHeight: Int, out: Rect) {
    // Letterbox while preserving aspect ratio.
    val targetRatio = FB_WIDTH.toFloat() / FB_HEIGHT.toFloat()
    val canvasRatio = canvasWidth.toFloat() / canvasHeight.toFloat()
    if (canvasRatio > targetRatio) {
        val scaledWidth = (canvasHeight * targetRatio).toInt()
        val left = (canvasWidth - scaledWidth) / 2
        out.set(left, 0, left + scaledWidth, canvasHeight)
    } else {
        val scaledHeight = (canvasWidth / targetRatio).toInt()
        val top = (canvasHeight - scaledHeight) / 2
        out.set(0, top, canvasWidth, top + scaledHeight)
    }
}

private enum class UiScreen {
    Instances,
    Emulator,
    Options,
    About,
    AndroidLicenses,
    ThirdPartyLicenses,
}

private fun sleepForNanos(nanos: Long) {
    if (nanos <= 0L) return
    // This runs on a dedicated emulation thread. Avoid coroutine-delay timer granularity
    // issues on some devices by blocking the emu thread directly.
    val ms = nanos / 1_000_000L
    val ns = (nanos % 1_000_000L).toInt()
    if (ms > 0L) {
        try {
            Thread.sleep(ms, ns)
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
        }
    } else {
        LockSupport.parkNanos(nanos)
    }
}

private const val FB_WIDTH = 160
private const val FB_HEIGHT = 144

private const val MASK_RIGHT = 0x01
private const val MASK_LEFT = 0x02
private const val MASK_UP = 0x04
private const val MASK_DOWN = 0x08
private const val MASK_A = 0x10
private const val MASK_B = 0x20
private const val MASK_SELECT = 0x40
private const val MASK_START = 0x80

private fun sanitizeRomName(raw: String): String {
    val base = raw.ifBlank { "rom.gb" }
    val cleaned = base.replace(Regex("[^A-Za-z0-9._-]"), "_")
    return if (cleaned.contains('.')) cleaned else "$cleaned.gb"
}

private fun queryDisplayName(context: android.content.Context, uri: Uri): String? {
    return try {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }
    } catch (_: Throwable) {
        null
    }
}

private enum class OptionsPage {
    Root,
    Emulation,
    BootRom,
    Input,
}

class MainActivity : ComponentActivity() {
    private val vm: EmulatorViewModel by viewModels()
    private lateinit var audioPlayer: AudioPlayer

    private val controllerPressedMaskState = mutableIntStateOf(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        InputMappingStore.init(this)
        audioPlayer = AudioPlayer(vm.emulator, lifecycleScope)
        enableEdgeToEdge()
        setContent {
            VibeEmuATheme {
                EmulatorScreen(
                    vm.emulator,
                    vm.emuDispatcher,
                    controllerPressedMask = controllerPressedMaskState.intValue,
                    onRomLoaded = {
                        audioPlayer.start()
                        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    }
                )
            }
        }
    }

    private fun setControllerMaskBit(mask: Int, pressed: Boolean) {
        val cur = controllerPressedMaskState.intValue
        val next = if (pressed) (cur or mask) else (cur and mask.inv())
        if (next != cur) controllerPressedMaskState.intValue = next
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (KeyCapture.isCapturing()) {
            if (KeyCapture.maybeConsumeKeyDown(event)) return true
        }

        val isController = isGameControllerDevice(event.device)
        val mappings = InputMappingStore.get()
        val mask = mappings.maskForKeyEvent(event, isController)
        if (mask != 0) {
            when (event.action) {
                KeyEvent.ACTION_DOWN -> {
                    setControllerMaskBit(mask, true)
                    return true
                }
                KeyEvent.ACTION_UP -> {
                    setControllerMaskBit(mask, false)
                    return true
                }
            }
        }
        return super.dispatchKeyEvent(event)
    }

    private fun getCenteredAxis(event: MotionEvent, device: InputDevice, axis: Int): Float {
        val range = device.getMotionRange(axis, event.source) ?: return 0f
        val value = event.getAxisValue(axis)
        return if (abs(value) > range.flat) value else 0f
    }

    override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
        if (event.action == MotionEvent.ACTION_MOVE && isGameControllerDevice(event.device)) {
            val device = event.device
            if (device != null && (event.source and InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK) {
                val hatX = event.getAxisValue(MotionEvent.AXIS_HAT_X)
                val hatY = event.getAxisValue(MotionEvent.AXIS_HAT_Y)

                val lx = getCenteredAxis(event, device, MotionEvent.AXIS_X)
                val ly = getCenteredAxis(event, device, MotionEvent.AXIS_Y)

                val threshold = 0.35f
                var dpadBits = 0

                // Prefer HAT if present; otherwise fall back to left stick.
                if (hatX <= -0.5f || lx <= -threshold) dpadBits = dpadBits or MASK_LEFT
                if (hatX >= 0.5f || lx >= threshold) dpadBits = dpadBits or MASK_RIGHT
                if (hatY <= -0.5f || ly <= -threshold) dpadBits = dpadBits or MASK_UP
                if (hatY >= 0.5f || ly >= threshold) dpadBits = dpadBits or MASK_DOWN

                val cur = controllerPressedMaskState.intValue
                val cleared = cur and (MASK_UP or MASK_DOWN or MASK_LEFT or MASK_RIGHT).inv()
                val next = cleared or dpadBits
                if (next != cur) controllerPressedMaskState.intValue = next
                return true
            }
        }
        return super.dispatchGenericMotionEvent(event)
    }

    override fun onResume() {
        super.onResume()
        vm.emulator.setPaused(false)
        if (vm.emulator.isReady()) {
            audioPlayer.start()
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }

    override fun onPause() {
        super.onPause()
        vm.emulator.setPaused(true)
        audioPlayer.stop()
        vm.emulator.saveRam()
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        vm.emulator.setPaused(!hasFocus)
        if (!hasFocus) {
            audioPlayer.stop()
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else if (vm.emulator.isReady()) {
            audioPlayer.start()
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        audioPlayer.stop()
    }

    override fun onStop() {
        super.onStop()
        vm.emulator.saveRam()
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
fun EmulatorScreen(
    emulator: Emulator,
    emuDispatcher: kotlinx.coroutines.CoroutineDispatcher,
    controllerPressedMask: Int = 0,
    onRomLoaded: () -> Unit = {},
    onOpenInstances: () -> Unit = {},
) {
    val context = LocalContext.current
    val configuration = LocalConfiguration.current
    val isLandscape = configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    val isTv = (configuration.uiMode and Configuration.UI_MODE_TYPE_MASK) == Configuration.UI_MODE_TYPE_TELEVISION
    val showTouchControls = !isTv

    val optionsRepository = remember(context) { OptionsRepository(context) }
    var options by remember { mutableStateOf(optionsRepository.load()) }
    var screen by rememberSaveable { mutableStateOf(UiScreen.Instances) }

    val bootRomDmgFile = remember(context) { File(context.filesDir, "bootrom_dmg.bin") }
    val bootRomCgbFile = remember(context) { File(context.filesDir, "bootrom_cgb.bin") }

    fun currentDmgBootRomBytes(): ByteArray? {
        if (!options.dmgBootRomEnabled) return null
        return try {
            if (bootRomDmgFile.exists()) bootRomDmgFile.readBytes() else null
        } catch (_: Throwable) {
            null
        }
    }

    fun currentCgbBootRomBytes(): ByteArray? {
        if (!options.cgbBootRomEnabled) return null
        return try {
            if (bootRomCgbFile.exists()) bootRomCgbFile.readBytes() else null
        } catch (_: Throwable) {
            null
        }
    }

    fun applySerialPeripheral(peripheral: SerialPeripheral) {
        if (!emulator.isReady()) return
        when (peripheral) {
            SerialPeripheral.None -> emulator.disableMobileAdapter()
            SerialPeripheral.MobileAdapterGb -> {
                val configPath = File(context.filesDir, "mobile_adapter.cfg").absolutePath
                val ok = emulator.enableMobileAdapter(configPath)
                if (!ok) {
                    // Fall back to None; UI will reflect this on next state update.
                    options = options.copy(serialPeripheral = SerialPeripheral.None)
                    optionsRepository.save(options)
                }
            }
        }
    }

    fun applyRuntimeOptions(newOptions: AppOptions) {
        emulator.setDmgNeutralPalette(newOptions.dmgNeutralPalette)
        applySerialPeripheral(newOptions.serialPeripheral)

        // Update boot ROM(s) for future resets/loads.
        val dmgBytes = currentDmgBootRomBytes()
        val cgbBytes = currentCgbBootRomBytes()
        if (dmgBytes != null) emulator.setBootRom(BootRomMode.Dmg, dmgBytes) else emulator.clearBootRom(BootRomMode.Dmg)
        if (cgbBytes != null) emulator.setBootRom(BootRomMode.Cgb, cgbBytes) else emulator.clearBootRom(BootRomMode.Cgb)
    }

    var status by rememberSaveable { mutableStateOf("Select an instance to play") }
    var romLabel by rememberSaveable { mutableStateOf("No instance loaded") }
    var inputState by remember { mutableStateOf(0xFF) }
    var menuExpanded by remember { mutableStateOf(false) }

    LaunchedEffect(screen) {
        // Pause emulation whenever we're not actively on the gameplay screen.
        // This prevents the core from running (and mutating SRAM) while the user is managing instances.
        emulator.setPaused(screen != UiScreen.Emulator)

        // Also flush SRAM/RTC to disk when leaving gameplay so exports see the latest data.
        if (screen != UiScreen.Emulator) {
            emulator.saveRam()
        }
    }

    var dpadPressedMask by remember { mutableStateOf(0) }
    var actionPressedMask by remember { mutableStateOf(0) }
    var metaPressedMask by remember { mutableStateOf(0) }

    val frameBuffer = remember { IntArray(FB_WIDTH * FB_HEIGHT) }
    val bitmap = remember { Bitmap.createBitmap(FB_WIDTH, FB_HEIGHT, Bitmap.Config.ARGB_8888) }
    val paint = remember {
        Paint().apply {
            // Nearest-neighbor scaling is both faster and more correct for pixel art.
            isFilterBitmap = false
            isDither = false
        }
    }
    val destRect = remember { Rect() }
    var surfaceHolder by remember { mutableStateOf<SurfaceHolder?>(null) }
    var hasFrame by remember { mutableStateOf(false) }
    // Core runs on a dedicated thread (provided by activity).

    fun loadInstance(instance: GameInstance) {
        val repo = GameInstancesRepository(context)
        val romFile = repo.romFile(instance.id)
        val dmgBootRom = currentDmgBootRomBytes()
        val cgbBootRom = currentCgbBootRomBytes()

        if (romFile.exists() && emulator.loadRomFromFile(romFile.absolutePath, options.emulationMode, dmgBootRom, cgbBootRom)) {
            romLabel = instance.nickname
            status = "Running ${instance.nickname} (${options.emulationMode.label})"
            inputState = 0xFF
            emulator.updateInput(inputState)
            applyRuntimeOptions(options)
            screen = UiScreen.Emulator
            onRomLoaded()
        } else {
            status = "Failed to load instance"
        }
    }

    LaunchedEffect(Unit) {
        // Drive emulation off the main thread to reduce UI contention.
        val targetFrameNs = 16_740_000L // ~59.7 fps
        withContext(emuDispatcher) {
            var nextFrameDeadline = System.nanoTime()
            val maxFrameSkip = 4
            while (isActive) {
                if (emulator.isReady() && !emulator.isPaused()) {
                    // Pace the loop to ~59fps to avoid running too fast.
                    val now = System.nanoTime()
                    val sleepNs = nextFrameDeadline - now
                    if (sleepNs > 0) {
                        sleepForNanos(sleepNs)
                        continue
                    }

                    // If we're behind, run a few frames without drawing to catch up.
                    var skipped = 0
                    var catchupDeadline = nextFrameDeadline
                    var catchupNow = now
                    while (skipped < maxFrameSkip && catchupNow - catchupDeadline > targetFrameNs) {
                        emulator.renderFrame(frameBuffer) // skip draw
                        catchupDeadline += targetFrameNs
                        skipped++
                        catchupNow = System.nanoTime()
                    }
                    nextFrameDeadline = catchupDeadline

                    val updated = emulator.renderFrame(frameBuffer)
                    if (updated) {
                        bitmap.setPixels(frameBuffer, 0, FB_WIDTH, 0, 0, FB_WIDTH, FB_HEIGHT)
                        surfaceHolder?.let { h ->
                            val canvas = try {
                                if (Build.VERSION.SDK_INT >= 26) h.lockHardwareCanvas() else h.lockCanvas()
                            } catch (_: Throwable) {
                                h.lockCanvas()
                            }
                            if (canvas != null) {
                                computeDestRect(canvas.width, canvas.height, destRect)
                                canvas.drawBitmap(bitmap, null, destRect, paint)
                                h.unlockCanvasAndPost(canvas)
                            }
                        }
                        if (!hasFrame) {
                            withContext(Dispatchers.Main.immediate) {
                                hasFrame = true
                            }
                        }
                    }
                    // Schedule next frame; if we ran long, resync to avoid runaway speed.
                    val after = System.nanoTime()
                    nextFrameDeadline += targetFrameNs
                    if (after - nextFrameDeadline > targetFrameNs) {
                        nextFrameDeadline = after + targetFrameNs
                    }
                } else {
                    // Back off slightly when no ROM is loaded.
                    sleepForNanos(30_000_000L)
                    nextFrameDeadline = System.nanoTime() + targetFrameNs
                }
            }
        }
    }

    LaunchedEffect(dpadPressedMask, actionPressedMask, metaPressedMask, controllerPressedMask) {
        val pressed = (dpadPressedMask or actionPressedMask or metaPressedMask) or controllerPressedMask
        val nextState = 0xFF and pressed.inv()
        inputState = nextState
        emulator.updateInput(nextState)
    }

    val showTopBar = (!isLandscape && !isTv) && screen == UiScreen.Emulator

    Scaffold(
        topBar = {
            if (showTopBar) {
                TopAppBar(
                    title = {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Image(
                                painter = androidx.compose.ui.res.painterResource(id = R.drawable.vibe_logo),
                                contentDescription = null,
                                modifier = Modifier.size(24.dp)
                            )
                            Spacer(modifier = Modifier.width(10.dp))
                            Text(
                                text = "vibeEmu (Android) v${BuildConfig.VERSION_NAME}",
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.Bold
                            )
                        }
                    },
                    actions = {
                        IconButton(onClick = { menuExpanded = true }) {
                            Icon(imageVector = Icons.Filled.Menu, contentDescription = "Menu")
                        }
                        DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
                            DropdownMenuItem(
                                text = { Text(text = "Instances") },
                                onClick = {
                                    menuExpanded = false
                                    emulator.saveRam()
                                    screen = UiScreen.Instances
                                    onOpenInstances()
                                }
                            )

                            DropdownMenuItem(
                                text = { Text(text = "Options") },
                                onClick = {
                                    menuExpanded = false
                                    emulator.saveRam()
                                    screen = UiScreen.Options
                                }
                            )

                            DropdownMenuItem(
                                text = { Text(text = "About") },
                                onClick = {
                                    menuExpanded = false
                                    emulator.saveRam()
                                    screen = UiScreen.About
                                }
                            )

                            DropdownMenuItem(
                                text = { Text(text = "Reset") },
                                onClick = {
                                    menuExpanded = false
                                    if (emulator.isReady()) {
                                        emulator.reset()
                                        status = "Reset $romLabel"
                                    }
                                }
                            )
                        }
                    }
                )
            }
        }
    ) { padding ->
        if (screen == UiScreen.Instances) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
            ) {
                InstancesScreen(
                    onPlayInstance = { inst ->
                        loadInstance(inst)
                    }
                )
            }
            return@Scaffold
        }

        if (screen == UiScreen.Options) {
            OptionsScreen(
                options = options,
                onOptionsChange = { updated ->
                    options = updated
                    optionsRepository.save(updated)
                    applyRuntimeOptions(updated)
                },
                onBack = { screen = UiScreen.Emulator },
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
            )
            return@Scaffold
        }

        if (screen == UiScreen.About) {
            AboutMenuScreen(
                onBack = { screen = UiScreen.Emulator },
                onOpenOpenSourceLicenses = {
                    screen = UiScreen.AndroidLicenses
                },
                onOpenThirdPartyLicensesHtml = {
                    screen = UiScreen.ThirdPartyLicenses
                },
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
            )
            return@Scaffold
        }

        if (screen == UiScreen.AndroidLicenses) {
            LicensesScreen(
                onBack = { screen = UiScreen.About },
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
            )
            return@Scaffold
        }

        if (screen == UiScreen.ThirdPartyLicenses) {
            ThirdPartyHtmlScreen(
                onBack = { screen = UiScreen.About },
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
            )
            return@Scaffold
        }

        BoxWithConstraints(
            modifier = Modifier
                .fillMaxSize()
                .background(MaterialTheme.colorScheme.background)
                .padding(padding)
                .navigationBarsPadding(),
        ) {
            val compactHeight = maxHeight < 640.dp
            val outerPadding = if (compactHeight) 8.dp else 16.dp
            val vGap = if (compactHeight) 8.dp else 12.dp

            val contentPadding = if (isLandscape) {
                PaddingValues(horizontal = outerPadding, vertical = 0.dp)
            } else {
                PaddingValues(all = outerPadding)
            }

            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(contentPadding)
            ) {
                if (!isTv && !isLandscape) {
                    Column(
                        modifier = Modifier.fillMaxSize(),
                        verticalArrangement = Arrangement.spacedBy(vGap)
                    ) {
                        Text(text = status, style = MaterialTheme.typography.bodyMedium)
                        PortraitPlayLayout(
                            modifier = Modifier.weight(1f, fill = true),
                            compactHeight = compactHeight,
                            showTouchControls = showTouchControls,
                            hasFrame = hasFrame,
                            surfaceHolder = surfaceHolder,
                            onSurfaceHolderChanged = { surfaceHolder = it },
                            dpadPressedMask = dpadPressedMask,
                            onDpadMaskChange = { dpadPressedMask = it },
                            actionPressedMask = actionPressedMask,
                            onActionMaskChange = { actionPressedMask = it },
                            metaPressedMask = metaPressedMask,
                            onMetaMaskChange = { metaPressedMask = it },
                        )
                        Text(text = "ROM: $romLabel", style = MaterialTheme.typography.bodyMedium)
                    }
                } else {
                    // Landscape (or TV): give gameplay the full vertical space.
                    LandscapePlayLayout(
                        modifier = Modifier.fillMaxSize(),
                        compactHeight = compactHeight,
                        showTouchControls = showTouchControls,
                        hasFrame = hasFrame,
                        surfaceHolder = surfaceHolder,
                        onSurfaceHolderChanged = { surfaceHolder = it },
                        dpadPressedMask = dpadPressedMask,
                        onDpadMaskChange = { dpadPressedMask = it },
                        actionPressedMask = actionPressedMask,
                        onActionMaskChange = { actionPressedMask = it },
                        metaPressedMask = metaPressedMask,
                        onMetaMaskChange = { metaPressedMask = it },
                    )

                    // Overlay hamburger menu for landscape phones so the game can reach top/bottom.
                    if (!isTv && isLandscape && screen == UiScreen.Emulator) {
                        Box(
                            modifier = Modifier
                                .fillMaxSize()
                                .statusBarsPadding(),
                        ) {
                            Box(modifier = Modifier.align(Alignment.TopEnd)) {
                                IconButton(onClick = { menuExpanded = true }) {
                                    Icon(imageVector = Icons.Filled.Menu, contentDescription = "Menu")
                                }
                                DropdownMenu(
                                    expanded = menuExpanded,
                                    onDismissRequest = { menuExpanded = false },
                                ) {
                                    DropdownMenuItem(
                                        text = { Text(text = "Instances") },
                                        onClick = {
                                            menuExpanded = false
                                            emulator.saveRam()
                                            screen = UiScreen.Instances
                                            onOpenInstances()
                                        }
                                    )
                                    DropdownMenuItem(
                                        text = { Text(text = "Options") },
                                        onClick = {
                                            menuExpanded = false
                                            emulator.saveRam()
                                            screen = UiScreen.Options
                                        }
                                    )

                                    DropdownMenuItem(
                                        text = { Text(text = "About") },
                                        onClick = {
                                            menuExpanded = false
                                            emulator.saveRam()
                                            screen = UiScreen.About
                                        }
                                    )
                                    DropdownMenuItem(
                                        text = { Text(text = "Reset") },
                                        onClick = {
                                            menuExpanded = false
                                            if (emulator.isReady()) {
                                                emulator.reset()
                                                status = "Reset $romLabel"
                                            }
                                        }
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun PortraitPlayLayout(
    modifier: Modifier,
    compactHeight: Boolean,
    showTouchControls: Boolean,
    hasFrame: Boolean,
    surfaceHolder: SurfaceHolder?,
    onSurfaceHolderChanged: (SurfaceHolder?) -> Unit,
    dpadPressedMask: Int,
    onDpadMaskChange: (Int) -> Unit,
    actionPressedMask: Int,
    onActionMaskChange: (Int) -> Unit,
    metaPressedMask: Int,
    onMetaMaskChange: (Int) -> Unit,
) {
    val vGap = if (compactHeight) 8.dp else 12.dp
    val padGap = if (compactHeight) 16.dp else 32.dp

    BoxWithConstraints(
        modifier = modifier.fillMaxWidth(),
    ) {
        val maxPadByHeight = (maxHeight.value * if (compactHeight) 0.22f else 0.25f).dp
        val padSize = minOf(320.dp, (maxWidth - padGap) / 2, maxPadByHeight)
        val keySize = (padSize.value * 0.28f).dp
        val metaHeight = if (compactHeight) 56.dp else 64.dp

        Column(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(vGap)
        ) {
            GameView(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f, fill = true),
                hasFrame = hasFrame,
                surfaceHolder = surfaceHolder,
                onSurfaceHolderChanged = onSurfaceHolderChanged,
            )

            if (showTouchControls) {
                Text(text = "Controls", style = MaterialTheme.typography.titleMedium)

                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 8.dp),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.Center,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        DpadPad(
                            padSize = padSize,
                            keySize = keySize,
                            pressedMask = dpadPressedMask,
                            onMaskChange = onDpadMaskChange,
                        )
                        Spacer(modifier = Modifier.width(padGap))
                        ActionPad(
                            padSize = padSize,
                            keySize = keySize,
                            pressedMask = actionPressedMask,
                            onMaskChange = onActionMaskChange,
                        )
                    }
                }

                StartSelectPad(
                    height = metaHeight,
                    pressedMask = metaPressedMask,
                    onMaskChange = onMetaMaskChange,
                )
            }
        }
    }
}

@Composable
private fun LandscapePlayLayout(
    modifier: Modifier,
    compactHeight: Boolean,
    showTouchControls: Boolean,
    hasFrame: Boolean,
    surfaceHolder: SurfaceHolder?,
    onSurfaceHolderChanged: (SurfaceHolder?) -> Unit,
    dpadPressedMask: Int,
    onDpadMaskChange: (Int) -> Unit,
    actionPressedMask: Int,
    onActionMaskChange: (Int) -> Unit,
    metaPressedMask: Int,
    onMetaMaskChange: (Int) -> Unit,
) {
    val hGap = if (compactHeight) 10.dp else 16.dp

    BoxWithConstraints(modifier = modifier.fillMaxSize()) {
        val aspect = FB_WIDTH.toFloat() / FB_HEIGHT.toFloat()
        val desiredGameWidth = maxHeight * aspect

        val minControlWidth = if (compactHeight) 132.dp else 156.dp
        val maxControlWidth = 320.dp

        val canFullHeightGame = !showTouchControls || (maxWidth >= desiredGameWidth + (minControlWidth * 2) + (hGap * 2))
        val computedGameColumnWidth = if (canFullHeightGame) {
            desiredGameWidth
        } else {
            // Not enough width to allow a full-height game while keeping controls usable.
            // Fall back to a balanced layout that still centers the screen.
            val fallbackControlWidth = minOf(maxControlWidth, maxWidth * 0.28f)
            maxOf(0.dp, maxWidth - (fallbackControlWidth * 2) - (hGap * 2))
        }

        val gameColumnWidth = if (computedGameColumnWidth > 0.dp) computedGameColumnWidth else maxWidth

        val controlColumnWidth = if (!showTouchControls) {
            0.dp
        } else {
            minOf(
                maxControlWidth,
                maxOf(0.dp, (maxWidth - gameColumnWidth - (hGap * 2)) / 2)
            )
        }

        val padSize = minOf(320.dp, controlColumnWidth, maxHeight * 0.72f)
        val keySize = (padSize.value * 0.28f).dp
        val metaHeight = minOf(if (compactHeight) 52.dp else 60.dp, maxHeight * 0.20f)

        Row(
            modifier = Modifier.fillMaxSize(),
            horizontalArrangement = Arrangement.spacedBy(hGap),
            verticalAlignment = Alignment.CenterVertically
        ) {
            if (showTouchControls) {
                Column(
                    modifier = Modifier.width(controlColumnWidth).fillMaxSize(),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    DpadPad(
                        padSize = padSize,
                        keySize = keySize,
                        pressedMask = dpadPressedMask,
                        onMaskChange = onDpadMaskChange,
                    )
                }
            }

            GameView(
                modifier = Modifier
                    .width(gameColumnWidth)
                    .fillMaxHeight(),
                hasFrame = hasFrame,
                surfaceHolder = surfaceHolder,
                onSurfaceHolderChanged = onSurfaceHolderChanged,
            )

            if (showTouchControls) {
                Column(
                    modifier = Modifier.width(controlColumnWidth).fillMaxSize(),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    ActionPad(
                        padSize = padSize,
                        keySize = keySize,
                        pressedMask = actionPressedMask,
                        onMaskChange = onActionMaskChange,
                    )
                    Spacer(modifier = Modifier.height(if (compactHeight) 10.dp else 16.dp))
                    StartSelectPad(
                        height = metaHeight,
                        pressedMask = metaPressedMask,
                        onMaskChange = onMetaMaskChange,
                    )
                }
            }
        }
    }
}

@Composable
private fun GameView(
    modifier: Modifier,
    hasFrame: Boolean,
    surfaceHolder: SurfaceHolder?,
    onSurfaceHolderChanged: (SurfaceHolder?) -> Unit,
) {
    Box(
        modifier = modifier,
        contentAlignment = Alignment.Center,
    ) {
        BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
            val aspect = FB_WIDTH.toFloat() / FB_HEIGHT.toFloat()
            val gameWidth = minOf(maxWidth, (maxHeight.value * aspect).dp)
            val gameHeight = (gameWidth.value / aspect).dp

            Surface(
                modifier = Modifier
                    .width(gameWidth)
                    .height(gameHeight),
                shape = RectangleShape,
                tonalElevation = 4.dp,
            ) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    AndroidView(
                        factory = { ctx ->
                            SurfaceView(ctx).apply {
                                holder.addCallback(object : SurfaceHolder.Callback {
                                    override fun surfaceCreated(h: SurfaceHolder) { onSurfaceHolderChanged(h) }
                                    override fun surfaceChanged(h: SurfaceHolder, format: Int, width: Int, height: Int) { onSurfaceHolderChanged(h) }
                                    override fun surfaceDestroyed(h: SurfaceHolder) { if (surfaceHolder === h) onSurfaceHolderChanged(null) }
                                })
                            }
                        },
                        modifier = Modifier.fillMaxSize()
                    )
                    if (!hasFrame) {
                        BasicText(text = "No frame yet", style = MaterialTheme.typography.bodyLarge)
                    }
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun OptionsScreen(
    options: AppOptions,
    onOptionsChange: (AppOptions) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current

    var page by remember { mutableStateOf(OptionsPage.Root) }

    val bootRomDmgFile = remember(context) { File(context.filesDir, "bootrom_dmg.bin") }
    val bootRomCgbFile = remember(context) { File(context.filesDir, "bootrom_cgb.bin") }

    var mappings by remember { mutableStateOf(InputMappingsRepository(context).load()) }

    fun saveMappings(next: InputMappings) {
        mappings = next
        InputMappingStore.set(context, next)
    }

    val pickDmgBootRom = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        val bytes = try {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (_: Throwable) {
            null
        }
        if (bytes != null) {
            try {
                bootRomDmgFile.writeBytes(bytes)
                onOptionsChange(options.copy(dmgBootRomEnabled = true))
            } catch (_: Throwable) {
            }
        }
    }

    val pickCgbBootRom = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        val bytes = try {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (_: Throwable) {
            null
        }
        if (bytes != null) {
            try {
                bootRomCgbFile.writeBytes(bytes)
                onOptionsChange(options.copy(cgbBootRomEnabled = true))
            } catch (_: Throwable) {
            }
        }
    }

    val title = when (page) {
        OptionsPage.Root -> "Options"
        OptionsPage.Emulation -> "Emulation"
        OptionsPage.BootRom -> "Boot ROM"
        OptionsPage.Input -> "Input"
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text(text = title) },
                navigationIcon = {
                    IconButton(
                        onClick = {
                            if (page == OptionsPage.Root) onBack() else page = OptionsPage.Root
                        }
                    ) {
                        Icon(imageVector = Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                }
            )
        }
    ) { innerPadding ->
        when (page) {
            OptionsPage.Root -> {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(innerPadding),
                ) {
                    ListItem(
                        headlineContent = { Text("Emulation") },
                        supportingContent = { Text("Hardware mode, palette") },
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { page = OptionsPage.Emulation }
                            .padding(horizontal = 8.dp),
                    )

                    HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))

                    ListItem(
                        headlineContent = { Text("Boot ROM") },
                        supportingContent = { Text("Select DMG/CGB boot ROM files") },
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { page = OptionsPage.BootRom }
                            .padding(horizontal = 8.dp),
                    )

                    HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))

                    ListItem(
                        headlineContent = { Text("Input") },
                        supportingContent = { Text("Remap keyboard/controller keys") },
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { page = OptionsPage.Input }
                            .padding(horizontal = 8.dp),
                    )
                }
            }

            OptionsPage.Emulation -> {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(innerPadding)
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(text = "Hardware mode", style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
                    EmulationMode.entries.forEach { mode ->
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(
                                selected = options.emulationMode == mode,
                                onClick = { onOptionsChange(options.copy(emulationMode = mode)) },
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text(text = mode.label)
                        }
                    }
                    Text(
                        text = "Hardware mode takes effect next time you load a ROM.",
                        style = MaterialTheme.typography.bodySmall,
                    )

                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Text(text = "DMG neutral palette", style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
                            Text(text = "Matches the upstream neutral DMG palette.", style = MaterialTheme.typography.bodySmall)
                        }
                        Switch(
                            checked = options.dmgNeutralPalette,
                            onCheckedChange = { onOptionsChange(options.copy(dmgNeutralPalette = it)) },
                        )
                    }

                    Spacer(modifier = Modifier.height(8.dp))
                    Text(text = "Serial", style = MaterialTheme.typography.titleMedium)
                    Text(text = "Peripheral", style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
                    SerialPeripheral.entries.forEach { p ->
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(
                                selected = options.serialPeripheral == p,
                                onClick = { onOptionsChange(options.copy(serialPeripheral = p)) },
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text(text = p.label)
                        }
                    }
                }
            }

            OptionsPage.BootRom -> {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(innerPadding)
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(text = "DMG boot ROM", style = MaterialTheme.typography.titleMedium)
                    Text(text = if (options.dmgBootRomEnabled && bootRomDmgFile.exists()) "Set" else "Not set", style = MaterialTheme.typography.bodySmall)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { pickDmgBootRom.launch("application/octet-stream") }) { Text("Choose") }
                        OutlinedButton(
                            onClick = {
                                try { bootRomDmgFile.delete() } catch (_: Throwable) {}
                                onOptionsChange(options.copy(dmgBootRomEnabled = false))
                            }
                        ) { Text("Clear") }
                    }
                    Text(text = "Used in DMG mode. Takes effect on next ROM load/reset.", style = MaterialTheme.typography.bodySmall)

                    Spacer(modifier = Modifier.height(12.dp))
                    Text(text = "CGB boot ROM", style = MaterialTheme.typography.titleMedium)
                    Text(text = if (options.cgbBootRomEnabled && bootRomCgbFile.exists()) "Set" else "Not set", style = MaterialTheme.typography.bodySmall)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { pickCgbBootRom.launch("application/octet-stream") }) { Text("Choose") }
                        OutlinedButton(
                            onClick = {
                                try { bootRomCgbFile.delete() } catch (_: Throwable) {}
                                onOptionsChange(options.copy(cgbBootRomEnabled = false))
                            }
                        ) { Text("Clear") }
                    }
                    Text(text = "Used in CGB mode. Takes effect on next ROM load/reset.", style = MaterialTheme.typography.bodySmall)
                }
            }

            OptionsPage.Input -> {
                LazyColumn(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(innerPadding)
                        .padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    item {
                        Text(text = "Key mapping", style = MaterialTheme.typography.titleMedium)
                    }
                    item {
                        Text(text = "Tap a binding, then press a key/button.", style = MaterialTheme.typography.bodySmall)
                    }

                    items(InputAction.entries) { a ->
                        val kb = mappings.keyboard[a] ?: KeyEvent.KEYCODE_UNKNOWN
                        val pad = mappings.controller[a] ?: KeyEvent.KEYCODE_UNKNOWN

                        Text(text = a.label, fontWeight = FontWeight.SemiBold)

                        OutlinedButton(
                            modifier = Modifier.fillMaxWidth(),
                            onClick = {
                                KeyCapture.request(a, forController = false) { code ->
                                    val next = mappings.copy(
                                        keyboard = mappings.keyboard.toMutableMap().apply { put(a, code) }
                                    )
                                    saveMappings(next)
                                }
                            }
                        ) {
                            Text(text = "Keyboard: ${KeyEvent.keyCodeToString(kb)}")
                        }

                        OutlinedButton(
                            modifier = Modifier.fillMaxWidth(),
                            onClick = {
                                KeyCapture.request(a, forController = true) { code ->
                                    val next = mappings.copy(
                                        controller = mappings.controller.toMutableMap().apply { put(a, code) }
                                    )
                                    saveMappings(next)
                                }
                            }
                        ) {
                            Text(text = "Controller: ${KeyEvent.keyCodeToString(pad)}")
                        }

                        HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
                    }
                }
            }
        }

        val p = KeyCapture.pending
        if (p != null) {
            AlertDialog(
                onDismissRequest = {
                    KeyCapture.cancel()
                },
                title = { Text("Press a ${if (p.forController) "controller button" else "keyboard key"}") },
                text = { Text("Binding for ${p.action.label}") },
                confirmButton = {
                    TextButton(
                        onClick = {
                            KeyCapture.cancel()
                        }
                    ) { Text("Cancel") }
                },
            )
        }
    }
}

@Composable
private fun DpadPad(
    padSize: Dp,
    keySize: Dp,
    pressedMask: Int,
    onMaskChange: (Int) -> Unit,
) {
    var sizePx by remember { mutableStateOf(IntSize.Zero) }
    val pointers = remember { mutableMapOf<Int, Offset>() }
    BoxWithConstraints(
        modifier = Modifier
            .size(padSize)
            .onSizeChanged { sizePx = it }
            .pointerInteropFilter { event ->
                fun recompute() {
                    val w = sizePx.width.toFloat()
                    val h = sizePx.height.toFloat()
                    if (w <= 0f || h <= 0f) return

                    val cx = w / 2f
                    val cy = h / 2f
                    val dead = minOf(w, h) * 0.14f

                    var next = 0
                    for (pos in pointers.values) {
                        // When another control is pressed, Android can deliver additional pointers
                        // to the first view that accepted the gesture. Ignore pointers outside.
                        if (pos.x < 0f || pos.x > w || pos.y < 0f || pos.y > h) continue
                        val dx = pos.x - cx
                        val dy = pos.y - cy
                        if (kotlin.math.abs(dx) > dead) {
                            next = next or if (dx > 0) MASK_RIGHT else MASK_LEFT
                        }
                        if (kotlin.math.abs(dy) > dead) {
                            next = next or if (dy > 0) MASK_DOWN else MASK_UP
                        }
                    }
                    if (next != pressedMask) onMaskChange(next)
                }

                when (event.actionMasked) {
                    MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                        val i = event.actionIndex
                        pointers[event.getPointerId(i)] = Offset(event.getX(i), event.getY(i))
                        recompute()
                        true
                    }
                    MotionEvent.ACTION_MOVE -> {
                        for (i in 0 until event.pointerCount) {
                            pointers[event.getPointerId(i)] = Offset(event.getX(i), event.getY(i))
                        }
                        recompute()
                        true
                    }
                    MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> {
                        val i = event.actionIndex
                        pointers.remove(event.getPointerId(i))
                        if (event.actionMasked == MotionEvent.ACTION_CANCEL) {
                            pointers.clear()
                        }
                        recompute()
                        true
                    }
                    else -> false
                }
            },
        contentAlignment = Alignment.Center
    ) {
        val armThickness = keySize
        val armLength = (keySize.value * 1.25f).dp
        val corner = RoundedCornerShape((armThickness.value * 0.22f).dp)

        Box(contentAlignment = Alignment.Center) {
            // Up arm
            PressableKey(
                label = "↑",
                pressed = (pressedMask and MASK_UP) != 0,
                width = armThickness,
                height = armLength,
                shape = corner,
                modifier = Modifier.offset(y = -((armThickness.value + armLength.value) / 2f).dp)
            )
            // Down arm
            PressableKey(
                label = "↓",
                pressed = (pressedMask and MASK_DOWN) != 0,
                width = armThickness,
                height = armLength,
                shape = corner,
                modifier = Modifier.offset(y = ((armThickness.value + armLength.value) / 2f).dp)
            )
            // Left arm
            PressableKey(
                label = "←",
                pressed = (pressedMask and MASK_LEFT) != 0,
                width = armLength,
                height = armThickness,
                shape = corner,
                modifier = Modifier.offset(x = -((armThickness.value + armLength.value) / 2f).dp)
            )
            // Right arm
            PressableKey(
                label = "→",
                pressed = (pressedMask and MASK_RIGHT) != 0,
                width = armLength,
                height = armThickness,
                shape = corner,
                modifier = Modifier.offset(x = ((armThickness.value + armLength.value) / 2f).dp)
            )
            // Center pivot
            Surface(
                modifier = Modifier.size((armThickness.value * 0.72f).dp),
                color = MaterialTheme.colorScheme.surfaceVariant,
                shape = CircleShape,
                tonalElevation = 0.dp,
                shadowElevation = 0.dp,
            ) {}
        }
    }
}

@Composable
private fun ActionPad(
    padSize: Dp,
    keySize: Dp,
    pressedMask: Int,
    onMaskChange: (Int) -> Unit,
) {
    var sizePx by remember { mutableStateOf(IntSize.Zero) }
    val pointers = remember { mutableMapOf<Int, Offset>() }
    BoxWithConstraints(
        modifier = Modifier
            .size(padSize)
            .onSizeChanged { sizePx = it }
            .pointerInteropFilter { event ->
                fun recompute() {
                    val w = sizePx.width.toFloat()
                    val h = sizePx.height.toFloat()
                    if (w <= 0f || h <= 0f) return

                    val bCenter = Offset(w * 0.38f, h * 0.58f)
                    val aCenter = Offset(w * 0.68f, h * 0.44f)
                    val radius = minOf(w, h) * 0.24f

                    var next = 0
                    for (pos in pointers.values) {
                        if (pos.x < 0f || pos.x > w || pos.y < 0f || pos.y > h) continue
                        val distB = (pos - bCenter).getDistance()
                        val distA = (pos - aCenter).getDistance()
                        if (distB <= radius) next = next or MASK_B
                        if (distA <= radius) next = next or MASK_A
                    }
                    if (next != pressedMask) onMaskChange(next)
                }

                when (event.actionMasked) {
                    MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                        val i = event.actionIndex
                        pointers[event.getPointerId(i)] = Offset(event.getX(i), event.getY(i))
                        recompute()
                        true
                    }
                    MotionEvent.ACTION_MOVE -> {
                        for (i in 0 until event.pointerCount) {
                            pointers[event.getPointerId(i)] = Offset(event.getX(i), event.getY(i))
                        }
                        recompute()
                        true
                    }
                    MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> {
                        val i = event.actionIndex
                        pointers.remove(event.getPointerId(i))
                        if (event.actionMasked == MotionEvent.ACTION_CANCEL) {
                            pointers.clear()
                        }
                        recompute()
                        true
                    }
                    else -> false
                }
            },
        contentAlignment = Alignment.Center
    ) {
        val key = keySize
        // Visual only: touch handling is on the parent box.
        Row(verticalAlignment = Alignment.CenterVertically) {
            PressableKey(
                label = "B",
                pressed = (pressedMask and MASK_B) != 0,
                size = key,
                modifier = Modifier.offset(y = 10.dp)
            )
            Spacer(modifier = Modifier.width(18.dp))
            PressableKey(
                label = "A",
                pressed = (pressedMask and MASK_A) != 0,
                size = key,
                modifier = Modifier.offset(y = (-10).dp)
            )
        }
    }
}

@Composable
private fun StartSelectPad(
    height: Dp,
    pressedMask: Int,
    onMaskChange: (Int) -> Unit,
) {
    var sizePx by remember { mutableStateOf(IntSize.Zero) }
    val pointers = remember { mutableMapOf<Int, Offset>() }
    BoxWithConstraints(
        modifier = Modifier
            .fillMaxWidth()
            .height(height)
            .onSizeChanged { sizePx = it }
            .pointerInteropFilter { event ->
                fun recompute() {
                    val w = sizePx.width.toFloat()
                    val h = sizePx.height.toFloat()
                    if (w <= 0f || h <= 0f) return

                    val pillW = w * 0.40f
                    val pillH = h * 0.70f
                    val y0 = (h - pillH) / 2f
                    val leftX0 = (w * 0.06f)
                    val rightX0 = w - leftX0 - pillW

                    fun hit(x0: Float, y0: Float, w: Float, h: Float, p: Offset): Boolean {
                        return p.x >= x0 && p.x <= x0 + w && p.y >= y0 && p.y <= y0 + h
                    }

                    var next = 0
                    for (pos in pointers.values) {
                        if (pos.x < 0f || pos.x > w || pos.y < 0f || pos.y > h) continue
                        if (hit(leftX0, y0, pillW, pillH, pos)) next = next or MASK_SELECT
                        if (hit(rightX0, y0, pillW, pillH, pos)) next = next or MASK_START
                    }
                    if (next != pressedMask) onMaskChange(next)
                }

                when (event.actionMasked) {
                    MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                        val i = event.actionIndex
                        pointers[event.getPointerId(i)] = Offset(event.getX(i), event.getY(i))
                        recompute()
                        true
                    }
                    MotionEvent.ACTION_MOVE -> {
                        for (i in 0 until event.pointerCount) {
                            pointers[event.getPointerId(i)] = Offset(event.getX(i), event.getY(i))
                        }
                        recompute()
                        true
                    }
                    MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP, MotionEvent.ACTION_CANCEL -> {
                        val i = event.actionIndex
                        pointers.remove(event.getPointerId(i))
                        if (event.actionMasked == MotionEvent.ACTION_CANCEL) {
                            pointers.clear()
                        }
                        recompute()
                        true
                    }
                    else -> false
                }
            },
        contentAlignment = Alignment.Center
    ) {
        val pillW = minOf(132.dp, (maxWidth - 16.dp) / 2)
        val pillH = minOf(44.dp, (height.value * 0.70f).dp)
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PressableKey(
                label = "Select",
                pressed = (pressedMask and MASK_SELECT) != 0,
                width = pillW,
                height = pillH,
                shape = RoundedCornerShape(22.dp)
            )
            PressableKey(
                label = "Start",
                pressed = (pressedMask and MASK_START) != 0,
                width = pillW,
                height = pillH,
                shape = RoundedCornerShape(22.dp)
            )
        }
    }
}

@Composable
private fun PressableKey(
    label: String,
    pressed: Boolean,
    modifier: Modifier = Modifier,
    size: androidx.compose.ui.unit.Dp = 72.dp,
    width: androidx.compose.ui.unit.Dp = size,
    height: androidx.compose.ui.unit.Dp = size,
    shape: Shape = CircleShape,
) {
    val shadow = if (pressed) 0.dp else 10.dp
    val tonal = if (pressed) 0.dp else 2.dp

    Surface(
        modifier = Modifier
            .then(modifier)
            .width(width)
            .height(height)
            .scale(if (pressed) 0.97f else 1f),
        color = MaterialTheme.colorScheme.primaryContainer,
        shape = shape,
        tonalElevation = tonal,
        shadowElevation = shadow,
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                text = label,
                fontSize = 16.sp,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onPrimaryContainer
            )
        }
    }
}