package com.example.vibeemua

import org.junit.Assert.*
import org.junit.Test

class SaveStatePolicyTest {
    private fun slot(id: Int, date: Long, available: Boolean = true) =
        StateSlot(id, "test", "", available, true, date)

    @Test fun newestCompatibleManualSaveIsSuggestedAndRecoveryIsNeverAutomatic() {
        assertEquals(2, suggestedState(listOf(slot(11, 10), slot(2, 20), slot(12, 30)))?.id)
        assertEquals(11, suggestedState(listOf(slot(11, 10), slot(2, 20, false)))?.id)
        assertNull(suggestedState(listOf(slot(12, 30), slot(2, 20, false))))
        assertNull(suggestedState(emptyList()))
    }

    @Test fun sameSecondTiesPreferQuickThenLowestNumberRegardlessOfInputOrder() {
        assertEquals(11, suggestedState(listOf(slot(5, 10), slot(1, 10), slot(11, 10)))?.id)
        assertEquals(1, suggestedState(listOf(slot(5, 10), slot(1, 10)))?.id)
    }
}
