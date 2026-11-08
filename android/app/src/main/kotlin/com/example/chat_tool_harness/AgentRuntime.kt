package com.example.chat_tool_harness

import android.app.Activity
import android.app.Application
import android.app.NotificationManager
import android.content.Intent
import android.os.Bundle
import io.flutter.embedding.engine.FlutterEngine
import java.util.UUID

data class RunIdentity(val runId: String, val conversationId: String, val lease: String) {
    fun event(action: String): Map<String, String> = mapOf("action" to action, "runId" to runId, "conversationId" to conversationId, "lease" to lease)
}

class AgentRuntime(private val application: Application, val engine: FlutterEngine) :
    Application.ActivityLifecycleCallbacks {
    val id: String = UUID.randomUUID().toString()
    var current: RunIdentity? = null
        private set
    val activeRuns: Int get() = if (current == null) 0 else 1
    var status = "idle"
        private set
    private var resumedActivity: Activity? = null
    private var admitted: RunIdentity? = null
    private var pendingStart: ((Boolean) -> Unit)? = null
    private var callback: ((Map<String, String>) -> Unit)? = null
    private val queuedCommands = ArrayDeque<Map<String, String>>()
    private val pending = application.getSharedPreferences("agent_pending_open", 0)
    init { application.registerActivityLifecycleCallbacks(this) }
    fun listen(listener: ((Map<String, String>) -> Unit)?) {
        callback = listener
        if (listener != null) while (queuedCommands.isNotEmpty()) listener(queuedCommands.removeFirst())
    }
    private fun emit(identity: RunIdentity, action: String) {
        emitEvent(identity.event(action))
    }
    private fun emitEvent(event: Map<String, String>) {
        callback?.invoke(event) ?: queuedCommands.addLast(event)
    }
    fun start(runId: String, conversationId: String, generation: String, acknowledge: (Boolean) -> Unit) {
        val identity = RunIdentity(runId, conversationId, generation)
        if (pendingStart != null || resumedActivity == null || current != null) { emit(identity, "refused"); acknowledge(false); return }
        admitted = identity
        current = identity
        pendingStart = acknowledge
        status = "starting"
        application.getSystemService(NotificationManager::class.java).cancel(runId, AgentService.PENDING_ID)
        if (!pending.edit().putString(runId, conversationId).commit()) {
            lose(identity, "refused")
            return
        }
        try {
            val intent = Intent(application, AgentService::class.java).putExtra("lease", identity.lease)
            if (android.os.Build.VERSION.SDK_INT >= 26) application.startForegroundService(intent) else application.startService(intent)
        } catch (_: RuntimeException) { lose(identity, "refused") }
    }
    fun started(identity: RunIdentity) {
        if (current != identity) return
        status = "running"
        val ack = pendingStart
        pendingStart = null
        ack?.invoke(true)
    }
    /** Core has persisted its final state before requesting normal cleanup. */
    fun cleanup(runId: String, conversationId: String, generation: String, waiting: Boolean) {
        val identity = admitted?.takeIf { it.runId == runId && it.conversationId == conversationId && it.lease == generation } ?: return
        if (waiting) {
            check(pending.edit().putString(runId, conversationId).commit())
        } else {
            pending.edit().remove(runId).apply()
            application.getSystemService(NotificationManager::class.java).cancel(runId, AgentService.PENDING_ID)
        }
        if (current != identity) return
        current = null
        status = if (waiting) "waiting" else "idle"
        val ack = pendingStart
        pendingStart = null
        ack?.invoke(false)
        application.stopService(Intent(application, AgentService::class.java))
        if (waiting) AgentService.showPending(application, identity)
    }
    fun lose(identity: RunIdentity, reason: String) {
        if (current != identity) return
        current = null
        status = reason
        val ack = pendingStart
        pendingStart = null
        ack?.invoke(false)
        emit(identity, reason)
        application.stopService(Intent(application, AgentService::class.java))
    }
    fun stop(identity: RunIdentity) = lose(identity, "stop")
    fun open(runId: String?, conversationId: String?) {
        if (runId == null || conversationId == null) return
        val active = current?.takeIf { it.runId == runId && it.conversationId == conversationId }
        if (active != null || pending.getString(runId, null) == conversationId) {
            emitEvent(mapOf("action" to "open", "runId" to runId, "conversationId" to conversationId))
        }
    }
    fun state(): Map<String, Any> = mapOf(
        "runtimeId" to id, "activeRuns" to activeRuns, "status" to status,
        "runId" to (current?.runId ?: ""), "lease" to (current?.lease ?: ""),
        "notificationVisibility" to if (!application.getSystemService(NotificationManager::class.java).areNotificationsEnabled()) "reduced" else "normal"
    )
    override fun onActivityResumed(activity: Activity) { resumedActivity = activity }
    override fun onActivityPaused(activity: Activity) { if (resumedActivity === activity) resumedActivity = null }
    override fun onActivityDestroyed(activity: Activity) { if (resumedActivity === activity) resumedActivity = null }
    override fun onActivityCreated(activity: Activity, state: Bundle?) = Unit
    override fun onActivityStarted(activity: Activity) = Unit
    override fun onActivityStopped(activity: Activity) = Unit
    override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
}
