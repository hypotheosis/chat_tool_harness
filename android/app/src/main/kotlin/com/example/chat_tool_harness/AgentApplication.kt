package com.example.chat_tool_harness

import android.app.Application
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterEngineCache

class AgentApplication : Application() {
    lateinit var runtime: AgentRuntime
        private set
    internal lateinit var pdfWorkerHost: PdfWorkerHost
        private set

    override fun onCreate() {
        super.onCreate()
        when (processRole(this)) {
            "pdf_worker", "pdf_test_client" -> return // No chat engine, runtime, or plugin registration.
            "main" -> Unit
            else -> throw IllegalStateException("Unknown application process role")
        }
        val engine = FlutterEngine(this)
        FlutterEngineCache.getInstance().put("agent_runtime", engine)
        runtime = AgentRuntime(this, engine)
        AgentPlatformChannels(engine, runtime)
        CredentialChannel(this, engine)
        pdfWorkerHost = PdfWorkerHost(this, engine)
        PdfSnapshotChannel(this, engine)
        installDebugProbe(engine)
        engine.dartExecutor.executeDartEntrypoint(
            io.flutter.embedding.engine.dart.DartExecutor.DartEntrypoint.createDefault()
        )
    }
}
