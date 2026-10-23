#!/usr/bin/env bash
set -euo pipefail

# Run on Linux with the official Android NDK. No third-party binary downloads.
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to an Android NDK installation}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/target/android-ffmpeg"
DIST="$WORK/dist"
FFMPEG_VERSION=9.0.2
LAME_VERSION=3.100
LAME_SHA256=ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e
SIGNER=FCF986EA15E6E293A5644F10B4322F04D67658D8
TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CC="$TOOLCHAIN/aarch64-linux-android24-clang"
export AR="$TOOLCHAIN/llvm-ar"
export RANLIB="$TOOLCHAIN/llvm-ranlib"
export STRIP="$TOOLCHAIN/llvm-strip"
test -x "$CC"
mkdir -p "$WORK" "$DIST/jniLibs/arm64-v8a" "$DIST/licenses"
cd "$WORK"
fetch() {
    curl --fail --location --retry 3 --proto '=https' --proto-redir '=https' -o "$2" "$1"
}
fetch "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" ffmpeg.tar.xz
fetch "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz.asc" ffmpeg.tar.xz.asc
fetch https://ffmpeg.org/ffmpeg-devel.asc ffmpeg-key.asc
export GNUPGHOME="$WORK/gnupg"
mkdir -p "$GNUPGHOME"
chmod 700 "$GNUPGHOME"
gpg --batch --import ffmpeg-key.asc
gpg --batch --status-fd 1 --verify ffmpeg.tar.xz.asc ffmpeg.tar.xz > signature-status.txt
grep -q "^\[GNUPG:\] VALIDSIG $SIGNER " signature-status.txt
# LAME checksum independently recorded by MSYS2 mingw-w64-lame/PKGBUILD.
fetch "https://downloads.sourceforge.net/project/lame/lame/$LAME_VERSION/lame-$LAME_VERSION.tar.gz" lame.tar.gz
printf '%s  lame.tar.gz\n' "$LAME_SHA256" | sha256sum --check --strict
tar -xf ffmpeg.tar.xz
tar -xf lame.tar.gz
mkdir -p lame-build ffmpeg-build
cd lame-build
CFLAGS="-O2 -fPIC" "$WORK/lame-$LAME_VERSION/configure" \
    --host=aarch64-linux-android --prefix="$WORK/prefix" \
    --disable-shared --enable-static --disable-frontend --disable-decoder \
    --disable-gtktest --disable-analyzer-hooks
make -j2
make install
cd "$WORK/ffmpeg-build"
"$WORK/ffmpeg-$FFMPEG_VERSION/configure" \
    --target-os=android --arch=aarch64 --enable-cross-compile \
    --cc="$CC" --ar="$AR" --ranlib="$RANLIB" --strip="$STRIP" \
    --enable-pic --extra-cflags="-O2 -I$WORK/prefix/include" \
    --extra-ldflags="-L$WORK/prefix/lib -fPIE -pie -Wl,-z,max-page-size=16384" \
    --disable-autodetect --disable-shared --enable-static \
    --disable-gpl --disable-nonfree --disable-doc --disable-debug \
    --disable-network --disable-everything --enable-ffmpeg --enable-ffprobe \
    --enable-libmp3lame --enable-encoder=libmp3lame,aac,pcm_s16le \
    --enable-decoder=aac,aac_fixed,mp3,mp3float,opus,vorbis,flac,alac,pcm_s16le,h264,vp9,av1 \
    --enable-parser=aac,mpegaudio,opus,vorbis,flac,h264,vp9,av1 \
    --enable-demuxer=mov,matroska,mp3,ogg,flac,aac,wav \
    --enable-muxer=mp4,mov,ipod,matroska,webm,mp3,ogg,flac,adts,wav,null \
    --enable-protocol=file,pipe \
    --enable-filter=aresample,aformat,anull,atrim,asetpts,concat,sine \
    --enable-bsf=aac_adtstoasc,h264_mp4toannexb,extract_extradata \
    --enable-indev=lavfi
make -j2
for feature in CONFIG_LIBMP3LAME_ENCODER CONFIG_MP3_MUXER CONFIG_MP4_MUXER CONFIG_IPOD_MUXER CONFIG_MATROSKA_MUXER; do
    grep -q "^#define $feature 1$" config_components.h
done
for tool in ffmpeg ffprobe; do
    cp "$tool" "$DIST/jniLibs/arm64-v8a/lib$tool.so"
    "$STRIP" --strip-unneeded "$DIST/jniLibs/arm64-v8a/lib$tool.so"
    "$TOOLCHAIN/llvm-readelf" -h -l -d "$DIST/jniLibs/arm64-v8a/lib$tool.so" > "$DIST/$tool-elf.txt"
    grep -q AArch64 "$DIST/$tool-elf.txt"
    grep -q /system/bin/linker64 "$DIST/$tool-elf.txt"
    # Only Android system libraries are allowed. Codec libraries are linked statically.
    if grep '(NEEDED)' "$DIST/$tool-elf.txt" | grep -Ev '\[(libc|libm|libdl|libz|liblog)\.so\]'; then
        echo "Unexpected dynamic dependency in $tool" >&2
        exit 1
    fi
done
cp "$WORK/ffmpeg-$FFMPEG_VERSION/COPYING.LGPLv2.1" "$DIST/licenses/FFmpeg-LGPL-2.1.txt"
cp "$WORK/lame-$LAME_VERSION/COPYING" "$DIST/licenses/LAME-LGPL.txt"
cp "$ROOT/scripts/build-android-ffmpeg.sh" "$DIST/"
cp "$WORK/ffmpeg-build/config.h" "$WORK/ffmpeg-build/ffbuild/config.mak" "$DIST/"
cp "$WORK/signature-status.txt" "$WORK/ffmpeg.tar.xz.asc" "$WORK/ffmpeg-key.asc" "$DIST/"
cp "$WORK/ffmpeg.tar.xz" "$WORK/lame.tar.gz" "$DIST/"
# Preserve objects and static libraries for LGPL relinking with modified libraries.
tar -C "$WORK" -czf "$DIST/relink-objects.tar.gz" \
    ffmpeg-build lame-build prefix
{
    printf 'FFmpeg=%s\nLAME=%s\nAPI=24\nABI=arm64-v8a\n' "$FFMPEG_VERSION" "$LAME_VERSION"
    cat "$ANDROID_NDK_HOME/source.properties"
    printf 'commit=%s\n' "${GITHUB_SHA:-local}"
} > "$DIST/BUILD-INFO.txt"
cd "$DIST"
find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
