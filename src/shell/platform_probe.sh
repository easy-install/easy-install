#!/bin/sh
# Detect the Rust target triple of the host running this script.
#
# Used by `ei`'s remote installer: the script is shipped to the remote host
# and executed with `sh -c`, so it must stay POSIX and dependency free.
#
# Output (parsed by `RemotePlatform::parse`):
#   OS=Linux ARCH=aarch64 LIBC=musl ANDROID=no TARGET=aarch64-unknown-linux-musl
#
# Rules:
#   * Linux       uname -m, plus musl/glibc detection
#                 (/etc/os-release, ldd --version, musl loader paths).
#                 ARM prefers musl: glibc builds often fail on those systems.
#   * Android     uname -o, $ANDROID_ROOT or /system/build.prop
#   * macOS       uname -s/m -> *-apple-darwin
#   * Windows     MSYS/MINGW/CYGWIN -> *-pc-windows-msvc
#   * FreeBSD     x86_64-unknown-freebsd
#
# Run locally to check: sh platform_probe.sh

OS=$(uname -s 2>/dev/null || echo unknown)
ARCH=$(uname -m 2>/dev/null || echo unknown)
KERNEL=$(uname -o 2>/dev/null || echo unknown)

LIBC=gnu
if [ "$OS" = Linux ]; then
  if [ -f /etc/os-release ] && grep -qi alpine /etc/os-release 2>/dev/null; then
    LIBC=musl
  elif [ -f /lib/ld-musl-x86_64.so.1 ] || [ -f /lib/ld-musl-aarch64.so.1 ] \
    || [ -f /lib/ld-musl-armhf.so.1 ]; then
    LIBC=musl
  elif command -v ldd >/dev/null 2>&1 \
    && ldd --version 2>&1 | head -n 1 | grep -qi musl; then
    LIBC=musl
  fi
fi

ANDROID=no
if [ "$KERNEL" = Android ] || [ -n "${ANDROID_ROOT:-}" ] || [ -f /system/build.prop ]; then
  ANDROID=yes
fi

TARGET=
case "$OS" in
  Linux)
    if [ "$ANDROID" = yes ]; then
      case "$ARCH" in
        aarch64 | arm64) TARGET=aarch64-linux-android ;;
        armv7* | armv8l) TARGET=armv7-linux-androideabi ;;
        i686 | x86) TARGET=i686-linux-android ;;
        x86_64) TARGET=x86_64-linux-android ;;
      esac
    else
      case "$ARCH" in
        x86_64 | amd64)
          TARGET="x86_64-unknown-linux-$LIBC"
          ;;
        aarch64 | arm64)
          if [ "$LIBC" = gnu ]; then
            TARGET=aarch64-unknown-linux-musl
          else
            TARGET="aarch64-unknown-linux-$LIBC"
          fi
          ;;
        armv7* | armv8l)
          if [ "$LIBC" = gnu ]; then
            TARGET=armv7-unknown-linux-musleabihf
          else
            TARGET="armv7-unknown-linux-${LIBC}eabihf"
          fi
          ;;
        armv6* | arm)
          if [ "$LIBC" = gnu ]; then
            TARGET=arm-unknown-linux-musleabihf
          else
            TARGET="arm-unknown-linux-${LIBC}eabihf"
          fi
          ;;
        i686 | i386) TARGET="i686-unknown-linux-$LIBC" ;;
        riscv64) TARGET="riscv64gc-unknown-linux-$LIBC" ;;
        loongarch64) TARGET="loongarch64-unknown-linux-$LIBC" ;;
        s390x) TARGET=s390x-unknown-linux-gnu ;;
        ppc64le | powerpc64le) TARGET="powerpc64le-unknown-linux-$LIBC" ;;
      esac
    fi
    ;;
  Darwin)
    case "$ARCH" in
      arm64 | aarch64) TARGET=aarch64-apple-darwin ;;
      *) TARGET=x86_64-apple-darwin ;;
    esac
    ;;
  MINGW* | MSYS* | CYGWIN* | Windows*)
    case "$ARCH" in
      aarch64 | arm64) TARGET=aarch64-pc-windows-msvc ;;
      i686 | i386) TARGET=i686-pc-windows-msvc ;;
      *) TARGET=x86_64-pc-windows-msvc ;;
    esac
    ;;
  FreeBSD)
    TARGET=x86_64-unknown-freebsd
    ;;
esac

echo "OS=$OS"
echo "ARCH=$ARCH"
echo "LIBC=$LIBC"
echo "ANDROID=$ANDROID"
echo "TARGET=$TARGET"
