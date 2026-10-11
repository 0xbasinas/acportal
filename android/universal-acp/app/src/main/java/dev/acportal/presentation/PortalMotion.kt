package dev.acportal.presentation

import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.FastOutLinearInEasing
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.tween

internal const val PORTAL_PAGE_FADE_MS=90

/** Fade-through: the leaving page fades out, then the new page fades in, so the two pages' text is
 * never on screen together (a plain cross-fade shows both, which read as a glitch). Both pages use
 * the screen background, so only the content dips. Total time is [PORTAL_PAGE_FADE_MS]. Used for
 * push, Back and predictive Back alike; nothing slides or scales. Compose still applies the system
 * animator duration scale, including disabled animations. */
internal fun portalPageTransition() = fadeIn(tween(PORTAL_PAGE_FADE_MS/2,delayMillis=PORTAL_PAGE_FADE_MS/2,easing=LinearOutSlowInEasing)) togetherWith fadeOut(tween(PORTAL_PAGE_FADE_MS/2,easing=FastOutLinearInEasing))
