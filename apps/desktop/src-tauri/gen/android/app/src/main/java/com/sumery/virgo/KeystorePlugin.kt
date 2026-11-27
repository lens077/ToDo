package com.sumery.virgo

import android.app.Activity
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class SealArgs {
  var plain: String = ""
}

@InvokeArg
class UnsealArgs {
  var value: String = ""
}

/**
 * 设备私钥的 Android 侧保护：一把不可导出的 Keystore AES-256-GCM 密钥，
 * 只用来把 Rust 生成的 PKCS#8 私钥加密后落盘。签名仍在 Rust 里做——Keystore
 * 负责「at rest」，Rust 负责协议，两边都简单。
 *
 * Keystore 密钥随应用卸载一起删除；换机解绑后本地密文也会被清掉。
 */
class KeystorePlugin(activity: Activity) : Plugin(activity) {
  companion object {
    private const val KEY_ALIAS = "virgo.auth.device-key"
    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val PROVIDER = "AndroidKeyStore"
    private const val TAG_LENGTH = 128
    private const val FLAGS = Base64.NO_WRAP or Base64.URL_SAFE
  }

  private fun keyStore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

  private fun secretKey(): SecretKey {
    val store = keyStore()
    val existing = store.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry
    if (existing != null) return existing.secretKey
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
    generator.init(
      KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setKeySize(256)
        .build(),
    )
    return generator.generateKey()
  }

  /** 加密：返回 `iv.密文`（两段 base64url，无填充）。 */
  @Command
  fun seal(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SealArgs::class.java)
      val cipher = Cipher.getInstance(TRANSFORMATION)
      cipher.init(Cipher.ENCRYPT_MODE, secretKey())
      val ciphertext = cipher.doFinal(args.plain.toByteArray(Charsets.UTF_8))
      val value = Base64.encodeToString(cipher.iv, FLAGS) + "." + Base64.encodeToString(ciphertext, FLAGS)
      val response = JSObject()
      response.put("value", value)
      invoke.resolve(response)
    } catch (error: Exception) {
      invoke.reject(error.message ?: "seal failed", error)
    }
  }

  /** 解密 `iv.密文`；格式损坏或密钥不匹配都返回错误。 */
  @Command
  fun unseal(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(UnsealArgs::class.java)
      val parts = args.value.split(".", limit = 2)
      if (parts.size != 2) throw IllegalArgumentException("sealed value malformed")
      val iv = Base64.decode(parts[0], FLAGS)
      val ciphertext = Base64.decode(parts[1], FLAGS)
      val cipher = Cipher.getInstance(TRANSFORMATION)
      cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(TAG_LENGTH, iv))
      val plain = String(cipher.doFinal(ciphertext), Charsets.UTF_8)
      val response = JSObject()
      response.put("value", plain)
      invoke.resolve(response)
    } catch (error: Exception) {
      invoke.reject(error.message ?: "unseal failed", error)
    }
  }
}
