/*
 * Text the app keeps sealed (decision 0098's servers' sign-ins, decision 0091's listening services' on this device):
 * encrypted with a key Android's keystore holds, which never leaves it, kept as base64 of the IV and the ciphertext.
 * One key per store. What cannot be read again (the key gone with a reset of the device's lock) reads as nothing.
 */
package app.cratarr

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class Sealed(private val alias: String, private val store: String) {
    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(alias, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    /** What is kept, or null where nothing is (or it cannot be read). */
    fun read(context: Context): String? {
        val kept = Prefs.shared(context).getString(store, null) ?: return null
        return try {
            val raw = Base64.decode(kept, Base64.NO_WRAP)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, raw, 0, 12))
            String(cipher.doFinal(raw, 12, raw.size - 12), Charsets.UTF_8)
        } catch (e: Exception) {
            Log.w("Sealed", "$store cannot be read: ${e.javaClass.simpleName}")
            null
        }
    }

    fun write(context: Context, text: String) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val out = cipher.iv + cipher.doFinal(text.toByteArray(Charsets.UTF_8))
        Prefs.shared(context).edit().putString(store, Base64.encodeToString(out, Base64.NO_WRAP)).apply()
    }
}
