package com.example.chat_tool_harness

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.io.FileNotFoundException
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Ciphertext only; a lost key is never silently replaced while ciphertext exists. */
class CredentialStore(context: Context, private val alias: String = "agent.credentials.v1",
                      directory: File = File(context.noBackupFilesDir, "credentials")) {
    private val root = directory.apply { mkdirs() }
    private val keys = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    private fun file(reference: String): AtomicFile {
        require(UUID.fromString(reference).toString() == reference)
        return AtomicFile(File(root, reference))
    }
    private fun key(create: Boolean): SecretKey {
        (keys.getKey(alias, null) as? SecretKey)?.let { return it }
        check(create && root.listFiles().orEmpty().isEmpty())
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true).build())
            generateKey()
        }
    }
    @Synchronized fun put(reference: String, value: ByteArray) {
        val target = file(reference)
        require(value.isNotEmpty() && value.size <= 8192)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key(true))
        cipher.updateAAD(reference.toByteArray(Charsets.UTF_8))
        val encrypted = cipher.doFinal(value)
        val output = target.startWrite()
        try {
            output.write(cipher.iv)
            output.write(encrypted)
            target.finishWrite(output)
        } catch (error: Exception) {
            target.failWrite(output)
            throw error
        }
    }
    /** Null when nothing is stored under [reference]; decrypt and keystore errors throw. */
    @Synchronized fun resolve(reference: String): ByteArray? {
        val encrypted = try {
            file(reference).readFully()
        } catch (_: FileNotFoundException) {
            return null
        }
        check(encrypted.size in 29..8220)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(false), GCMParameterSpec(128, encrypted.copyOfRange(0, 12)))
        cipher.updateAAD(reference.toByteArray(Charsets.UTF_8))
        return cipher.doFinal(encrypted, 12, encrypted.size - 12)
    }
    /**
     * Scheme token (when the value starts with "<scheme> " followed by more text, the same
     * rule as Rust's `net::authorization_value`) and UTF-8 byte length; null if absent.
     */
    @Synchronized fun describe(reference: String): Map<String, Any?>? {
        val value = resolve(reference) ?: return null
        try {
            val text = String(value, Charsets.UTF_8)
            val space = text.indexOf(' ')
            val scheme = if (space > 0 && space < text.length - 1 && isScheme(text.substring(0, space))) {
                text.substring(0, space)
            } else {
                null
            }
            return mapOf("scheme" to scheme, "length" to value.size)
        } finally {
            value.fill(0)
        }
    }
    private fun isScheme(token: String): Boolean =
        token.first().let { it in 'A'..'Z' || it in 'a'..'z' } &&
            token.all { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it in "!#$%&'*+.^_`|~-" }
    @Synchronized fun delete(reference: String) {
        val target = file(reference)
        target.delete()
        check(!target.baseFile.exists())
    }
}
