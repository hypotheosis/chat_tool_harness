package com.example.chat_tool_harness

import android.content.Intent

internal fun isPdfWorkerTestProcess(name: String?, packageName: String) = false

internal fun addPdfWorkerDebugFault(intent: Intent) = Unit
internal fun capturePdfWorkerDebugFault(intent: Intent?) = Unit
internal fun applyPdfWorkerDebugFault(service: PdfWorkerService): Boolean = false
