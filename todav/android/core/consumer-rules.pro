# JNA and the UniFFI bindings find their classes and fields by reflection.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class dev.aperte.todav.core.** { *; }
-dontwarn java.awt.**
