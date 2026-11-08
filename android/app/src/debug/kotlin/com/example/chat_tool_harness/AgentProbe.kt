package com.example.chat_tool_harness

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/** Debug-only lifecycle probe. No fixture commands or counters ship in release. */
class AgentProbe(private val application: AgentApplication, engine: FlutterEngine) :
    Application.ActivityLifecycleCallbacks {
    private val handler = Handler(Looper.getMainLooper())
    private var activity: Activity? = null
    private var count = 0
    private var steps = 0
    private var savedStop: android.app.PendingIntent? = null
    private var waiting: MethodChannel.Result? = null
    private var finishWaiting: MethodChannel.Result? = null
    private var restoring: MethodChannel.Result? = null

    init {
        application.registerActivityLifecycleCallbacks(this)
        MethodChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/agent_probe")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "scenarioDirectory" -> {
                        val name = call.arguments as? String
                        if (name == null || !Regex("[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").matches(name)) {
                            result.error("INVALID_SCENARIO", "Expected test UUID", null)
                        } else {
                            val root = java.io.File(application.noBackupFilesDir, "task12-process-$name")
                            root.mkdirs()
                            result.success(root.path)
                        }
                    }
                    "stepWithoutActivity" -> {
                        if (waiting != null || activity == null || AgentService.activeService == null) {
                            result.error("NO_ACTIVITY_OR_SERVICE", "Probe requires active Activity and service", null)
                        } else {
                            waiting = result
                            activity?.finish()
                        }
                    }
                    "restoreActivity" -> {
                        if (count != 0 || restoring != null) {
                            result.error("ACTIVITY_PRESENT", "Expected no Activity", null)
                        } else {
                            restoring = result
                            application.startActivity(Intent(application, MainActivity::class.java)
                                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        }
                    }
                    "finishActivity" -> {
                        if (activity == null) result.error("NO_ACTIVITY", "No Activity", null)
                        else {
                            finishWaiting = result
                            activity?.finish()
                        }
                    }
                    "dispatchNotificationStop" -> {
                        val service = AgentService.activeService
                        if (service == null) result.error("NO_SERVICE", "No active service", null)
                        else {
                            AgentService.stopAction(service).send()
                            result.success(null)
                        }
                    }
                    "captureStop" -> {
                        savedStop = AgentService.activeService?.let { AgentService.stopAction(it) }
                        result.success(savedStop != null)
                    }
                    "dispatchSavedStop" -> { savedStop?.send(); result.success(null) }
                    "loseService" -> {
                        application.stopService(Intent(application, AgentService::class.java))
                        result.success(null)
                    }
                    "notificationState" -> {
                        val notifications = application.getSystemService(android.app.NotificationManager::class.java).activeNotifications
                        result.success(notifications.map { item -> mapOf(
                            "id" to item.id,
                            "title" to item.notification.extras.getString(android.app.Notification.EXTRA_TITLE),
                            "text" to item.notification.extras.getString(android.app.Notification.EXTRA_TEXT),
                            "actions" to (item.notification.actions?.size ?: 0)
                        ) })
                    }
                    "openPending" -> {
                        val item = application.getSystemService(android.app.NotificationManager::class.java).activeNotifications.firstOrNull { it.id == AgentService.PENDING_ID }
                        if (item == null) result.error("NO_PENDING", "No pending notification", null)
                        else { item.notification.contentIntent.send(); result.success(null) }
                    }
                    "pdfFixture" -> {
                        val fixture = call.arguments as? String
                        if (fixture !in setOf("two_pages", "encrypted_empty_password", "malformed")) {
                            result.error("UNKNOWN_FIXTURE", "Unknown test fixture", null)
                        } else {
                            val root = java.io.File(application.filesDir, "pdf_staging")
                            root.mkdirs()
                            val file = java.io.File(root, "$fixture.pdf")
                            application.assets.open("pdf_fixtures/$fixture.pdf").use { source ->
                                file.outputStream().use { source.copyTo(it) }
                            }
                            result.success(file.canonicalPath)
                        }
                    }
                    "pdfDiagnostics" -> result.success(application.pdfWorkerHost.diagnostics())
                    "pdfFault" -> {
                        val mode = call.arguments as? String
                        if (mode !in setOf("abort", "partial", "withhold")) {
                            result.error("UNKNOWN_FAULT", "Unknown worker fault", null)
                        } else {
                            setPdfWorkerDebugFault(mode!!)
                            result.success(null)
                        }
                    }
                    "stepIfActive" -> result.success(
                        AgentService.activeService?.runIfActive { steps++ } ?: false
                    )
                    else -> result.notImplemented()
                }
            }
    }

    private fun snapshot(): Map<String, Any> = application.runtime.state() + mapOf(
        "activityCount" to count, "steps" to steps
    )

    override fun onActivityCreated(activity: Activity, state: Bundle?) {
        count++
        this.activity = activity
    }
    override fun onActivityDestroyed(activity: Activity) {
        count--
        if (this.activity === activity) this.activity = null
        if (count == 0 && finishWaiting != null) {
            finishWaiting?.success(snapshot())
            finishWaiting = null
        }
        if (count == 0 && waiting != null) {
            // Post after the destruction callback so the work runs from the
            // retained service, not inside the Activity teardown stack.
            handler.post {
                val result = waiting ?: return@post
                waiting = null
                if (count != 0) result.error("ACTIVITY_PRESENT", "Activity was restored early", null)
                else if (AgentService.activeService?.runIfActive { steps++ } == true) result.success(snapshot())
                else result.error("NO_SERVICE", "Service did not own the step", null)
            }
        }
    }
    override fun onActivityResumed(activity: Activity) {
        if (restoring != null && count == 1) {
            val result = restoring ?: return
            restoring = null
            result.success(snapshot())
        }
    }
    override fun onActivityStarted(activity: Activity) = Unit
    override fun onActivityPaused(activity: Activity) = Unit
    override fun onActivityStopped(activity: Activity) = Unit
    override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
}
