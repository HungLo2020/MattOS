#!/usr/bin/env python3
"""Exercise hardware audio from the active, unprivileged desktop session."""

import glob
import json
import math
import os
import struct
import subprocess
import tempfile
import time
import wave
from pathlib import Path


def main():
    if os.geteuid() == 0:
        raise RuntimeError("Run this check as the logged-in desktop user")
    for attempt in range(60):
        controls = glob.glob("/dev/snd/controlC*")
        if controls and all(os.access(p, os.R_OK | os.W_OK) for p in controls):
            break
        time.sleep(1)
    else:
        raise RuntimeError("The desktop user cannot access the ALSA sound cards")
    for attempt in range(60):
        # An ordinary client must activate the core and its session manager.
        # Do not start services manually or require pre-existing clients.
        objects = json.loads(subprocess.check_output(["pw-dump"], timeout=15))
        sinks = [item.get("info", {}).get("props", {}) for item in objects]
        sinks = [p for p in sinks if p.get("media.class") == "Audio/Sink"
                 and p.get("node.name", "").startswith("alsa_output.")]
        if sinks:
            break
        time.sleep(1)
    else:
        raise RuntimeError("WirePlumber did not create a hardware ALSA output")
    subprocess.run(["systemctl", "--user", "is-active", "pipewire.service",
                    "wireplumber.service", "pipewire-pulse.socket"], check=True)
    with tempfile.TemporaryDirectory(prefix="mattos-audio-") as directory:
        tone = Path(directory) / "tone.wav"
        with wave.open(str(tone), "wb") as output:
            output.setnchannels(2)
            output.setsampwidth(2)
            output.setframerate(48000)
            output.writeframes(b"".join(
                struct.pack("<hh", sample, sample)
                for sample in (int(5000 * math.sin(2 * math.pi * 440 * n / 48000))
                               for n in range(48000))
            ))
        subprocess.run(["pw-play", str(tone)], check=True, timeout=30)
    # Exercise the PulseAudio compatibility server, not just native clients.
    subprocess.run(["pactl", "info"], check=True, timeout=15)
    pulse_sinks = subprocess.check_output(["pactl", "list", "short", "sinks"], timeout=15).decode()
    if "alsa_output." not in pulse_sinks:
        raise RuntimeError("PulseAudio clients cannot see the hardware sink")
    print("PASS hardware audio: session ACLs, ALSA sink, playback, PulseAudio compatibility")


if __name__ == "__main__":
    main()
