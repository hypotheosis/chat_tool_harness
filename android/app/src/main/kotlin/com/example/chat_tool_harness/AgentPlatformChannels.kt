package com.example.chat_tool_harness

import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel

private fun validId(value: String?): Boolean = try {
    value != null && java.util.UUID.fromString(value).toString() == value
} catch (_: IllegalArgumentException) { false }

class AgentPlatformChannels(engine: FlutterEngine, runtime: AgentRuntime) {
    init {
        MethodChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/agent_runtime")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "state" -> result.success(runtime.state())
                    "startRunService" -> {
                        val run = call.argument<String>("runId")
                        val conversation = call.argument<String>("conversationId")
                        val generation = call.argument<String>("generation")
                        if (!validId(run) || !validId(conversation) || !validId(generation)) result.error("INVALID_ID", "Invalid identity", null)
                        else runtime.start(run!!, conversation!!, generation!!) { started ->
                            if (started) result.success(null)
                            else result.error("SERVICE_REFUSED", "Foreground service unavailable. Open app and explicitly resume.", null)
                        }
                    }
                    "stopRunService" -> {
                        val run = call.argument<String>("runId")
                        val conversation = call.argument<String>("conversationId")
                        val generation = call.argument<String>("generation")
                        if (!validId(run) || !validId(conversation) || !validId(generation)) result.error("INVALID_ID", "Invalid identity", null)
                        else {
                            runtime.cleanup(run!!, conversation!!, generation!!, call.argument<Boolean>("waitingApproval") == true)
                            result.success(null)
                        }
                    }
                    else -> result.notImplemented()
                }
            }
        EventChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/agent_lifecycle")
            .setStreamHandler(object : EventChannel.StreamHandler {
                override fun onListen(arguments: Any?, events: EventChannel.EventSink?) {
                    runtime.listen { command -> events?.success(command) }
                }
                override fun onCancel(arguments: Any?) {
                    runtime.listen(null)
                }
            })
    }
}
