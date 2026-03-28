package com.example.vibeemua.ui.theme

import android.app.Activity
import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

private val DarkColorScheme = darkColorScheme(
    primary = VibeCyan,
    onPrimary = Color.Black,
    primaryContainer = VibeCyanDark,
    onPrimaryContainer = Color.Black,
    secondary = VibePink,
    onSecondary = Color.Black,
    secondaryContainer = VibePinkDark,
    onSecondaryContainer = Color.Black,
    tertiary = VibeCyan,
    onTertiary = Color.Black,
    background = VibeBgDark,
    onBackground = Color(0xFFFFE6EE),
    surface = VibeSurfaceDark,
    onSurface = Color(0xFFFFE6EE),
    surfaceVariant = Color(0xFF2A141C),
    onSurfaceVariant = Color(0xFFF3C3D2),
    outline = Color(0xFF6D4A57),
)

private val LightColorScheme = lightColorScheme(
    primary = VibeCyanDark,
    onPrimary = Color.White,
    primaryContainer = VibeCyan,
    onPrimaryContainer = Color(0xFF001F26),
    secondary = VibePinkDark,
    onSecondary = Color.White,
    secondaryContainer = VibePink,
    onSecondaryContainer = Color(0xFF3F0018),
    tertiary = VibePinkDark,
    onTertiary = Color.White,
    background = VibeBgLight,
    onBackground = Color(0xFF201A1C),
    surface = VibeSurfaceLight,
    onSurface = Color(0xFF201A1C),
    surfaceVariant = Color(0xFFFFE1E9),
    onSurfaceVariant = Color(0xFF514349),
    outline = Color(0xFF84737A),

    /* Other default colors to override
    background = Color(0xFFFFFBFE),
    surface = Color(0xFFFFFBFE),
    onPrimary = Color.White,
    onSecondary = Color.White,
    onTertiary = Color.White,
    onBackground = Color(0xFF1C1B1F),
    onSurface = Color(0xFF1C1B1F),
    */
)

@Composable
fun VibeEmuATheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    // Dynamic color is available on Android 12+
    dynamicColor: Boolean = false,
    content: @Composable () -> Unit
) {
    val colorScheme = when {
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }

        darkTheme -> DarkColorScheme
        else -> LightColorScheme
    }

    MaterialTheme(
        colorScheme = colorScheme,
        typography = Typography,
        content = content
    )
}