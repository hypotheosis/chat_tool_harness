package com.example.chat_tool_harness

import android.app.ActivityManager
import android.app.Application
import android.content.Context
import android.os.Build
import android.os.Process
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

internal fun processRole(application: Application): String {
    val name = if (Build.VERSION.SDK_INT >= 28) Application.getProcessName() else {
        val manager = application.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
        manager.runningAppProcesses?.firstOrNull { it.pid == Process.myPid() }?.processName
    }
    return when (name) {
        application.packageName -> "main"
        "${application.packageName}:pdf_worker" -> "pdf_worker"
        else -> if (isPdfWorkerTestProcess(name, application.packageName)) "pdf_test_client" else "unknown"
    }
}

/** Only worker-owned control/watchdog paths may terminate this process. */
internal fun workerSelfTerminate(application: Application) {
    if (processRole(application) == "pdf_worker") Process.killProcess(Process.myPid())
}

internal class PdfWorkerChannels(
    engine: FlutterEngine,
    private val service: PdfWorkerService,
) {
    init {
        MethodChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/pdf_worker_engine")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "ready" -> {
                        service.onEngineReady()
                        result.success(null)
                    }
                    "finished" -> {
                        service.onEngineFinished()
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
    }
}
