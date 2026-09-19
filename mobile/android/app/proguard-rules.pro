# kotlinx.serialization: keep generated serializers for the DTOs.
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.**
-keepclassmembers class **$$serializer { *; }
-keepclasseswithmembers class com.kubuno.docs.net.** {
    kotlinx.serialization.KSerializer serializer(...);
}
-keep,includedescriptorclasses class com.kubuno.docs.net.**$$serializer { *; }
-keepclassmembers class com.kubuno.docs.net.** {
    <fields>;
}

# The document body is free-form ProseMirror JSON parsed as JsonElement, so the
# polymorphic serializers of kotlinx-serialization-json must survive shrinking.
-keep class kotlinx.serialization.json.** { *; }

# Retrofit: keep the API interface and its Kotlin metadata (suspend signatures).
-keep,allowobfuscation interface com.kubuno.docs.net.DocsApi
-keepattributes Signature, Exceptions
