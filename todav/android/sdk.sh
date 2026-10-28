#!/usr/bin/env bash
# The Android SDK and emulator for building and trying the app. Arch Linux; safe to re-run.
#
#     ./sdk.sh setup     # check the tools, install the SDK packages, create the `apps` AVD
#     ./sdk.sh start     # start the emulator unless it runs, wait until it has booted
#     ./sdk.sh stop      # stop it
#     ./sdk.sh wipe      # stop it and start again with its data wiped
#
# One SDK for every project: ~/Android/Sdk (or $ANDROID_HOME), as Android Studio and Noogle's
# scripts/setup-avd.sh use it. It gets exactly the versions gradle.properties names, so the build
# never downloads copies of its own. The emulator is the one for developing apps (Todav, later
# Notav too), on the stock Google Play image, which Noogle's AVDs use too: each AVD links to the
# shared image from an own copy (<avd>.avd/system-image) and changes only its own files there,
# Noogle its ramdisk (Magisk), this one its fold states. Nothing system-wide is installed: missing
# tools are named with the command that installs them.
#
# adb: the SDK's platform-tools stay installed (the SDK and Gradle check for them), but their adb
# is a link to android-tools' /usr/bin/adb. Two adb versions keep restarting the server and drop
# the emulator.
#
# The emulator is emulator-5580 (ANDROID_SERIAL), so it runs next to Noogle's (5554 and up); its
# log is ../../target/android/emulator.log.

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
prop() {
	sed -n "s/^todav\.$1=//p" "$here/gradle.properties"
}
platform="$(prop platform)"
image="$(prop image)"
packages=("platforms;$platform" "build-tools;$(prop buildTools)" "ndk;$(prop ndk)" platform-tools
	emulator "$image")
avd=apps
port=5580
sdk="${ANDROID_HOME:-$HOME/Android/Sdk}"
log="$here/../../target/android/emulator.log"

# JDK 21, the LTS the Android tools support; Arch's default Java may be another or none.
if [ -z "${JAVA_HOME:-}" ] && [ -d /usr/lib/jvm/java-21-openjdk ]; then
	export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
fi
export ANDROID_HOME="$sdk"
export ANDROID_SERIAL="emulator-$port"
export PATH="$sdk/cmdline-tools/latest/bin:$sdk/emulator:$PATH"

running() {
	pgrep -f -- "-avd $avd( |$)" > /dev/null
}

