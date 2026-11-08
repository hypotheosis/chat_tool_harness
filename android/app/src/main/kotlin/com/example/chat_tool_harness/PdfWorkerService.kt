package com.example.chat_tool_harness

import android.app.Service
import android.content.Intent
import android.os.Binder
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.Process
import java.util.concurrent.atomic.AtomicBoolean
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.dart.DartExecutor
import io.flutter.plugin.common.MethodChannel

/** One accepted session per disposable worker process. No chat runtime is constructed here. */
class PdfWorkerService : Service() {
    private val main = Handler(Looper.getMainLooper())
    @Volatile private var session: Session? = null
    private val accepted = AtomicBoolean(false)
    private var engine: FlutterEngine? = null

    private fun validPdfRequestId(id: String) = id.length == 36 &&
        id.withIndex().all { (i, c) ->
            if (i in listOf(8, 13, 18, 23)) c == '-'
            else c in '0'..'9' || c in 'a'..'f' || c in 'A'..'F'
        }

    private data class Session(
        val id: String, val generation: Long, val input: ParcelFileDescriptor,
        val output: ParcelFileDescriptor, val limits: IntArray,
        val callingUid: Int, val callingPid: Int, val callback: IBinder,
    )

    private val control = object : IPdfWorker.Stub() {
        override fun start(
            requestId: String, generation: Long, inputFd: ParcelFileDescriptor,
            outputFd: ParcelFileDescriptor, maxInputBytes: Int, maxPages: Int,
            maxTextBytes: Int, maxDecompressedBytes: Int, events: IPdfWorkerEvents,
        ) {
            if (processRole(application as AgentApplication) != "pdf_worker" ||
                Binder.getCallingUid() != Process.myUid() || generation < 0 ||
                !validPdfRequestId(requestId) || maxInputBytes !in 1..10 * 1024 * 1024 ||
                maxPages !in 1..500 || maxTextBytes !in 1..2 * 1024 * 1024 ||
                maxDecompressedBytes !in 1..4 * 1024 * 1024 ||
                !accepted.compareAndSet(false, true)
            ) {
                inputFd.close(); outputFd.close(); return
            }
            val job = Session(requestId, generation, inputFd, outputFd,
                intArrayOf(maxInputBytes, maxPages, maxTextBytes, maxDecompressedBytes),
                Binder.getCallingUid(), Binder.getCallingPid(), events.asBinder())
            session = job
            try { job.callback.linkToDeath({ main.post { end(job) } }, 0) }
            catch (_: Exception) { end(job); return }
            // Independent of engine startup, parsing, and Binder control responsiveness.
            Thread {
                try { Thread.sleep(60_000) } catch (_: InterruptedException) { return@Thread }
                if (session === job) workerSelfTerminate(application as AgentApplication)
            }.apply { isDaemon = true; name = "pdf-worker-watchdog" }.start()
            val started = try { events.started(requestId, generation) } catch (_: Exception) { false }
            if (!started) {
                end(job)
                return
            }
            if (applyPdfWorkerDebugFault(this@PdfWorkerService)) return
            main.post {
                if (session !== job) return@post
                val workerEngine = FlutterEngine(this@PdfWorkerService, null, false)
                engine = workerEngine
                PdfWorkerChannels(workerEngine, this@PdfWorkerService)
                workerEngine.dartExecutor.executeDartEntrypoint(
                    DartExecutor.DartEntrypoint(
                        io.flutter.FlutterInjector.instance().flutterLoader().findAppBundlePath(),
                        "package:chat_tool_harness/pdf_worker.dart",
                        "pdfWorkerMain",
                    )
                )
            }
        }

        override fun acknowledge(requestId: String, generation: Long, events: IPdfWorkerEvents) =
            controlEnd(requestId, generation, events)
        override fun cancel(requestId: String, generation: Long, events: IPdfWorkerEvents) =
            controlEnd(requestId, generation, events)

        private fun controlEnd(requestId: String, generation: Long, events: IPdfWorkerEvents) {
            val job = session ?: return
            if (job.callingUid == Binder.getCallingUid() &&
                job.callingPid == Binder.getCallingPid() && job.callback == events.asBinder() &&
                job.id == requestId && job.generation == generation) {
                main.post { end(job) }
            }
        }
    }

    override fun onBind(intent: Intent?): IBinder {
        capturePdfWorkerDebugFault(intent)
        return control
    }

    internal fun debugWritePartial() {
        val job = session ?: return
        job.output.fileDescriptor.let { java.io.FileOutputStream(it).write(byteArrayOf(4, 0)) }
        job.output.close()
        main.post { end(job) }
    }

    internal fun onEngineReady() {
        val job = session ?: return
        val args = mapOf(
            "requestId" to job.id,
            "inputFd" to job.input.fd,
            "outputFd" to job.output.fd,
            "maxInputBytes" to job.limits[0],
            "maxPages" to job.limits[1],
            "maxTextBytes" to job.limits[2],
            "maxDecompressedBytes" to job.limits[3],
        )
        MethodChannel(engine!!.dartExecutor.binaryMessenger, "chat_tool_harness/pdf_worker_engine")
            .invokeMethod("extract", args)
    }

    internal fun onEngineFinished() {
        // FRB has returned: its duplicated output is closed. Close the transferred write
        // descriptor to deliver EOF; keep the process alive until acknowledge/cancel.
        session?.output?.close()
        session?.input?.close()
    }

    private fun end(job: Session) {
        if (session !== job) return
        session = null
        job.input.close()
        job.output.close()
        workerSelfTerminate(application as AgentApplication)
    }

    override fun onDestroy() {
        session?.let(::end)
        super.onDestroy()
    }
}
