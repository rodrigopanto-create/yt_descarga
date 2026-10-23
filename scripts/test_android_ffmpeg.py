"""Offline device test: packaged Python -S test_android_ffmpeg.py NATIVE_DIR INPUT_MP4."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

native, source = map(Path, sys.argv[1:])
sys.path.insert(0, str(Path(os.environ["PYTHONHOME"]).parent / "yt-dlp.pyz"))
from yt_dlp import YoutubeDL


def run(tool, *args):
    return subprocess.check_output(
        [str(native / f"lib{tool}.so"), *map(str, args)],
        stderr=subprocess.STDOUT,
        timeout=60,
    )


def streams(path):
    return json.loads(run(
        "ffprobe", "-v", "error", "-show_entries",
        "stream=codec_name,codec_type,duration", "-of", "json", path,
    ))["streams"]


with tempfile.TemporaryDirectory(prefix="ffmpeg-test-", dir=source.parent) as directory:
    work = Path(directory)
    video, audio = work / "video.mp4", work / "audio.m4a"
    mp3 = work / "converted.mp3"
    run("ffmpeg", "-v", "error", "-nostdin", "-n", "-i", source,
        "-map", "0:v:0", "-c", "copy", video)
    run("ffmpeg", "-v", "error", "-nostdin", "-n", "-i", source,
        "-map", "0:a:0", "-c", "copy", audio)
    with YoutubeDL({
        "ffmpeg_location": str(native / "libffmpeg.so"),
        "outtmpl": str(work / "converted.%(ext)s"),
        "enable_file_urls": True,
        "quiet": True,
        "noprogress": True,
        "postprocessors": [{
            "key": "FFmpegExtractAudio",
            "preferredcodec": "mp3",
            "preferredquality": "0",
        }],
    }) as ydl:
        ydl.process_ie_result({
            "id": "licensed-sintel-audio",
            "title": "Sintel MP3 test",
            "url": audio.as_uri(),
            "ext": "m4a",
            "acodec": "aac",
            "vcodec": "none",
        }, download=True)
    encoded = streams(mp3)
    assert len(encoded) == 1 and encoded[0]["codec_name"] == "mp3", encoded
    assert float(encoded[0]["duration"]) > 50, encoded
    run("ffmpeg", "-v", "error", "-nostdin", "-i", mp3,
        "-c:a", "pcm_s16le", "-f", "null", "-")
    print("MP3_ENCODE_AND_DECODE_OK", json.dumps(encoded))

    with YoutubeDL({
        "ffmpeg_location": str(native / "libffmpeg.so"),
        "format": "bestvideo+bestaudio",
        "outtmpl": str(work / "merged.%(ext)s"),
        "enable_file_urls": True,
        "quiet": True,
        "noprogress": True,
    }) as ydl:
        ydl.process_ie_result({
            "id": "licensed-sintel-fixture",
            "title": "Sintel merge test",
            "formats": [
                {"format_id": "video", "url": video.as_uri(),
                 "ext": "mp4", "vcodec": "h264", "acodec": "none"},
                {"format_id": "audio", "url": audio.as_uri(),
                 "ext": "m4a", "vcodec": "none", "acodec": "aac"},
            ],
        }, download=True)
    merged = streams(work / "merged.mp4")
    assert sorted(s["codec_type"] for s in merged) == ["audio", "video"], merged
    assert all(float(s["duration"]) > 50 for s in merged), merged
    print("YTDLP_MERGE_OK", json.dumps(merged))
