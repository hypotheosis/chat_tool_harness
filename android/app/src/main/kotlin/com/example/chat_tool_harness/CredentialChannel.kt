package com.example.chat_tool_harness

import android.content.Context
import android.os.Handler
import android.os.Looper
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.Executors

class CredentialChannel(context: Context, engine: FlutterEngine) {
    private val store by lazy { CredentialStore(context) }
    private val executor = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())
    init {
        MethodChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/credentials")
            .setMethodCallHandler { call, result ->
                if (call.method !in setOf("put", "resolve", "describe", "delete")) {
                    result.notImplemented()
                } else {
                    executor.execute {
                        var value: ByteArray? = null
                        try {
                            val reference = call.argument<String>("reference") ?: error("reference")
                            val response: Any? = when (call.method) {
                                "put" -> {
                                    value = call.argument<ByteArray>("value") ?: error("value")
                                    store.put(reference, value!!)
                                    null
                                }
                                "resolve" -> store.resolve(reference)
                                "describe" -> store.describe(reference)
                                else -> { store.delete(reference); null }
                            }
                            main.post {
                                try { result.success(response) }
                                finally { if (response is ByteArray) response.fill(0) }
                            }
                        } catch (_: Exception) {
                            main.post { result.error("CREDENTIAL_UNAVAILABLE", "Credential unavailable. Delete and re-enter it.", null) }
                        } finally {
                            value?.fill(0)
                        }
                    }
                }
            }
    }
}
