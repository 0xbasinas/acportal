package dev.acportal.presentation

import android.os.Bundle
import android.os.Parcel
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.*
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.acportal.data.PortalRepository

/** Debug-only entry point for real Activity recreation with isolated storage. */
class McpLifecycleFixtureActivity:ComponentActivity() {
    companion object {
        var fixtureRepository by mutableStateOf<PortalRepository?>(null)
        var savedBytes:ByteArray?=null
    }
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            fixtureRepository?.let {repository->
                val vm:PortalViewModel=viewModel(factory=viewModelFactory {initializer {PortalViewModel(repository)}})
                PortalTheme {PortalNavigation(vm)}
            }
        }
    }
    override fun onSaveInstanceState(outState:Bundle) {
        super.onSaveInstanceState(outState)
        val parcel=Parcel.obtain()
        try {parcel.writeBundle(outState);savedBytes=parcel.marshall()} finally {parcel.recycle()}
    }
}
