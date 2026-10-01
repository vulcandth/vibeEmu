package com.example.vibeemua

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class LandscapeControlsTest {
    @get:Rule val compose = createComposeRule()

    @Test fun controlsFlankDmgAndSgbAndAcceptInput() {
        var frame by mutableStateOf(FrameSize(160, 144))
        var directions by mutableStateOf(0)
        var actions by mutableStateOf(0)
        var meta by mutableStateOf(0)
        compose.setContent {
            MaterialTheme {
                LandscapePlayLayout(Modifier, true, true, frame, false, null, {},
                    directions, { directions = it }, actions, { actions = it }, meta, { meta = it })
            }
        }
        for (size in listOf(FrameSize(160, 144), FrameSize(256, 224))) {
            compose.runOnIdle { frame = size }
            val left = compose.onNodeWithTag("touch-dpad").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
            val game = compose.onNodeWithTag("landscape-game").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
            val right = compose.onNodeWithTag("touch-actions").assertIsDisplayed().fetchSemanticsNode().boundsInRoot
            val layout = compose.onNodeWithTag("landscape-layout").fetchSemanticsNode().boundsInRoot
            compose.onNodeWithTag("touch-start-select").assertIsDisplayed()
            assertTrue("D-pad must be to the left of the game", left.right <= game.left)
            assertTrue("A/B must be to the right of the game", game.right <= right.left)
            assertEquals("The game stays centered", layout.center.x, game.center.x, 1f)
            compose.onNodeWithTag("touch-dpad").performTouchInput { down(Offset(width * .85f, height * .5f)) }
            compose.runOnIdle { assertEquals(1, directions) }
            compose.onNodeWithTag("touch-dpad").performTouchInput { up() }
            compose.onNodeWithTag("touch-actions").performTouchInput { down(Offset(width * .68f, height * .44f)) }
            compose.runOnIdle { assertTrue(actions and 0x10 != 0) }
            compose.onNodeWithTag("touch-actions").performTouchInput { up() }
            compose.onNodeWithText("A").performTouchInput { down(center) }
            compose.runOnIdle { assertTrue(actions and 0x10 != 0) }
            compose.onNodeWithText("A").performTouchInput { up() }
            compose.runOnIdle { assertEquals(0, directions); assertEquals(0, actions) }
        }
    }
}
