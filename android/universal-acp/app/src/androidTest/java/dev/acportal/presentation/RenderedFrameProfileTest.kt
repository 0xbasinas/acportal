package dev.acportal.presentation

import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.view.FrameMetrics
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.SessionState
import dev.acportal.protocol.TimelineItem
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.util.Collections

/** Actual rendered-window durations on an isolated fixture; emulator results are not phone budgets. */
class RenderedFrameProfileTest {
    @Test fun darkStreamingWindowReportsFrameDurations() = profile("dark")
    @Test fun lightStreamingWindowReportsFrameDurations() = profile("light")
    private fun profile(theme: String) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        assumeTrue("Profiling requires separate acceptance storage", context.packageName == "dev.acportal.acceptance")
        val durations = Collections.synchronizedList(mutableListOf<Long>())
        val thread = HandlerThread("owned-frame-profile").apply { start() }
        val listener = android.view.Window.OnFrameMetricsAvailableListener { _, metrics, _ ->
            if (metrics.getMetric(FrameMetrics.FIRST_DRAW_FRAME) == 0L) {
                durations.add(metrics.getMetric(FrameMetrics.TOTAL_DURATION))
            }
        }
        val scenario = ActivityScenario.launch<UiAccessibilityFixtureActivity>(Intent(context, UiAccessibilityFixtureActivity::class.java)
            .putExtra("mode", theme).putExtra("page", "conversation"))
        try {
            val deadline = SystemClock.uptimeMillis() + 10000
            var ready = false
            while (!ready && SystemClock.uptimeMillis() < deadline) {
                scenario.onActivity { ready = it.fixtureLive != null }
                SystemClock.sleep(25)
            }
            assertTrue("Conversation fixture did not render", ready)
            scenario.onActivity { it.window.addOnFrameMetricsAvailableListener(listener, Handler(thread.looper)) }
            // Produce visible Markdown changes with the real SessionScreen while issuing no prompts.
            repeat(120) { index ->
                scenario.onActivity { activity ->
                    activity.fixtureLive!!.state.value = SessionState(processing=true,
                        items=listOf(TimelineItem.Text("stream", "agent", "# Fixture stream\n\n" + "Visible chunk $index. ".repeat(20))))
                }
                SystemClock.sleep(33)
            }
            SystemClock.sleep(250)
            val samples = synchronized(durations) { durations.sorted() }
            assertTrue("No useful rendered frame samples", samples.size >= 20)
            fun percentile(fraction: Double) = samples[((samples.size - 1) * fraction).toInt()] / 1_000_000.0
            instrumentation.sendStatus(2, Bundle().apply {
                putString("acportalRenderedFrames", "theme=$theme; updates=120; samples=${samples.size}; p50Ms=${percentile(.5)}; p95Ms=${percentile(.95)}; p99Ms=${percentile(.99)}; maxMs=${samples.last()/1_000_000.0}; emulator/debug fixture")
            })
        } finally {
            scenario.onActivity { it.window.removeOnFrameMetricsAvailableListener(listener) }
            scenario.close()
            thread.quitSafely(); thread.join(5000)
        }
    }
}
