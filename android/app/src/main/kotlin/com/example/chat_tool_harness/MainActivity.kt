package com.example.chat_tool_harness

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine

class MainActivity : FlutterActivity() {
    private var documents: DocumentAccess? = null
    override fun onCreate(state: android.os.Bundle?) {
        super.onCreate(state)
        routeOpen(intent)
        val engine = (application as AgentApplication).runtime.engine
        documents = DocumentAccess(this, io.flutter.plugin.common.MethodChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/documents"))
    }
    private fun routeOpen(intent: android.content.Intent?) {
        if (intent?.action == "com.example.chat_tool_harness.OPEN_AGENT") {
            (application as AgentApplication).runtime.open(intent.getStringExtra("runId"), intent.getStringExtra("conversationId"))
        }
    }
    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        routeOpen(intent)
    }
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: android.content.Intent?) {
        if (documents?.onActivityResult(requestCode, resultCode, data) != true) super.onActivityResult(requestCode, resultCode, data)
    }
    override fun onDestroy() {
        documents?.close()
        documents = null
        super.onDestroy()
    }
    override fun provideFlutterEngine(context: android.content.Context): FlutterEngine =
        (application as AgentApplication).runtime.engine

    override fun shouldDestroyEngineWithHost(): Boolean = false
}
