package com.example.chat_tool_harness

import android.app.Service
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.Process
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Test-owned Binder client; never constructs an engine, and only terminates itself. */
open class PdfWorkerTestClientService : Service() {
    private val callback = object : IPdfWorkerEvents.Stub() {
        override fun started(requestId: String, generation: Long) = true
    }
    private val control = object : IPdfWorkerTestClient.Stub() {
        override fun mainBarrier(): Int {
            val done = CountDownLatch(1)
            Handler(Looper.getMainLooper()).post { done.countDown() }
            check(done.await(5, TimeUnit.SECONDS))
            return Process.myPid()
        }
        override fun chatInitialized(): Boolean = try {
            (application as AgentApplication).runtime; true
        } catch (_: UninitializedPropertyAccessException) { false }
        override fun wrongControls(worker: IPdfWorker, id: String, generation: Long, events: IPdfWorkerEvents) {
            worker.cancel(id, generation, events)
            worker.acknowledge(id, generation, events)
        }
        override fun start(worker: IPdfWorker, id: String, generation: Long, input: ParcelFileDescriptor, output: ParcelFileDescriptor) {
            try { worker.start(id, generation, input, output, 10 * 1024 * 1024, 500, 2 * 1024 * 1024, 4 * 1024 * 1024, callback) }
            finally { input.close(); output.close() }
        }
        override fun terminate() {
            check(processRole(application) == "pdf_test_client")
            Process.killProcess(Process.myPid())
        }
    }
    override fun onBind(intent: Intent?) = control
}

/** Main-looper barrier in the already-owned worker, not a second worker engine. */
class PdfWorkerBarrierService : PdfWorkerTestClientService()
