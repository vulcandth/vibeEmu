package com.example.vibeemua

import android.content.res.Configuration
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.MaterialTheme
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** One pause menu for touch, keyboard and TV controller navigation. */
@Composable
@OptIn(ExperimentalMaterial3Api::class)
fun GameplayMenu(
    gameName: String,
    onResume: () -> Unit,
    onSettings: () -> Unit,
    onReset: () -> Unit,
    onInstances: () -> Unit,
    onAbout: () -> Unit,
) {
    val configuration = LocalConfiguration.current
    val isTv = configuration.uiMode and Configuration.UI_MODE_TYPE_MASK == Configuration.UI_MODE_TYPE_TELEVISION
    val content: @Composable () -> Unit = {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(16.dp).navigationBarsPadding(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(gameName, style = MaterialTheme.typography.titleLarge)
            Button(onClick = onResume, modifier = Modifier.fillMaxWidth()) { Text("Resume") }
            Button(onClick = onSettings, modifier = Modifier.fillMaxWidth()) { Text("Settings") }
            Button(onClick = onReset, modifier = Modifier.fillMaxWidth()) { Text("Reset") }
            Button(onClick = onInstances, modifier = Modifier.fillMaxWidth()) { Text("Return to instances") }
            TextButton(onClick = onAbout, modifier = Modifier.fillMaxWidth()) { Text("About and licenses") }
        }
    }
    if (!isTv && configuration.screenWidthDp < 600) {
        ModalBottomSheet(onDismissRequest = onResume) { content() }
        return
    }
    AlertDialog(
        onDismissRequest = onResume,
        text = content,
        confirmButton = {},
    )
}

/** Opposing directions cancel; action buttons are preserved. */
fun neutralizeDirections(pressed: Int): Int {
    var result = pressed
    if (result and 3 == 3) result = result and 3.inv()
    if (result and 12 == 12) result = result and 12.inv()
    return result
}
