package com.example.chat_tool_harness

import android.app.Activity
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.core.content.FileProvider
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.concurrent.Executor
import java.util.UUID

/** Only core-owned generated text files can become export capabilities. */
internal object ArtifactDocuments {
    fun file(context: Context, path: String, name: String): File {
        require(name.length in 1..128 && !name.startsWith('.') && name == name.trim() &&
            name.none { it.isISOControl() || it == '/' || it == '\\' } &&
            (name.endsWith(".txt") || name.endsWith(".md")))
        val root = File(context.filesDir, "agent/artifacts").canonicalFile
        val source = File(path)
        val canonical = source.canonicalFile
        // Android aliases /data/user/0 to /data/data; validate the canonical parent, not its spelling.
        require(canonical.parentFile == root && !android.system.OsConstants.S_ISLNK(android.system.Os.lstat(source.path).st_mode) &&
            canonical.extension == "txt" && UUID.fromString(canonical.nameWithoutExtension).toString() == canonical.nameWithoutExtension &&
            canonical.isFile && canonical.length() <= 1024 * 1024)
        return canonical
    }
    fun uri(context: Context, file: File, name: String): Uri =
        FileProvider.getUriForFile(context, "${context.packageName}.artifacts", file, name)

    fun mime(name: String) = if (name.endsWith(".md")) "text/markdown" else "text/plain"
    fun shareIntent(context: Context, file: File, name: String): Intent {
        val uri = uri(context, file, name)
        return Intent(Intent.ACTION_SEND).apply {
            type = mime(name)
            putExtra(Intent.EXTRA_STREAM, uri)
            putExtra(Intent.EXTRA_TITLE, name)
            clipData = ClipData.newRawUri(name, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
    }
    fun saveIntent(name: String) = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = mime(name)
        putExtra(Intent.EXTRA_TITLE, name)
        addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION or Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
}

internal interface DocumentIo {
    fun openInput(file: File): InputStream
    fun openOutput(uri: Uri): OutputStream
}

internal class DocumentAccess(
    host: Activity,
    private val channel: MethodChannel,
    private val io: DocumentIo = resolverIo(host.applicationContext),
    private val ioExecutor: Executor = Executor { Thread(it, "artifact-document-io").start() }
) {
    private var activity: Activity? = host
    private val context = host.applicationContext
    private val mainHandler = Handler(Looper.getMainLooper())
    private var pending: SaveOperation? = null

    // Lifecycle, callback and pending state are main-looper confined. Only resource ownership
    // and cancellation cross threads; never hold their lock while calling a provider.
    private class SaveOperation(val file: File, private var result: MethodChannel.Result?) {
        var copying = false
        var release: (() -> Unit)? = null
        private val lock = Any()
        @Volatile private var cancelled = false
        private val resources = mutableListOf<java.io.Closeable>()

        fun checkActive() { check(!cancelled) }
        fun <T : java.io.Closeable> own(resource: T): T {
            val accepted = synchronized(lock) {
                if (cancelled) false else { resources.add(resource); true }
            }
            if (!accepted) {
                // A provider open may return after cancellation; this runs on the worker.
                resource.close()
                checkActive()
            }
            return resource
        }
        private fun takeResources(): List<java.io.Closeable> = synchronized(lock) {
            resources.toList().also { resources.clear() }
        }
        fun closeResources(): Boolean {
            var success = true
            while (true) {
                // Claim one close at a time so cancellation can still claim the other stream
                // if this provider close blocks. Every resource has exactly one closer.
                val resource = synchronized(lock) {
                    if (resources.isEmpty()) null else resources.removeAt(resources.lastIndex)
                } ?: break
                try { resource.close() } catch (_: Exception) { success = false }
            }
            return success
        }
        fun finish(status: String?) {
            val callback = result ?: return
            result = null
            release?.invoke()
            release = null
            if (status != null) callback.success(status)
            else callback.error("unavailable", "Document write failed", null)
        }
        fun cancel(executor: Executor) {
            synchronized(lock) { cancelled = true }
            finish("cancelled")
            // Closing a provider stream may itself block. Independent off-main attempts avoid
            // one stuck close preventing cleanup of the other stream. No external-byte rollback.
            for (resource in takeResources()) executor.execute {
                try { resource.close() } catch (_: Exception) { /* Best-effort cancellation. */ }
            }
        }
    }
    init {
        channel.setMethodCallHandler { call, result ->
            try {
                val currentActivity = checkNotNull(activity)
                val path = call.argument<String>("privatePath") ?: throw IllegalArgumentException()
                val name = call.argument<String>("name") ?: throw IllegalArgumentException()
                val file = ArtifactDocuments.file(context, path, name)
                when (call.method) {
                    "prepareArtifact" -> result.success(ArtifactDocuments.uri(context, file, name).toString())
                    "saveArtifact" -> {
                        check(pending == null)
                        val operation = SaveOperation(file, result)
                        operation.release = { if (pending === operation) pending = null }
                        pending = operation
                        try { currentActivity.startActivityForResult(ArtifactDocuments.saveIntent(name), REQUEST_SAVE) }
                        catch (_: RuntimeException) { operation.finish(null) }
                    }
                    "shareArtifact" -> {
                        check(pending == null)
                        currentActivity.startActivity(Intent.createChooser(ArtifactDocuments.shareIntent(context, file, name), "Share generated text"))
                        // A chooser launch is not evidence that a receiver consumed the file.
                        result.success("shared")
                    }
                    else -> result.notImplemented()
                }
            } catch (_: Exception) { result.error("unavailable", "Document action unavailable", null) }
        }
    }
    fun onActivityResult(request: Int, code: Int, data: Intent?): Boolean {
        if (request != REQUEST_SAVE) return false
        val operation = pending ?: return true
        if (operation.copying) return true
        if (code != Activity.RESULT_OK) { operation.finish("cancelled"); return true }
        val uri = data?.data
        if (uri == null || uri.scheme != "content") { operation.finish(null); return true }
        operation.copying = true
        val documentIo = io
        val handler = mainHandler
        // The worker owns only the operation, application resolver and main handler, never an Activity.
        try { ioExecutor.execute { copy(operation, uri, documentIo, handler) } }
        catch (_: RuntimeException) { operation.finish(null) }
        return true
    }
    fun close() {
        activity = null
        channel.setMethodCallHandler(null)
        pending?.cancel(ioExecutor)
    }
    companion object {
        const val REQUEST_SAVE = 61010
        private fun copy(operation: SaveOperation, uri: Uri, io: DocumentIo, handler: Handler) {
            var success = false
            try {
                operation.checkActive()
                val output = operation.own(io.openOutput(uri))
                operation.checkActive()
                val input = operation.own(io.openInput(operation.file))
                val buffer = ByteArray(8192)
                while (true) {
                    operation.checkActive()
                    val count = input.read(buffer)
                    if (count < 0) break
                    operation.checkActive()
                    output.write(buffer, 0, count)
                }
                operation.checkActive()
                output.flush()
                operation.checkActive()
                success = true
            } catch (_: Exception) { /* Only sanitized terminal failure crosses the channel. */ }
            finally { if (!operation.closeResources()) success = false }
            val status = if (success) "saved" else null
            handler.post { operation.finish(status) }
        }
        private fun resolverIo(context: Context): DocumentIo {
            val resolver = context.contentResolver
            return object : DocumentIo {
                override fun openInput(file: File) = file.inputStream()
                override fun openOutput(uri: Uri) = checkNotNull(resolver.openOutputStream(uri, "w"))
            }
        }
    }
}
