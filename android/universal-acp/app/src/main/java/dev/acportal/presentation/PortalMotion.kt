package dev.acportal.presentation

import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.tween

/** Brief overlapping fades avoid moving/re-laying out an entire conversation during navigation.
 * Compose still applies the system animator duration scale, including disabled animations. */
internal fun portalPageTransition() = fadeIn(tween(150)) togetherWith fadeOut(tween(100))
