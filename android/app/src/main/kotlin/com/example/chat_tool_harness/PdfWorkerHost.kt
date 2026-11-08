package com.example.chat_tool_harness

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Binder
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.Process
import android.os.SystemClock
import android.system.Os
import android.system.OsConstants
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.io.FileInputStream
import java.io.IOException
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong

/** Main-process broker. State transitions run on main; no remote Binder transaction does. */
internal class PdfWorkerHost(
    private val context: Context, engine: FlutterEngine,
    private val binding: Binding = Binding(context),
    private val clock: Clock = Clock(),
    private val scheduler: ScheduledExecutorService = Executors.newSingleThreadScheduledExecutor(),
    private val controlExecutor: java.util.concurrent.ExecutorService = Executors.newFixedThreadPool(2),
    private val reader: java.util.concurrent.ExecutorService = Executors.newSingleThreadExecutor(),
    private val open: (File) -> ParcelFileDescriptor = { ParcelFileDescriptor.open(it, ParcelFileDescriptor.MODE_READ_ONLY) },
) {
    internal open class Binding(private val context: Context) {
        open fun bind(intent: Intent, connection: ServiceConnection): Boolean =
            context.bindService(intent, connection, Context.BIND_AUTO_CREATE)
        open fun unbind(connection: ServiceConnection) = context.unbindService(connection)
    }
    internal open class Clock { open fun now() = SystemClock.elapsedRealtime() }
    private val main = Handler(Looper.getMainLooper())
    private val generation = AtomicLong()
    @Volatile private var job: Job? = null
    @Volatile private var unavailable = false
    private var lastPid = 0
    private var lastRole = "unknown"
    private var lastChat = true
    private var lastDeath = "none"

    private inner class Job(
        val id: String, val generation: Long, val result: MethodChannel.Result,
        val maxInput: Int, val maxPages: Int, val maxText: Int, val maxDecompressed: Int,
        val input: ParcelFileDescriptor, val read: ParcelFileDescriptor, val write: ParcelFileDescriptor,
        val deadline: Long,
    ) {
        @Volatile var binder: IBinder? = null
        var control: IPdfWorker? = null
        var bound = false
        var transactionQueued = false
        @Volatile var started = false
        var validated = false
        @Volatile var finished = false
        var dead = false
        @Volatile var diedBeforeTransport = false
        var readerDone = false
        var readerFailure: String? = null
        @Volatile var sawOutput = false
        @Volatile var transportComplete = false
        var disconnected = false
        var pending: Map<String, Any>? = null
        var pendingFailure: String? = null
        var deathRecipient: IBinder.DeathRecipient? = null
        var graceScheduled = false
        val events = object : IPdfWorkerEvents.Stub() {
            override fun started(requestId: String, requestedGeneration: Long): Boolean {
                val pid = Binder.getCallingPid()
                val accepted = Binder.getCallingUid() == Process.myUid() && pid != Process.myPid() &&
                    requestId == id && requestedGeneration == generation &&
                    job === this@Job && binder?.isBinderAlive == true && !finished && clock.now() < deadline
                if (accepted) main.post {
                    if (job === this@Job && !finished) {
                        started = true
                        lastPid = pid
                        lastRole = "pdf_worker"
                        lastChat = false
                    }
                }
                return accepted
            }
        }
        val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName, service: IBinder) {
                main.post { connected(this@Job, service) }
            }
            override fun onServiceDisconnected(name: ComponentName) {
                val completeAtDisconnect = transportComplete
                if (!completeAtDisconnect) diedBeforeTransport = true
                main.post { disconnected(this@Job, completeAtDisconnect) }
            }
            override fun onBindingDied(name: ComponentName) = onServiceDisconnected(name)
        }
    }

    init {
        MethodChannel(engine.dartExecutor.binaryMessenger, "chat_tool_harness/pdf_worker")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "extract" -> extract(call.arguments as? Map<*, *>, result)
                    "cancel" -> {
                        val id = (call.arguments as? Map<*, *>)?.get("requestId") as? String
                        if (id != null && cancel(id)) result.success(mapOf("kind" to "ok"))
                        else result.success(failed("unavailable"))
                    }
                    else -> result.notImplemented()
                }
            }
    }

    fun diagnostics(): Map<String, Any> = mapOf(
        "workerPid" to lastPid, "workerRole" to lastRole,
        "workerChatInitialized" to lastChat, "mainPid" to Process.myPid(),
        "lastDeath" to lastDeath, "unavailable" to unavailable,
    )

    private fun connected(current: Job, service: IBinder) {
        if (job !== current || current.finished || current.disconnected || current.binder != null) return
        if (clock.now() >= current.deadline) { fail(current, "timeout"); return }
        current.binder = service
        current.control = IPdfWorker.Stub.asInterface(service)
        try {
            current.deathRecipient = IBinder.DeathRecipient {
                val completeAtDeath = current.transportComplete
                if (!completeAtDeath) current.diedBeforeTransport = true
                main.post { death(current, completeAtDeath) }
            }
            service.linkToDeath(current.deathRecipient!!, 0)
            current.transactionQueued = true
            controlExecutor.execute {
                try {
                    current.control!!.start(current.id, current.generation, current.input, current.write,
                        current.maxInput, current.maxPages, current.maxText, current.maxDecompressed, current.events)
                } catch (_: Exception) {
                    main.post {
                        if (job === current && !current.finished) {
                            if (service.isBinderAlive) fail(current, "unavailable") else death(current)
                        }
                    }
                } finally { current.input.close(); current.write.close() }
            }
        } catch (_: Exception) {
            if (service.isBinderAlive) fail(current, "unavailable") else death(current)
        }
    }

    private fun disconnected(current: Job, completeAtDisconnect: Boolean) {
        if (job !== current) return
        lastDeath = "disconnected(alive=${current.binder?.isBinderAlive})"
        current.disconnected = true
        releaseBinding(current)
        if (!completeAtDisconnect) fail(current, "worker_died")
        // The reader may already have consumed valid done+EOF but its validation is queued.
        // Do not discard it or close its descriptor until that completion is processed.
        if (current.binder?.isBinderAlive == false) death(current, completeAtDisconnect)
        else grace(current)
    }

    private fun death(current: Job, completeAtDeath: Boolean = current.transportComplete) {
        if (job !== current || current.dead) return
        lastDeath = "binderDeath"
        current.dead = true
        // Transport completion is published by the reader before its main-thread delivery.
        // Death may overtake that delivery, but may not precede the complete transport.
        if (!completeAtDeath) current.diedBeforeTransport = true
        if (current.readerFailure == "protocol" && !current.sawOutput && current.pendingFailure == "protocol") {
            current.pendingFailure = "worker_died"
        }
        releaseBinding(current)
        // Do not close read until the dedicated reader posts its result; it may
        // already have consumed valid done+EOF before this death notification.
        if (current.readerDone || current.finished) complete(current)
        else grace(current)
    }

    private fun releaseBinding(current: Job) {
        if (current.bound) { binding.unbind(current.connection); current.bound = false }
    }

    internal fun extract(args: Map<*, *>?, result: MethodChannel.Result) {
        if (unavailable) { result.success(failed("unavailable")); return }
        if (job != null) { result.success(failed("busy")); return }
        val id = args?.get("requestId") as? String
        val path = args?.get("snapshotPath") as? String
        val maxInput = args?.get("maxInputBytes") as? Int
        val maxPages = args?.get("maxPages") as? Int
        val maxText = args?.get("maxTextBytes") as? Int
        val maxDecompressed = args?.get("maxDecompressedBytes") as? Int
        if (id == null || !isUuid(id) || path == null || maxInput == null || maxInput !in 1..10 * 1024 * 1024 ||
            maxPages == null || maxPages !in 1..500 || maxText == null || maxText !in 1..2 * 1024 * 1024 ||
            maxDecompressed == null || maxDecompressed !in 1..4 * 1024 * 1024
        ) {
            result.success(failed("invalid_input")); return
        }
        val root = File(context.filesDir, "pdf_staging")
        val supplied = File(path)
        val file = try { supplied.canonicalFile } catch (_: IOException) { null }
        if (file == null || file.parentFile != root.canonicalFile || !file.isFile || file.length() <= 0 ||
            file.length() > maxInput
        ) {
            result.success(failed("invalid_input")); return
        }
        val prior = try { Os.stat(file.path) } catch (_: Exception) {
            result.success(failed("invalid_input")); return
        }
        val input = try { open(file) } catch (_: Exception) { result.success(failed("unavailable")); return }
        val valid = try {
            val opened = Os.fstat(input.fileDescriptor)
            OsConstants.S_ISREG(opened.st_mode) && opened.st_size in 1..maxInput.toLong() &&
                opened.st_dev == prior.st_dev && opened.st_ino == prior.st_ino
        } catch (_: Exception) { false }
        if (!valid) { input.close(); result.success(failed("invalid_input")); return }
        val pipe = try { ParcelFileDescriptor.createPipe() }
        catch (_: Exception) { input.close(); result.success(failed("unavailable")); return }
        val current = Job(id, generation.incrementAndGet(), result, maxInput, maxPages, maxText,
            maxDecompressed, input, pipe[0], pipe[1], clock.now() + 60_000L)
        job = current
        val deadline = current.deadline
        scheduler.schedule({ main.post {
            if (job === current && clock.now() >= deadline && !current.validated) fail(current, "timeout")
        } }, 60, TimeUnit.SECONDS)
        current.bound = binding.bind(Intent(context, PdfWorkerService::class.java)
            .also { addPdfWorkerDebugFault(it) }, current.connection)
        if (!current.bound) fail(current, "unavailable")
        else reader.execute {
            val outcome = try { PdfWorkerProtocol.read(object : java.io.FilterInputStream(FileInputStream(current.read.fileDescriptor)) {
                override fun read(): Int = super.read().also { if (it >= 0) current.sawOutput = true }
                override fun read(bytes: ByteArray, offset: Int, length: Int): Int =
                    super.read(bytes, offset, length).also { if (it > 0) current.sawOutput = true }
            }, id, maxPages, maxText) } catch (f: PdfWorkerProtocol.Failure) {
                main.post { if (job === current) {
                    current.readerDone = true
                    current.readerFailure = f.code
                    if (!current.finished) fail(current, if (current.dead && f.code == "protocol" && !current.sawOutput) "worker_died" else f.code)
                    if (current.dead) complete(current)
                } }; return@execute
            } catch (_: Exception) {
                main.post { if (job === current) { current.readerDone = true; if (!current.finished) fail(current, if (current.dead) "worker_died" else "protocol"); if (current.dead) complete(current) } }; return@execute
            }
            current.transportComplete = true
            main.post {
                if (job === current) {
                    current.readerDone = true
                    if (!current.finished && clock.now() >= current.deadline) fail(current, "timeout")
                    if (!current.finished && current.diedBeforeTransport) fail(current, "worker_died")
                    if (current.finished && current.dead) { complete(current); return@post }
                    if (!current.finished) {
                        current.validated = true
                        current.pending = outcome
                        grace(current)
                        if (current.dead) complete(current)
                        else if (current.disconnected) grace(current)
                        else current.control?.let { control ->
                            controlExecutor.execute {
                                try { control.acknowledge(id, current.generation, current.events) }
                                catch (_: Exception) { main.post { if (job === current && !current.dead) fail(current, "unavailable") } }
                            }
                        }
                    }
                }
            }
        }
    }

    internal fun cancel(id: String): Boolean {
        val current = job ?: return false
        if (current.id != id || current.validated || current.finished) return false
        fail(current, "cancelled")
        return true
    }

    private fun fail(current: Job, code: String) {
        if (job !== current || current.finished) return
        current.pendingFailure = code
        current.finished = true
        current.read.close()
        if (current.transactionQueued) current.control?.let { control ->
            controlExecutor.execute {
                try { control.cancel(current.id, current.generation, current.events) }
                catch (_: Exception) { /* grace will mark unavailable if death cannot be confirmed */ }
            }
        }
        if (current.binder == null) {
            // No start was sent. Release the idle binding; no accepted worker session
            // exists to kill or wait for, and a late callback may never start it.
            releaseBinding(current)
            current.dead = true
            complete(current)
        } else grace(current)
    }

    private fun grace(current: Job) {
        if (current.graceScheduled) return
        current.graceScheduled = true
        scheduler.schedule({ main.post {
            if (job === current) {
                if (!current.dead) {
                    unavailable = true
                    current.pendingFailure = "unavailable"
                } else if (!current.readerDone && !current.finished) {
                    current.pendingFailure = "worker_died"
                    current.finished = true
                }
                if (current.dead || unavailable) complete(current)
            }
        } }, 5, TimeUnit.SECONDS)
    }

    private fun complete(current: Job) {
        if (job !== current) return
        if (!current.dead && !unavailable) { grace(current); return }
        current.read.close()
        if (!current.transactionQueued) { current.input.close(); current.write.close() }
        releaseBinding(current)
        current.deathRecipient?.let { current.binder?.unlinkToDeath(it, 0) }
        job = null
        val outcome = current.pendingFailure?.let(::failed) ?: current.pending ?: failed("worker_died")
        current.pending = null
        current.result.success(outcome)
    }

    private fun failed(code: String): Map<String, String> = mapOf("kind" to "failed", "code" to code)
    private fun isUuid(id: String) = id.length == 36 && id.withIndex().all { (i, c) ->
        if (i in listOf(8, 13, 18, 23)) c == '-' else c in '0'..'9' || c in 'a'..'f' || c in 'A'..'F'
    }
}
