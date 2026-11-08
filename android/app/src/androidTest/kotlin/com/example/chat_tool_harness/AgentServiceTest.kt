package com.example.chat_tool_harness

import android.app.Notification
import android.app.NotificationManager
import android.content.Intent
import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class AgentServiceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val app get() = instrumentation.targetContext.applicationContext as AgentApplication
    private fun awaitCondition(test: () -> Boolean) {
        val deadline = android.os.SystemClock.elapsedRealtime() + 5000
        while (!test() && android.os.SystemClock.elapsedRealtime() < deadline) {
            instrumentation.waitForIdleSync()
            android.os.SystemClock.sleep(10)
        }
        assertTrue(test())
    }
    private fun permission(grant: Boolean) {
        if (Build.VERSION.SDK_INT >= 33) instrumentation.uiAutomation.executeShellCommand(
            "pm ${if (grant) "grant" else "revoke"} ${app.packageName} android.permission.POST_NOTIFICATIONS"
        ).use { android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes() }
    }
    private fun start(previous: RunIdentity? = null): RunIdentity {
        val started = CountDownLatch(1)
        var accepted = false
        instrumentation.runOnMainSync {
            app.runtime.start(previous?.runId ?: UUID.randomUUID().toString(), previous?.conversationId ?: UUID.randomUUID().toString(), UUID.randomUUID().toString()) { accepted = it; started.countDown() }
        }
        assertTrue(started.await(5, TimeUnit.SECONDS))
        assertTrue(accepted)
        return app.runtime.current!!
    }
    private fun cleanup(identity: RunIdentity, waiting: Boolean = false) {
        instrumentation.runOnMainSync { app.runtime.cleanup(identity.runId, identity.conversationId, identity.lease, waiting) }
        awaitCondition { AgentService.activeService == null }
    }
    @Test fun notification_contains_no_payload_and_stale_notification_cannot_execute() {
        permission(true)
        val activity = instrumentation.startActivitySync(Intent(app, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            val first = start()
            val stale = AgentService.stopAction(AgentService.activeService!!)
            awaitCondition { app.getSystemService(NotificationManager::class.java).activeNotifications.any { it.id == AgentService.NOTIFICATION_ID } }
            val notification = app.getSystemService(NotificationManager::class.java).activeNotifications.single { it.id == AgentService.NOTIFICATION_ID }.notification
            assertEquals("Agent running", notification.extras.getString(Notification.EXTRA_TITLE))
            assertEquals(1, notification.actions.size)
            assertEquals(setOf(Notification.EXTRA_TITLE, Notification.EXTRA_TEXT).map { notification.extras.getString(it) }.any { it?.contains(first.runId) == true }, false)
            cleanup(first, true)
            awaitCondition { app.getSystemService(NotificationManager::class.java).activeNotifications.any { it.id == AgentService.PENDING_ID } }
            val pending = app.getSystemService(NotificationManager::class.java).activeNotifications.single { it.id == AgentService.PENDING_ID }.notification
            assertEquals("Agent needs attention", pending.extras.getString(Notification.EXTRA_TITLE))
            assertEquals(0, pending.actions?.size ?: 0)
            assertNotNull(pending.contentIntent)
            val second = start(first)
            assertNotEquals(first.lease, second.lease)
            stale.send()
            instrumentation.runOnMainSync { app.runtime.cleanup(first.runId, first.conversationId, first.lease, true) }
            instrumentation.waitForIdleSync()
            assertEquals(second, app.runtime.current)
            awaitCondition { app.getSystemService(NotificationManager::class.java).activeNotifications.none { it.id == AgentService.PENDING_ID && it.tag == first.runId } }
            AgentService.stopAction(AgentService.activeService!!).send()
            awaitCondition { app.runtime.current == null }
            cleanup(second)
            instrumentation.runOnMainSync { app.runtime.cleanup(first.runId, first.conversationId, first.lease, false) }
        } finally {
            app.runtime.current?.let { cleanup(it) }
            app.getSystemService(NotificationManager::class.java).activeNotifications.filter { it.id == AgentService.PENDING_ID }.forEach { app.getSystemService(NotificationManager::class.java).cancel(it.tag, it.id) }
            instrumentation.runOnMainSync { activity.finish() }
        }
    }
    @Test fun notification_denial_reduces_visibility_but_foreground_start_acknowledges() {
        // Revoke on the host before instrumentation: Android may kill the target on revoke.
        assertFalse("Revoke the test app's notification permission before this isolated instrumentation invocation", app.getSystemService(NotificationManager::class.java).areNotificationsEnabled())
        val activity = instrumentation.startActivitySync(Intent(app, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            val identity = start()
            assertEquals("reduced", app.runtime.state()["notificationVisibility"])
            assertEquals(1, app.runtime.activeRuns)
            cleanup(identity)
            // No resumed Activity: real native refusal, not a fabricated callback.
            instrumentation.runOnMainSync { activity.finish() }
            awaitCondition { activity.isDestroyed }
            val latch = CountDownLatch(1)
            var accepted = true
            instrumentation.runOnMainSync { app.runtime.start(UUID.randomUUID().toString(), UUID.randomUUID().toString(), UUID.randomUUID().toString()) { accepted = it; latch.countDown() } }
            assertTrue(latch.await(5, TimeUnit.SECONDS))
            assertFalse(accepted)
        } finally {
            app.runtime.current?.let { cleanup(it) }
            permission(true)
            instrumentation.runOnMainSync { if (!activity.isDestroyed) activity.finish() }
        }
    }
}
