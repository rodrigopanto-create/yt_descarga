#!/usr/bin/env bash
set -euo pipefail

# Run with Git Bash on Windows; the NDK supplies clang and GNU make.
ndk=$(cygpath -u "$1")
root=$(cd "$(dirname "$0")/.." && pwd)
work="$root/target/ffmpeg-android"
toolchain="$ndk/toolchains/llvm/prebuilt/windows-x86_64/bin"
export PATH="$toolchain:$ndk/prebuilt/windows-x86_64/bin:$PATH"
export SHELL=/usr/bin/sh
make_shell=$(cygpath -m -s /usr/bin/sh.exe)
jobs=${NUMBER_OF_PROCESSORS:-4}
prefix=$(cygpath -m "$work/prefix")

cd "$work/lame-3.100"
if [ ! -f Makefile ]; then
    CC="clang.exe --target=aarch64-linux-android21" \
    AR=llvm-ar.exe RANLIB=llvm-ranlib.exe NM=llvm-nm.exe \
    ./configure --host=aarch64-linux-android \
        --prefix="$prefix" --disable-shared --enable-static \
        --disable-frontend --disable-analyzer-hooks \
        CFLAGS="-O2 -fPIC"
fi
make.exe -j"$jobs" SHELL="$make_shell"
make.exe install SHELL="$make_shell"

cd "$work/ffmpeg-8.0.3"
./configure --target-os=android --arch=aarch64 --enable-cross-compile \
    --cc="clang.exe --target=aarch64-linux-android21" \
    --cxx="clang++.exe --target=aarch64-linux-android21" \
    --host-cc="clang.exe --target=x86_64-pc-windows-msvc -fuse-ld=link" \
    --host-extralibs= \
    --ar=llvm-ar.exe --ranlib=llvm-ranlib.exe \
    --strip=llvm-strip.exe --nm=llvm-nm.exe \
    --disable-shared --enable-static --enable-pic \
    --disable-autodetect --disable-doc --disable-debug --disable-ffplay \
    --disable-network --disable-avdevice --disable-everything \
    --enable-ffmpeg --enable-ffprobe --enable-avfilter --enable-swresample \
    --enable-protocol=file,pipe \
    --enable-demuxer=mov,matroska,mp3,aac,ogg,wav,flac \
    --enable-muxer=mp3,mp4,mov,matroska,webm,ipod,ogg,wav,flac \
    --enable-decoder=aac,mp3,opus,vorbis,flac,pcm_s16le \
    --enable-parser=aac,mpegaudio,opus,vorbis,flac,h264,hevc,av1,vp9 \
    --enable-bsf=aac_adtstoasc,h264_mp4toannexb,hevc_mp4toannexb,extract_extradata,vp9_superframe,av1_metadata,setts \
    --enable-filter=aresample,aformat,anull,atrim,asetpts \
    --enable-libmp3lame --enable-encoder=libmp3lame \
    --pkg-config=false \
    --extra-cflags="-I$prefix/include" \
    --extra-ldflags="-L$prefix/lib -Wl,-z,max-page-size=16384"
make.exe -j"$jobs" SHELL="$make_shell" ffmpeg ffprobe
llvm-strip.exe ffmpeg ffprobe
cp ffmpeg "$root/jniLibs/arm64-v8a/libffmpeg.so"
cp ffprobe "$root/jniLibs/arm64-v8a/libffprobe.so"
