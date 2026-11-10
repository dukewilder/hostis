package black.hostis.pyrate

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The node's hold key: everything it keeps on disk is sealed and named under it. The key is
 * kept wrapped by the phone's own secure hardware, which never hands out the wrapping key, so
 * a copy of the phone's storage taken off the chip is noise. The node gets it at start, as hex.
 * If the hardware will not help, the node keeps a key beside its data instead.
 */
object Hold {
    private const val ALIAS = "pyrate-hold"
    @Volatile private var cached: String? = null

    private fun wrapKey(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        return (ks.getKey(ALIAS, null) as? SecretKey) ?: KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(256)
                    .build()
            )
            generateKey()
        }
    }

    /**
     * The Android side's own few notes (the bells, the wake-up station, the last station
     * tuned) are sealed under the same hardware key, so the phone's storage says nothing of
     * what its owner listens to. A note written before this, in the clear, is read once and
     * sealed when next written.
     */
    fun getS(ctx: Context, key: String): String? {
        val v = ctx.getSharedPreferences("pyrate", Context.MODE_PRIVATE).getString(key, null) ?: return null
        if (!v.startsWith("s1:")) return v
        return try {
            val b = android.util.Base64.decode(v.substring(3), android.util.Base64.NO_WRAP)
            String(Cipher.getInstance("AES/GCM/NoPadding").run {
                init(Cipher.DECRYPT_MODE, wrapKey(), GCMParameterSpec(128, b, 0, 12))
                doFinal(b, 12, b.size - 12)
            })
        } catch (e: Exception) {
            null
        }
    }

    fun putS(ctx: Context, key: String, value: String?) {
        val e = ctx.getSharedPreferences("pyrate", Context.MODE_PRIVATE).edit()
        if (value == null) {
            e.remove(key).apply()
            return
        }
        val sealed = try {
            val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, wrapKey()) }
            "s1:" + android.util.Base64.encodeToString(c.iv + c.doFinal(value.toByteArray()), android.util.Base64.NO_WRAP)
        } catch (x: Exception) {
            // Without the hardware the note is kept as the node keeps its own: beside it.
            value
        }
        e.putString(key, sealed).apply()
    }

    /**
     * Scuttle: the hardware key that wraps the hold key is destroyed, and with it everything
     * the radio kept on this phone becomes noise at once, before a single file is deleted.
     * A note is left so the next start clears the noise away and begins fresh.
     */
    fun scuttle(ctx: Context) {
        synchronized(this) {
            try { KeyStore.getInstance("AndroidKeyStore").apply { load(null) }.deleteEntry(ALIAS) } catch (e: Exception) {}
            File(ctx.filesDir, "hold.wrapped").delete()
            cached = null
            ctx.getSharedPreferences("pyrate", Context.MODE_PRIVATE).edit().clear().commit()
            File(ctx.filesDir, "scuttled").writeText("1")
        }
    }

    /** After a scuttle: the node's folder, now noise, is cleared before anything opens it. */
    fun clearIfScuttled(ctx: Context) {
        val mark = File(ctx.filesDir, "scuttled")
        if (!mark.exists()) return
        File(ctx.filesDir, "node").deleteRecursively()
        ctx.cacheDir.listFiles()?.forEach { it.deleteRecursively() }
        mark.delete()
    }

    fun key(ctx: Context): String? {
        cached?.let { return it }
        synchronized(this) {
            cached?.let { return it }
            return try {
                val wrap = wrapKey()
                val f = File(ctx.filesDir, "hold.wrapped")
                val raw = if (f.exists()) {
                    val b = f.readBytes()
                    Cipher.getInstance("AES/GCM/NoPadding").run {
                        init(Cipher.DECRYPT_MODE, wrap, GCMParameterSpec(128, b, 0, 12))
                        doFinal(b, 12, b.size - 12)
                    }
                } else {
                    val k = ByteArray(32).also { SecureRandom().nextBytes(it) }
                    val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, wrap) }
                    val tmp = File(ctx.filesDir, "hold.wrapped.part")
                    tmp.writeBytes(c.iv + c.doFinal(k))
                    tmp.renameTo(f)
                    k
                }
                raw.joinToString("") { "%02x".format(it) }.also { cached = it }
            } catch (e: Exception) {
                null
            }
        }
    }
}
