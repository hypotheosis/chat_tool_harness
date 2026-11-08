package com.example.chat_tool_harness;

import android.os.ParcelFileDescriptor;
import com.example.chat_tool_harness.IPdfWorkerEvents;

interface IPdfWorker {
    void start(String requestId, long generation, in ParcelFileDescriptor inputFd,
        in ParcelFileDescriptor outputFd, int maxInputBytes, int maxPages,
        int maxTextBytes, int maxDecompressedBytes, IPdfWorkerEvents events);
    void acknowledge(String requestId, long generation, IPdfWorkerEvents events);
    void cancel(String requestId, long generation, IPdfWorkerEvents events);
}
