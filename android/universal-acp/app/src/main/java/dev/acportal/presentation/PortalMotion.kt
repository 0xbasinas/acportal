package dev.acportal.presentation

import androidx.compose.animation.ExitTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.LinearOutSlowInEasing
import androidx.compose.animation.core.tween

/** One short fade-in over the page that is leaving. The outgoing page stays opaque until the
 * incoming one covers it, so nothing dips through the background (the flash a fade-out/fade-in
 * overlap caused) and nothing slides or scales. Used for push, Back and predictive Back alike.
 * Compose still applies the system animator duration scale, including disabled animations. */
internal fun portalPageTransition() = fadeIn(tween(90,easing=LinearOutSlowInEasing)) togetherWith ExitTransition.None
