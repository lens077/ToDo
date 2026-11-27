# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# App-local Android 插件（auth.rs 通过 register_android_plugin + run_mobile_plugin
# 反射调用）在 release 构建里会被 R8 改名/裁剪：方法名一旦变成 a/b，Rust 发来的
# `seal` / `unseal` 就找不到命令（实测报 “No command seal found for plugin
# com.sumery.virgo.KeystorePlugin”）。这里显式保名，并保留运行时可见注解
# （@Command 是命令注册依据，@InvokeArg 是 Jackson 解析依据）。
-keep class com.sumery.virgo.KeystorePlugin { *; }
-keep class com.sumery.virgo.SealArgs { *; }
-keep class com.sumery.virgo.UnsealArgs { *; }
-keep class com.sumery.virgo.UriPlugin { *; }
-keep class com.sumery.virgo.ReadUriArgs { *; }
-keepattributes RuntimeVisibleAnnotations
