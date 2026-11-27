package com.sumery.virgo

import android.app.Activity
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class ReadUriArgs {
  var uri: String = ""
}

/**
 * SAF 选中的文件在 Android 上是以 `content://…` 授权给应用的，Rust 侧没有
 * ContentResolver，按路径 open 会报 `No such file or directory`。这里把字节
 * 读出来（连同显示名）交给 Rust，由 Rust 写到应用缓存再走既有导入管线。
 */
class UriPlugin(private val host: Activity) : Plugin(host) {
  @Command
  fun read(invoke: Invoke) {
    val args = invoke.parseArgs(ReadUriArgs::class.java)
    try {
      val uri = Uri.parse(args.uri)
      val name = displayName(uri)
      val bytes = host.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        ?: throw java.io.IOException("无法打开所选文件")
      val out = JSObject()
      out.put("data", Base64.encodeToString(bytes, Base64.NO_WRAP))
      out.put("name", name)
      invoke.resolve(out)
    } catch (error: Exception) {
      invoke.reject(error.message ?: "读取所选文件失败")
    }
  }

  private fun displayName(uri: Uri): String {
    host.contentResolver
      .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
      ?.use { cursor ->
        if (cursor.moveToFirst()) return cursor.getString(0) ?: ""
      }
    return ""
  }
}
