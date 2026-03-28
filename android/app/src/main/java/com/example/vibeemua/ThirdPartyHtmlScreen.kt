package com.example.vibeemua

import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThirdPartyHtmlScreen(
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val assetUrl = "file:///android_asset/licenses/vibeEmu_THIRD_PARTY_LICENSES.html"

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(text = "Third-party licenses") },
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
        AndroidView(
            modifier = modifier
                .fillMaxSize()
                .padding(padding),
            factory = { context ->
                WebView(context).apply {
                    webViewClient = WebViewClient()
                    settings.allowFileAccess = true
                    settings.allowContentAccess = true
                    settings.javaScriptEnabled = false
                    loadUrl(assetUrl)
                }
            },
            update = { webView ->
                if (webView.url != assetUrl) webView.loadUrl(assetUrl)
            },
        )
    }
}
