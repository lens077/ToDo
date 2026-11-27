import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 37
    namespace = "com.sumery.virgo"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.sumery.virgo"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}


// Tauri 的 deep-link 插件按行扫描 AndroidManifest.xml，把 `virgo://` 的
// intent-filter 插到**每一个** `</activity>` 前面。本应用除了 MainActivity 还有
// 一个 ShareReceiverActivity（系统分享），如果它也声明 `virgo://`，从浏览器
// 回跳登录时 Android 可能选中它（分享接收器只认 ACTION_SEND，会静默退出）。
// 这里在合并清单之前，把自动生成的那段从非 MainActivity 的 activity 里删掉。
// 分享接收器自身的 SEND 过滤器不受影响。
//
// 只捕获 File（可序列化）以保持 configuration cache 可用；不要捕获脚本级 lambda。
tasks.matching { it.name.startsWith("pre") && it.name.endsWith("Build") }.configureEach {
    val manifestFile = layout.projectDirectory.file("src/main/AndroidManifest.xml").asFile
    doFirst {
        if (manifestFile.isFile) {
            val marker = "<!-- DEEP LINK PLUGIN. AUTO-GENERATED. DO NOT REMOVE. -->"
            val activityBlock = Regex("<activity\\b[\\s\\S]*?</activity>")
            val generated = Regex("\\n[ \\t]*" + Regex.escape(marker) + "[\\s\\S]*?" + Regex.escape(marker))
            var changed = false
            val rewritten = activityBlock.replace(manifestFile.readText()) { match ->
                val activity = match.value
                if (activity.contains("MainActivity") || !activity.contains(marker)) {
                    activity
                } else {
                    changed = true
                    activity.replace(generated, "").replace(Regex("(\\n[ \\t]+)+\\n"), "\n")
                }
            }
            // 插件生成的块里每行都带缩进，空白行因此留下尾随空格；
            // `git diff --check` 会把它当错误。这里顺手 trim 每行行尾。
            val tidied = rewritten.lines().joinToString("\n") { it.trimEnd() }
            if (changed || tidied != manifestFile.readText()) manifestFile.writeText(tidied)
        }
    }
}

apply(from = file("tauri.build.gradle.kts"))
