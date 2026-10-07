package dev.acportal

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.SideEffect
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.core.view.WindowCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.acportal.presentation.*

class MainActivity:ComponentActivity() {
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val repository=(application as PortalApplication).repository
        setContent {
            val vm:PortalViewModel=viewModel(factory=viewModelFactory { initializer { PortalViewModel(repository) } })
            val settings by vm.settings.collectAsStateWithLifecycle()
            val systemDark=isSystemInDarkTheme()
            val dark=when(settings.theme) {"dark"->true;"light"->false;else->systemDark}
            SideEffect {WindowCompat.getInsetsController(window,window.decorView).apply {isAppearanceLightStatusBars=!dark;isAppearanceLightNavigationBars=!dark}}
            PortalTheme(settings.theme) { PortalNavigation(vm) }
        }
    }
}
