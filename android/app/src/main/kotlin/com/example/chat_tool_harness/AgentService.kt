package com.example.chat_tool_harness

import android.app.Application
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.IBinder
import android.os.Build

class AgentService : Service() {
    private var identity: RunIdentity? = null
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onCreate() { super.onCreate(); activeService = this }
    override fun onDestroy() {
        if (activeService === this) activeService = null
        identity?.let { (application as AgentApplication).runtime.lose(it, "hostLost") }
        super.onDestroy()
    }
    internal fun runIfActive(work: () -> Unit): Boolean {
        if ((application as AgentApplication).runtime.current != identity || identity == null) return false
        work(); return true
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val runtime = (application as AgentApplication).runtime
        val active = runtime.current
        if (intent?.action == ACTION_STOP) {
            if (active != null && intent.getStringExtra("lease") == active.lease) runtime.stop(active)
            if (active == null) stopSelf()
            return START_NOT_STICKY
        }
        if (active == null || intent?.getStringExtra("lease") != active.lease) {
            if (active == null) stopSelf()
            return START_NOT_STICKY
        }
        identity = active
        try {
            val manager = getSystemService(NotificationManager::class.java)
            channel(manager)
            val notification = builder(application)
                .setSmallIcon(applicationInfo.icon).setContentTitle("Agent running")
                .setContentText("Work in progress; Stop is local, not remote rollback")
                .setVisibility(Notification.VISIBILITY_PRIVATE)
                .setContentIntent(openAction(application, active))
                .addAction(Notification.Action.Builder(null, "Stop", stopAction(this)).build())
                .setOngoing(true).build()
            if (Build.VERSION.SDK_INT >= 29) startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
            else startForeground(NOTIFICATION_ID, notification)
            runtime.started(active)
        } catch (_: RuntimeException) { runtime.lose(active, "refused"); stopSelf() }
        return START_NOT_STICKY
    }
    override fun onTimeout(startId: Int, fgsType: Int) {
        identity?.let { (application as AgentApplication).runtime.lose(it, "timeout") }
        stopSelf()
    }
    companion object {
        private const val CHANNEL = "agent_run"
        internal const val NOTIFICATION_ID = 1201
        internal const val PENDING_ID = 1202
        private const val ACTION_STOP = "com.example.chat_tool_harness.STOP_AGENT"
        internal var activeService: AgentService? = null
            private set
        private fun channel(manager: NotificationManager) {
            if (Build.VERSION.SDK_INT >= 26) manager.createNotificationChannel(NotificationChannel(CHANNEL, "Agent status", NotificationManager.IMPORTANCE_LOW))
        }
        private fun builder(app: Application): Notification.Builder = if (Build.VERSION.SDK_INT >= 26) Notification.Builder(app, CHANNEL) else Notification.Builder(app)
        internal fun openAction(app: Application, identity: RunIdentity): PendingIntent = PendingIntent.getActivity(
            app, 0, Intent(app, MainActivity::class.java).setAction("com.example.chat_tool_harness.OPEN_AGENT")
                .setData(Uri.parse("agent-open:${identity.runId}:${identity.lease}"))
                .putExtra("runId", identity.runId).putExtra("conversationId", identity.conversationId),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        internal fun stopAction(service: AgentService): PendingIntent {
            val identity = service.identity ?: error("No active run")
            return PendingIntent.getService(service, 1,
                Intent(service, AgentService::class.java).setAction(ACTION_STOP)
                    .setData(Uri.parse("agent-stop:${identity.lease}"))
                    .putExtra("lease", identity.lease),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }
        internal fun showPending(app: Application, identity: RunIdentity) {
            val manager = app.getSystemService(NotificationManager::class.java)
            channel(manager)
            try {
                manager.notify(identity.runId, PENDING_ID, builder(app)
                    .setSmallIcon(app.applicationInfo.icon).setContentTitle("Agent needs attention")
                    .setContentText("Open the conversation to review")
                    .setVisibility(Notification.VISIBILITY_PRIVATE)
                    .setContentIntent(openAction(app, identity)).setAutoCancel(true).build())
            } catch (_: SecurityException) { /* Permission denial reduces visibility, not run eligibility. */ }
        }
    }
}
