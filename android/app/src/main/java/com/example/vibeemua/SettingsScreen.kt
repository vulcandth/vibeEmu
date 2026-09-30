package com.example.vibeemua
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.selection.toggleable
import androidx.compose.ui.semantics.Role
import android.view.KeyEvent
import java.io.File
import androidx.activity.compose.BackHandler
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.RadioButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.AlertDialog
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.platform.LocalContext
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import android.net.Uri
import androidx.compose.material3.Slider
import androidx.compose.material3.OutlinedTextField

private enum class OptionsPage {
    Root,
    General,
    Video,
    Audio,
    Peripherals,
    Emulation,
    BootRom,
    Input,
}
@Composable
@OptIn(ExperimentalMaterial3Api::class)
fun OptionsScreen(
    options: AppOptions,
    onOptionsChange: (AppOptions) -> Unit,
    onBack: () -> Unit,
    controllerNames: List<String>,
    canReload: Boolean,
    onReload: () -> Unit,
    modifier: Modifier = Modifier,
    pendingMachineChanges: Boolean = false,
    onDiscardMachineChanges: () -> Unit = {},
    bootRomFiles: Map<BootRomMode, File> = emptyMap(),
    onImportBootRom: (BootRomMode, ByteArray?) -> Unit = { _, _ -> },
) {
    val context = LocalContext.current

    var page by rememberSaveable { mutableStateOf(OptionsPage.Root) }
    var search by rememberSaveable { mutableStateOf("") }
    val categories = listOf(
        OptionsPage.General to "General", OptionsPage.Emulation to "System & Boot",
        OptionsPage.Video to "Video & Colors", OptionsPage.Audio to "Audio",
        OptionsPage.Input to "Controls", OptionsPage.Peripherals to "Peripherals",
    )
    val keywords = mapOf(
        OptionsPage.General to "speed pause background battery save",
        OptionsPage.Emulation to "model hardware dmg cgb gbc sgb agb boot rom reload",
        OptionsPage.Video to "palette color border sgb display",
        OptionsPage.Audio to "sound volume mono stereo mute",
        OptionsPage.Input to "keyboard controller buttons mapping touch player",
        OptionsPage.Peripherals to "serial mobile adapter link",
    )
    val matches = categories.filter { (destination, label) ->
        search.trim().split(Regex("\\s+")).all { term -> "$label ${keywords[destination]}".contains(term, ignoreCase = true) }
    }

    fun back() {
        if (search.isNotEmpty()) { search = ""; page = OptionsPage.Root }
        else if (page == OptionsPage.Root) onBack() else page = OptionsPage.Root
    }

    var pendingBootMode by rememberSaveable { mutableStateOf(BootRomMode.Dmg) }
    var bootError by remember { mutableStateOf<String?>(null) }

    var mappings by remember { mutableStateOf(InputMappingsRepository(context).load()) }

    fun saveMappings(next: InputMappings) {
        mappings = next
        InputMappingStore.set(context, next)
    }

    val pickBootRom = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri != null) {
            try {
                val mode = pendingBootMode
                val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                    ?: error("Cannot read the selected file")
                require(mode.acceptsSize(bytes.size)) {
                    "${mode.label} boot ROM must be ${if (mode.color) "2048 or 2304" else "256"} bytes"
                }
                onImportBootRom(mode, bytes)
                bootError = null
            } catch (e: Exception) { bootError = e.message ?: "Unable to import boot ROM" }
        }
    }

    val title = when (page) {
        OptionsPage.Root -> "Settings"
        OptionsPage.General -> "General"
        OptionsPage.Video -> "Video & Colors"
        OptionsPage.Audio -> "Audio"
        OptionsPage.Peripherals -> "Peripherals"
        OptionsPage.Emulation -> "System & Boot"
        OptionsPage.BootRom -> "Boot ROM"
        OptionsPage.Input -> "Controls"
    }

    BackHandler { back() }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text(text = title) },
                navigationIcon = {
                    IconButton(
                        onClick = {
                            back()
                        }
                    ) {
                        Icon(imageVector = Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                }
            )
        }
    ) { innerPadding ->
        Column(Modifier.fillMaxSize().padding(innerPadding)) {
            OutlinedTextField(value = search, onValueChange = { search = it; page = OptionsPage.Root },
                label = { Text("Search settings") }, singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp))
            if (pendingMachineChanges) {
                Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
                    Text("Pending model or boot ROM changes", style = MaterialTheme.typography.labelLarge)
                    if (!canReload) Text("These changes apply when you next open a game.", style = MaterialTheme.typography.bodySmall)
                    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (canReload) TextButton(onClick = onReload) { Text("Apply and reload") }
                        TextButton(onClick = onDiscardMachineChanges) { Text("Discard pending changes") }
                    }
                }
            }
        BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
            val wide = maxWidth >= 840.dp
            Row(Modifier.fillMaxSize()) {
                if (wide) {
                    Column(Modifier.width(220.dp).verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        matches.forEach { (destination, label) ->
                            OutlinedButton(onClick = { page = destination }, modifier = Modifier.fillMaxWidth()) { Text(label) }
                        }
                    }
                }
                Box(Modifier.weight(1f)) {
                    // The scaffold insets are already applied to the containing row.
                    val innerPadding = PaddingValues(0.dp)
                    when (page) {
                        OptionsPage.Root -> {
                            Column(Modifier.fillMaxSize().padding(innerPadding).verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                if (matches.isEmpty()) Text("No settings match your search.")
                                else if (wide) Text("Select a settings category.")
                                else for ((destination, label) in matches) {
                                    OutlinedButton(onClick = { page = destination }, modifier = Modifier.fillMaxWidth()) { Text(label) }
                                }
                            }
                        }
                        OptionsPage.General -> {
                            Column(Modifier.fillMaxSize().padding(innerPadding).verticalScroll(rememberScrollState()).padding(16.dp)) {
                                Text("Gameplay speed")
                                for (speed in listOf(50, 100, 150, 200, 400)) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        RadioButton(selected = options.speedPercent == speed, onClick = { onOptionsChange(options.copy(speedPercent = speed)) })
                                        Text("$speed%")
                                    }
                                }
                                Text("The game pauses when you open the menu or leave the app. Battery saves are written periodically and when leaving gameplay.")
                            }
                        }
                        OptionsPage.Audio -> {
                            Column(Modifier.fillMaxSize().padding(innerPadding).verticalScroll(rememberScrollState()).padding(16.dp)) {
                                SettingSwitch("Enable sound", options.soundEnabled) { onOptionsChange(options.copy(soundEnabled = it)) }
                                Text("Volume: ${options.volume}%")
                                Slider(value = options.volume.toFloat(), onValueChange = { onOptionsChange(options.copy(volume = it.toInt())) }, valueRange = 0f..100f)
                                SettingSwitch("Mono output", options.mono) { onOptionsChange(options.copy(mono = it)) }
                                Text("Audio follows system routing. Altered-speed audio is muted.")
                            }
                        }
                        OptionsPage.Video -> {
                            Column(Modifier.fillMaxSize().padding(innerPadding).verticalScroll(rememberScrollState()).padding(16.dp)) {
                                SettingSwitch("Show SGB border", options.showSgbBorder) { onOptionsChange(options.copy(showSgbBorder = it)) }
                                SettingSwitch("Neutral monochrome palette", options.dmgNeutralPalette) { onOptionsChange(options.copy(dmgNeutralPalette = it)) }
                            }
                        }
                        OptionsPage.Peripherals -> {
                            Column(Modifier.fillMaxSize().padding(innerPadding).verticalScroll(rememberScrollState()).padding(16.dp)) {
                                Text("Serial peripheral")
                                SerialPeripheral.entries.forEach { peripheral ->
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        RadioButton(options.serialPeripheral == peripheral, { onOptionsChange(options.copy(serialPeripheral = peripheral)) })
                                        Text(peripheral.label)
                                    }
                                }
                            }
                        }
                        OptionsPage.Emulation -> {
                            Column(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(innerPadding)
                                    .verticalScroll(rememberScrollState())
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
                                    text = "Model and boot ROM changes take effect on the next load. Reset retains the current machine and initial border.",
                                    style = MaterialTheme.typography.bodySmall,
                                )

                                Text("SGB + GBC accepts live SGB commands. Initial border starts normal CGB gameplay after capture; it has no SGB multiplayer or later border changes.", style = MaterialTheme.typography.bodySmall)
                                OutlinedButton(onClick = { page = OptionsPage.BootRom }, modifier = Modifier.fillMaxWidth()) { Text("Manage boot ROMs") }
                            }
                        }

                        OptionsPage.BootRom -> {
                            Column(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(innerPadding)
                                    .verticalScroll(rememberScrollState())
                                    .padding(16.dp),
                                verticalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                Text("Initial-border mode uses the SGB boot ROM for capture and the CGB boot ROM for gameplay.")
                                Text("Boot ROMs are optional. Changes take effect on the next game load.", style = MaterialTheme.typography.bodySmall)
                                bootError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                                BootRomMode.entries.forEach { mode ->
                                    val file = bootRomFiles[mode]
                                    Text("${mode.label} boot ROM", style = MaterialTheme.typography.titleMedium)
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Text(if (file != null) "Imported" else "Not set", modifier = Modifier.weight(1f))
                                        Switch(enabled = file != null, checked = mode in options.enabledBootRoms,
                                            onCheckedChange = { enabled -> onOptionsChange(options.copy(enabledBootRoms =
                                                if (enabled) options.enabledBootRoms + mode else options.enabledBootRoms - mode)) })
                                    }
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        OutlinedButton(onClick = { pendingBootMode = mode; pickBootRom.launch("*/*") }) { Text("Choose") }
                                        OutlinedButton(onClick = {
                                            try { onImportBootRom(mode, null); bootError = null }
                                            catch (error: Exception) { bootError = error.message ?: "Could not clear ${mode.label} boot ROM" }
                                        }) { Text("Clear") }
                                    }
                                    HorizontalDivider()
                                }
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
                                    Text("Keyboard and touch control player 1. SGB games can use up to four controllers. Other modes combine controllers into player 1.")
                                    controllerNames.forEachIndexed { player, name -> Text("Player ${player + 1}: $name") }
                                }

                                item {
                                    SettingSwitch("Hide touch controls with a controller", options.hideTouchWithController) { onOptionsChange(options.copy(hideTouchWithController = it)) }
                                }
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
private fun SettingSwitch(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().toggleable(value = checked, role = Role.Switch, onValueChange = onChange).padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = null)
    }
}
