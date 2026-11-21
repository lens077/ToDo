package com.sumery.virgo

import android.content.res.Configuration
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowInsetsController

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
  }

  override fun onResume() {
    super.onResume()
    applySystemBarTheme(resources.configuration)
  }

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    // manifest 声明了 uiMode，切深浅色不重建 activity，这里补一次着色。
    applySystemBarTheme(newConfig)
  }

  /**
   * 系统栏跟随应用主题：内容铺到系统栏后面（WebView 里由 safe-area-inset
   * 兜底排版），状态栏/导航栏透明，应用的背景色直接透出来，深浅色下都和
   * 界面同色；图标明暗随系统深浅色切换。并关掉 3 键导航栏的系统对比度灰
   * 底——不关的话暗色主题底部会压着一条浅灰条（横屏时出现在侧边）。
   *
   * 必须在视图树就绪后调用（onResume / onConfigurationChanged）：
   * onCreate 阶段 wry 还没 setContentView，DecorView 为 null，
   * getInsetsController() 在部分机型（vivo / Android 17）直接抛 NPE。
   */
  private fun applySystemBarTheme(config: Configuration) {
    window.statusBarColor = Color.TRANSPARENT
    @Suppress("DEPRECATION")
    window.navigationBarColor = Color.TRANSPARENT
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      window.isNavigationBarContrastEnforced = false
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      window.setDecorFitsSystemWindows(false)
      if (window.peekDecorView() == null) return
      window.insetsController?.let { controller ->
        val light =
          WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
            WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
        controller.setSystemBarsAppearance(
          if ((config.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES) 0 else light,
          light,
        )
      }
    } else {
      // 旧系统不强铺 edge-to-edge，只把图标明暗对齐到当前深浅色。
      @Suppress("DEPRECATION")
      val lightFlags = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
      val decor = window.peekDecorView() ?: return
      @Suppress("DEPRECATION")
      decor.systemUiVisibility = if (
        (config.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
      ) {
        decor.systemUiVisibility and lightFlags.inv()
      } else {
        decor.systemUiVisibility or lightFlags
      }
    }
  }
}