# The tools outside the SDK: Rust's Android targets are added here, the rest only named.
check_tools() {
	local missing=()
	command -v adb > /dev/null || missing+=(android-tools)
	command -v msgfmt > /dev/null || missing+=(gettext)
	command -v cargo-ndk > /dev/null || missing+=(cargo-ndk)
	command -v rustup > /dev/null || missing+=(rustup)
	[ -n "${JAVA_HOME:-}" ] || missing+=(jdk21-openjdk)
	command -v unzip > /dev/null || missing+=(unzip)
	if [ ${#missing[@]} -gt 0 ]; then
		echo "Missing: sudo pacman -S --needed ${missing[*]}"
		echo "(rustup replaces the rust package; then: rustup default stable)"
		exit 1
	fi
	if [ ! -w /dev/kvm ]; then
		echo "/dev/kvm is not writable: enable virtualization, or: sudo usermod -aG kvm $USER"
		exit 1
	fi
}

setup() {
	check_tools

	if [ ! -x "$sdk/cmdline-tools/latest/bin/sdkmanager" ]; then
		echo "Installing the command-line tools into $sdk..."
		local zip tmp
		zip="$(curl -fsSL https://dl.google.com/android/repository/repository2-3.xml \
			| grep -o 'commandlinetools-linux-[0-9]*_latest.zip' | sort -V | tail -n 1)"
		tmp="$(mktemp -d)"
		curl -fL --progress-bar "https://dl.google.com/android/repository/$zip" -o "$tmp/tools.zip"
		unzip -q "$tmp/tools.zip" -d "$tmp"
		mkdir -p "$sdk/cmdline-tools"
		rm -rf "$sdk/cmdline-tools/latest"
		mv "$tmp/cmdline-tools" "$sdk/cmdline-tools/latest"
		rm -rf "$tmp"
	fi

	# A platform-tools without its package.xml (as Noogle's script leaves it) counts as missing:
	# the SDK would install a second copy next to it.
	if [ ! -f "$sdk/platform-tools/package.xml" ]; then
		rm -rf "$sdk/platform-tools" "$sdk"/platform-tools-[0-9]*
	fi
	yes | sdkmanager --licenses > /dev/null 2>&1 || true
	echo "Installing ${packages[*]}..."
	sdkmanager --install "${packages[@]}"
	ln -sfn /usr/bin/adb "$sdk/platform-tools/adb"

	if ! emulator -list-avds | grep -qx "$avd"; then
		echo "Creating the $avd AVD..."
		# A foldable (`make fold` / `make unfold`): open it tries wide layouts, folded its cover
		# screen is a phone's. Google's Pixel 9 Pro Fold with the Honor Magic V5's screens (open
		# 2172×2352, 827×896 dp; cover 1060×2376, 404 dp wide); typing with the computer's keyboard.
		echo no | avdmanager create avd -n "$avd" -k "$image" -d pixel_9_pro_fold > /dev/null
		sed -i -e 's/^hw.keyboard=no$/hw.keyboard=yes/' \
			-e 's/^hw.lcd.width=.*/hw.lcd.width=2172/' -e 's/^hw.lcd.height=.*/hw.lcd.height=2352/' \
			-e 's/^hw.lcd.density=.*/hw.lcd.density=420/' \
			-e 's/^hw.displayRegion.0.1.width=.*/hw.displayRegion.0.1.width=1060/' \
			-e 's/^hw.displayRegion.0.1.height=.*/hw.displayRegion.0.1.height=2376/' \
			-e 's/^hw.sensor.hinge.areas=.*/hw.sensor.hinge.areas=1086-0-0-2352/' \
			"$HOME/.android/avd/$avd.avd/config.ini"
	fi
	own_image
	echo "Ready: $sdk, AVD $avd ($image)"
}

# The fold states the emulator copies from the image into /data on the first boot put the device
# to sleep when closed (Android's setting for it, "Continue using apps on fold", is off in the
# emulator), and a Google Play image is a user build, without `adb root` to change them there. So
# the AVD gets its own image: links to the shared one, its own copy of that file without "sleep".
own_image() {
	local own="$HOME/.android/avd/$avd.avd/system-image"
	local states=data/misc/pixel_9_pro_fold/devicestate/device_state_configuration.xml
	[ -d "$own" ] && return
	mkdir -p "$own"
	cp -as "$sdk/${image//;//}/." "$own"
	rm "$own/$states"
	sed /TRIGGER_SLEEP/d "$sdk/${image//;//}/$states" > "$own/$states"
	sed -i "s|^image.sysdir.1=.*|image.sysdir.1=$own/|" "$HOME/.android/avd/$avd.avd/config.ini"
}

# $@: more emulator options
start() {
	if ! running; then
		emulator -list-avds 2> /dev/null | grep -qx "$avd" || setup
		mkdir -p "$(dirname "$log")"
		echo "Starting the emulator (log: $log)..."
		# Its own session, so it outlives the task that started it.
		setsid nohup emulator -avd "$avd" -port "$port" -no-snapshot-save -no-metrics \
			-crash-report-mode never "$@" > "$log" 2>&1 &
		booted
		always_on
	fi
	booted
	echo "Booted: $ANDROID_SERIAL"
}

booted() {
	until [ "$(adb shell getprop sys.boot_completed 2> /dev/null | tr -d '\r')" = 1 ]; do
		running || { echo "The emulator exited, see $log"; exit 1; }
		sleep 2
	done
}

# Always on and unlocked, as a phone in use: no screen timeout, no lock screen (apps kept open on
# folding comes with the AVD's own image, `own_image`).
always_on() {
	adb shell settings put system screen_off_timeout 2147483647
	adb shell locksettings set-disabled true > /dev/null
}

stop() {
	running || return 0
	adb emu kill > /dev/null 2>&1 || true
	while running; do sleep 1; done
	echo "Stopped"
}

case "${1:-}" in
	setup) setup ;;
	start) start ;;
	stop) stop ;;
	wipe)
		stop
		start -wipe-data
		;;
	*)
		sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'
		exit 1
		;;
esac
