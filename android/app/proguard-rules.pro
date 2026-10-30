# Keep file names and line numbers for stack traces
-keepattributes SourceFile,LineNumberTable

# Spike B: JNA and UniFFI's Kotlin find their classes and fields by name
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class app.cratarr.rules.** { *; }
-dontwarn java.awt.**
