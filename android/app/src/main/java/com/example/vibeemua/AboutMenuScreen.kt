package com.example.vibeemua

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AboutMenuScreen(
    onBack: () -> Unit,
    onOpenOpenSourceLicenses: () -> Unit,
    onOpenThirdPartyLicensesHtml: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(text = "About") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = "Back",
                        )
                    }
                },
            )
        },
    ) { padding ->
        androidx.compose.foundation.layout.Column(
            modifier = modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            ListItem(
                headlineContent = { Text(text = "Open source licenses") },
                supportingContent = { Text(text = "Android dependencies (AboutLibraries)") },
                modifier = Modifier.clickable(onClick = onOpenOpenSourceLicenses),
            )
            ListItem(
                headlineContent = { Text(text = "Third-party licenses") },
                supportingContent = { Text(text = "Rust dependencies (cargo-about)") },
                modifier = Modifier.clickable(onClick = onOpenThirdPartyLicensesHtml),
            )
        }
    }
}
