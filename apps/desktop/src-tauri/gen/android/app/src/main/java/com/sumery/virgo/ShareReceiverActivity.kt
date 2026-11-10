package com.sumery.virgo

import android.app.Activity
import android.content.Intent
import android.graphics.Typeface
import android.os.Bundle
import android.util.Log
import android.view.Gravity
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.Toast
import java.net.HttpURLConnection
import java.net.URL

/**
 * 系统分享接收：任何 app 分享文本时提取第一个 http(s) 链接，直接 POST 到
 * todo-sync 服务器（不进 WebView 主界面）。Token 存 SharedPreferences；
 * 首次使用在本 activity 的极简表单里填写一次。
 *
 * 服务器地址与 token 的存储键与桌面端「同步」面板的约定保持同源：桌面端
 * 走 localStorage，移动端没有本地事实源（决策③），所以这里只存服务端凭据。
 */
class ShareReceiverActivity : Activity() {

    companion object {
        const val TAG = "ShareReceiver"
        const val PREFS = "virgo-sync"
        const val KEY_BASE = "base"
        const val KEY_TOKEN = "token"
        const val DEFAULT_BASE = "https://todo-sync.apikv.com"
    }

    private var pendingUrl: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pendingUrl = extractUrl(intent)
        if (pendingUrl == null) {
            toast("没有可保存的链接")
            finish()
            return
        }
        val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
        if (prefs.getString(KEY_TOKEN, "").isNullOrBlank()) {
            showTokenForm()
        } else {
            save()
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        pendingUrl = extractUrl(intent)
        if (pendingUrl == null) finish() else save()
    }

    /** 分享文本里抓第一个 http(s) 链接；没有就退回原文。 */
    private fun extractUrl(intent: Intent): String? {
        if (intent.action != Intent.ACTION_SEND) return null
        val text = intent.getStringExtra(Intent.EXTRA_TEXT) ?: return null
        return Regex("https?://\\S+").find(text)?.value?.trimEnd(',', '，', ')', '）')
            ?.takeIf { it.isNotBlank() }
    }

    private fun showTokenForm() {
        val padding = (16 * resources.displayMetrics.density).toInt()
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(padding, padding * 2, padding, padding)
        }
        val baseInput = EditText(this).apply {
            hint = "服务器（默认 $DEFAULT_BASE）"
            setText(DEFAULT_BASE)
        }
        val tokenInput = EditText(this).apply { hint = "Token（todo-sync-token）" }
        val saveButton = Button(this).apply {
            text = "保存 Token 并收藏链接"
            setOnClickListener {
                val base = baseInput.text.toString().trim().trimEnd('/')
                    .ifBlank { DEFAULT_BASE }
                val token = tokenInput.text.toString().trim()
                if (token.isBlank()) {
                    toast("Token 不能为空")
                    return@setOnClickListener
                }
                getSharedPreferences(PREFS, MODE_PRIVATE)
                    .edit()
                    .putString(KEY_BASE, base)
                    .putString(KEY_TOKEN, token)
                    .apply()
                root.removeAllViews()
                save()
            }
        }
        root.addView(baseInput)
        root.addView(tokenInput)
        root.addView(saveButton)
        setContentView(root)
    }

    /** POST {base}/links；网络在子线程，结果回主线程提示。 */
    private fun save() {
        val url = pendingUrl ?: return
        val prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
        val base = prefs.getString(KEY_BASE, DEFAULT_BASE) ?: DEFAULT_BASE
        val token = prefs.getString(KEY_TOKEN, "") ?: ""
        val body = """{"url":${jsonString(url)}}"""
        Thread {
            var ok = false
            var detail = ""
            try {
                val connection = URL("$base/links").openConnection() as HttpURLConnection
                connection.requestMethod = "POST"
                connection.setRequestProperty("Authorization", "Bearer $token")
                connection.setRequestProperty("Content-Type", "application/json")
                connection.doOutput = true
                connection.connectTimeout = 10_000
                connection.readTimeout = 15_000
                connection.outputStream.use { it.write(body.toByteArray()) }
                ok = connection.responseCode in 200..299
                if (!ok) {
                    detail = try {
                        connection.errorStream?.bufferedReader()?.readText()?.take(160) ?: ""
                    } catch (_: Exception) {
                        ""
                    }
                }
            } catch (error: Exception) {
                detail = error.message ?: error.toString()
                Log.w(TAG, "share save failed", error)
            }
            runOnUiThread {
                toast(if (ok) "已保存到 Virgo：$url" else "保存失败：${detail.take(120)}")
                finish()
            }
        }.start()
    }

    private fun jsonString(value: String): String {
        val escaped = buildString {
            for (char in value) {
                when (char) {
                    '"' -> append("\\\"")
                    '\\' -> append("\\\\")
                    '\n' -> append("\\n")
                    '\r' -> append("\\r")
                    '\t' -> append("\\t")
                    else -> if (char < ' ') append("\\u%04x".format(char.code)) else append(char)
                }
            }
        }
        return "\"$escaped\""
    }

    private fun toast(message: String) {
        Toast.makeText(this, message, Toast.LENGTH_LONG).show()
    }
}
