plugins {
    id("com.android.application")
}


val webclxVersion = providers.gradleProperty("webclxVersion").orNull
    ?: error("Build with -PwebclxVersion=<Cargo package version>")
val webclxVersionCode = providers.gradleProperty("webclxVersionCode").orNull?.toIntOrNull()
    ?: error("Build with -PwebclxVersionCode=<positive integer>")

android {
    namespace = "com.webclx.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.webclx.app"
        minSdk = 24
        targetSdk = 35
        versionCode = webclxVersionCode
        versionName = webclxVersion
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.core:core:1.15.0")
}
