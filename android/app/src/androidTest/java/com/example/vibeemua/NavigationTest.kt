package com.example.vibeemua

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Runs unchanged on phone, tablet and TV emulator window configurations. */
class NavigationTest {
    @get:Rule val compose = createComposeRule()

    @Test fun categoriesEditPreferencesAndBackReturnsToParent() {
        var options by mutableStateOf(AppOptions())
        var exits = 0
        compose.setContent {
            MaterialTheme {
                OptionsScreen(options, { options = it }, { exits++ }, emptyList(), false, {})
            }
        }
        compose.onNode(hasText("Audio") and hasClickAction()).performClick()
        compose.onNodeWithText("Mono output").assertIsDisplayed().performClick().assertIsOn()
        compose.runOnIdle { assertTrue(options.mono) }
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNode(hasText("System & Boot") and hasClickAction()).assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.runOnIdle { assertEquals(1, exits) }
    }

    @Test fun gameplayMenuRoutesToSettingsAndResume() {
        var resumed = 0
        var settings = 0
        compose.setContent {
            MaterialTheme {
                GameplayMenu("Test game", { resumed++ }, { settings++ }, {}, {}, {})
            }
        }
        compose.onNodeWithText("Settings").assertIsDisplayed().performClick()
        compose.onNodeWithText("Resume").assertIsDisplayed().performClick()
        compose.runOnIdle {
            assertEquals(1, settings)
            assertEquals(1, resumed)
        }
    }

    @Test fun searchFindsSettingsByOptionAndPendingChangesCanBeDiscarded() {
        var discarded = 0
        var reloaded = 0
        compose.setContent {
            MaterialTheme {
                OptionsScreen(AppOptions(), {}, {}, emptyList(), true, { reloaded++ },
                    pendingMachineChanges = true, onDiscardMachineChanges = { discarded++ })
            }
        }
        compose.onNodeWithText("Search settings").performTextInput("volume")
        compose.onNode(hasText("Audio") and hasClickAction()).assertIsDisplayed().performClick()
        compose.onNodeWithText("Volume: 100%").assertIsDisplayed()
        compose.onNodeWithText("Apply and reload").performClick()
        compose.onNodeWithText("Discard pending changes").performClick()
        compose.runOnIdle { assertEquals(1, reloaded); assertEquals(1, discarded) }
    }
}
