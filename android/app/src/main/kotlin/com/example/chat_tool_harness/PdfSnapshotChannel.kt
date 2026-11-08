package com.example.chat_tool_harness

import android.content.Context
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.io.IOException

/** App-owned handoff for trusted import bytes, never a worker/model path opener. */
internal class PdfSnapshotChannel(context: Context, engine: FlutterEngine) {
    init {
        MethodChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/pdf_snapshot")
            .setMethodCallHandler { call, result ->
                if (call.method != "stage") { result.notImplemented(); return@setMethodCallHandler }
                val bytes = call.arguments as? ByteArray
                if (bytes == null || bytes.isEmpty() || bytes.size > 10 * 1024 * 1024) {
                    result.error("invalid_input", "Invalid PDF snapshot", null)
                    return@setMethodCallHandler
                }
                try {
                    val root = File(context.filesDir, "pdf_staging")
                    if (!root.isDirectory && !root.mkdirs()) throw IOException("staging unavailable")
                    val staged = File.createTempFile("pdf-", ".pdf", root)
                    try {
                        staged.outputStream().use { it.write(bytes) }
                        result.success(staged.absolutePath)
                    } catch (error: IOException) { staged.delete(); throw error }
                } catch (_: IOException) { result.error("unavailable", "Staging failed", null) }
            }
    }
}
