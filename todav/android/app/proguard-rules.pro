# Glance makes a widget's action callbacks (the calendar widgets' arrows) by reflection.
-keep class * implements androidx.glance.appwidget.action.ActionCallback { <init>(); }
