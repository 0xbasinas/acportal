// Top-level build file where you can add configuration options common to all sub-projects/modules.
plugins {
  id("com.google.devtools.ksp") version "2.3.12" apply false
  kotlin("jvm") version "2.3.20" apply false
  alias(libs.plugins.android.application) apply false
  alias(libs.plugins.compose.compiler) apply false
  alias(libs.plugins.kotlin.serialization) apply false
}
