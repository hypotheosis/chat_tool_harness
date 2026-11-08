package com.example.chat_tool_harness

import android.content.Intent
import android.content.pm.PackageManager
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID

class ArtifactDocumentsTest {
    @Test fun restrictedContentUriReadsOwnTextAndShareGrantsReadOnly() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.filesDir, "agent/artifacts").apply { mkdirs() }
        val file = File(root, "${UUID.randomUUID()}.txt")
        val unrelated = File(context.filesDir, "task10-${UUID.randomUUID()}.txt")
        try {
            file.writeText("# literal <script> é")
            unrelated.writeText("not exportable")
            val validated = ArtifactDocuments.file(context, file.path, "generated.md")
            val intent = ArtifactDocuments.shareIntent(context, validated, "generated.md")
            assertEquals(Intent.ACTION_SEND, intent.action)
            assertEquals("text/markdown", intent.type)
            assertEquals(Intent.FLAG_GRANT_READ_URI_PERMISSION, intent.flags)
            val uri = intent.clipData!!.getItemAt(0).uri
            assertEquals("content", uri.scheme)
            assertEquals("${context.packageName}.artifacts", uri.authority)
            assertEquals("# literal <script> é", context.contentResolver.openInputStream(uri)!!.bufferedReader().use { it.readText() })
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)!!.use {
                assertTrue(it.moveToFirst())
                assertEquals("generated.md", it.getString(0))
            }
            val provider = context.packageManager.resolveContentProvider(uri.authority!!, PackageManager.GET_META_DATA)!!
            assertFalse(provider.exported)
            assertTrue(provider.grantUriPermissions)
            try { FileProvider.getUriForFile(context, uri.authority!!, unrelated); fail("outside provider root") }
            catch (_: IllegalArgumentException) { }
            for (path in listOf(unrelated.path, File(root, "../agent.sqlite3").path)) {
                try { ArtifactDocuments.file(context, path, "x.txt"); fail("outside root") }
                catch (_: IllegalArgumentException) { }
            }
            val link = File(root, "${UUID.randomUUID()}.txt")
            try {
                android.system.Os.symlink(file.path, link.path)
                try { ArtifactDocuments.file(context, link.path, "x.txt"); fail("symlink file") }
                catch (_: IllegalArgumentException) { }
            } finally { link.delete() }
            val save = ArtifactDocuments.saveIntent("generated.md")
            assertEquals(Intent.ACTION_CREATE_DOCUMENT, save.action)
            assertTrue(save.hasCategory(Intent.CATEGORY_OPENABLE))
            assertEquals("generated.md", save.getStringExtra(Intent.EXTRA_TITLE))
            assertFalse(save.flags and Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION != 0)
        } finally { file.delete(); unrelated.delete() }
    }
}
