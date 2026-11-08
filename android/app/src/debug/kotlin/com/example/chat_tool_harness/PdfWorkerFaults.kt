package com.example.chat_tool_harness

import android.content.Intent

/** Debug-only fault injection; release sources contain no commands. */
private object PdfWorkerFaults {
    @Volatile var next: String? = null
    @Volatile var current: String? = null
}

internal fun isPdfWorkerTestProcess(name: String?, packageName: String) = name == "$packageName:pdf_test_client"

internal fun setPdfWorkerDebugFault(mode: String) { PdfWorkerFaults.next = mode }
internal fun addPdfWorkerDebugFault(intent: Intent) {
    intent.putExtra("pdfFault", PdfWorkerFaults.next)
    PdfWorkerFaults.next = null
}
internal fun capturePdfWorkerDebugFault(intent: Intent?) {
    PdfWorkerFaults.current = intent?.getStringExtra("pdfFault")
}
internal fun applyPdfWorkerDebugFault(service: PdfWorkerService): Boolean =
    when (PdfWorkerFaults.current) {
        "abort" -> { workerSelfTerminate(service.application as AgentApplication); true }
        "partial" -> { service.debugWritePartial(); true }
        "withhold" -> true
        else -> false
    }
