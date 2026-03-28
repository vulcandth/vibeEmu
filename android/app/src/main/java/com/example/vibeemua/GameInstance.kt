package com.example.vibeemua

import java.util.UUID

data class GameInstance(
    val id: String = UUID.randomUUID().toString(),
    val nickname: String,
    val romDisplayName: String,
    val createdAtMillis: Long = System.currentTimeMillis(),
    val lastSavExportMillis: Long? = null,
)
