# The Android app: build it, and run it on the emulator that sdk.sh sets up (`make setup` first).
# The SDK is in ~/Android/Sdk unless ANDROID_HOME says otherwise; Gradle runs on JDK 21.

export ANDROID_HOME ?= $(HOME)/Android/Sdk
export JAVA_HOME ?= $(wildcard /usr/lib/jvm/java-21-openjdk)
export ANDROID_SERIAL := emulator-5580

GRADLE = ./gradlew --console=plain
APP = dev.aperte.todav

.PHONY: setup emulator stop wipe fold unfold build release run logs uninstall

# SDK, NDK, emulator and the `apps` AVD in ~/Android/Sdk.
setup:
	./sdk.sh setup

emulator:
	./sdk.sh start

stop:
	./sdk.sh stop

# The emulator started again with its data wiped.
wipe:
	./sdk.sh wipe

# The foldable emulator closed, apps going on on its cover screen, or open.
fold:
	adb emu fold

unfold:
	adb emu unfold

# Debug APK for every ABI: app/build/outputs/apk/debug/.
build:
	$(GRADLE) assembleDebug

release:
	$(GRADLE) assembleRelease

# Built for the emulator's ABI only, installed and started.
run: emulator
	$(GRADLE) installDebug -Ptodav.abis=x86_64
	adb shell am start -n $(APP)/.MainActivity

# The app's log while it runs.
logs:
	adb logcat -v color --pid=$$(adb shell pidof -s $(APP))

uninstall:
	adb uninstall $(APP)
