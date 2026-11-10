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

    fun key(ctx: Context): String? {
        cached?.let { return it }
        synchronized(this) {
            cached?.let { return it }
            return try {
                val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
                val wrap = (ks.getKey(ALIAS, null) as? SecretKey) ?: KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
                    init(
                        KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                            .setKeySize(256)
                            .build()
                    )
                    generateKey()
                }
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
