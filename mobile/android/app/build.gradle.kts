// Kubuno Docs — the native client for the office module's document function.
// A consumer app, like photos, mail and maps: it owns no account of its own, it
// reuses :core-account (shared system accounts, the com.kubuno authenticator)
// and borrows an access token from whichever app holds the session. :core-ui
// carries the shared design system.
//
// :core-viewer is deliberately NOT a dependency: its viewers cover images, PDF,
// text, audio and video, and `previewKind` classifies every format this app
// handles (DOCX, ODT, DOC — exports and imported sources alike) as UNSUPPORTED.
// It would only ever draw its "open with" fallback, which the system share sheet
// already does.
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.ksp)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.hilt)
}

android {
    namespace = "com.kubuno.docs"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.kubuno.docs.android"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    // Release signing is opt-in: pass -PkubunoKeystore=… (and the passwords) to
    // sign, otherwise the release APK is unsigned (what the CI publishes). EVERY
    // Kubuno app MUST be signed with the SAME certificate — the shared-account
    // model grants access by matching signature, so a differently signed build
    // is refused a borrowed token and every screen ends up at 401.
    val keystorePath = (findProperty("kubunoKeystore") as String?)?.takeIf { it.isNotBlank() }
    val keystoreFile = keystorePath?.let { rootProject.file(it) }
    signingConfigs {
        if (keystoreFile != null && keystoreFile.exists()) {
            create("release") {
                storeFile = keystoreFile
                storePassword = findProperty("kubunoKeystorePassword") as String?
                keyAlias = findProperty("kubunoKeyAlias") as String?
                keyPassword = findProperty("kubunoKeyPassword") as String?
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    implementation(libs.kubuno.core.api)
    implementation(libs.kubuno.core.account)
    implementation(libs.kubuno.core.ui)

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.appcompat)

    val composeBom = platform(libs.compose.bom)
    implementation(composeBom)
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    debugImplementation(libs.compose.ui.tooling)

    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.viewmodel.compose)

    implementation(libs.hilt.android)
    ksp(libs.hilt.compiler)
    implementation(libs.hilt.navigation.compose)

    implementation(libs.coroutines.android)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.retrofit)
    implementation(libs.retrofit.kotlinx.serialization)

    implementation(libs.coil.compose)
    implementation(libs.coil.network.okhttp)
    // The office editor encodes shapes and text boxes as SVG data URLs, and
    // documents may embed SVG images: without this decoder they all fail.
    implementation(libs.coil.svg)

    testImplementation(libs.junit)
    testImplementation(libs.coroutines.test)
}
