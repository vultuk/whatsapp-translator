#!/usr/bin/env python3
"""Render Babel Bridge's original short notification tones (no external samples).

Outputs mono 44.1 kHz, 16-bit linear PCM WAV, suitable for Apple notifications
and browser previews. Xcode copies these same assets into both native apps.
"""
import json
import math
from pathlib import Path
import struct
import wave

RATE = 44100
OUTPUT = Path(__file__).resolve().parents[1] / "web/public/sounds"


def render(name, duration, notes, harmonics, decay, bend=0):
    samples = [0.0] * int(duration * RATE)
    for start, frequency, gain in notes:
        for index in range(int(start * RATE), len(samples)):
            t = index / RATE - start
            attack = min(1.0, t / 0.008)
            release = min(1.0, (duration - index / RATE) / 0.09)
            phase = frequency * t + bend * 0.025 * (1 - math.exp(-t / 0.025))
            value = sum(
                weight * math.sin(2 * math.pi * multiple * phase)
                * math.exp(-t * decay * (1 + (multiple - 1) * 0.22))
                for multiple, weight in harmonics
            )
            samples[index] += gain * attack * release * value
    peak = max(abs(value) for value in samples)
    samples = [value * 0.48 / peak for value in samples]
    filename = f"bb-{name}.wav"
    with wave.open(str(OUTPUT / filename), "wb") as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(RATE)
        audio.writeframes(b"".join(struct.pack("<h", round(value * 32767)) for value in samples))
    rms = math.sqrt(sum(value * value for value in samples) / len(samples))
    print(f"{filename}: {duration:.2f}s, peak -6.4 dBFS, RMS {20 * math.log10(rms):.1f} dBFS")


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    bell = [(1, 1), (2, 0.22), (3, 0.07)]
    render("aurora", 1.65, [(0, 659.25, 0.8), (0.17, 830.61, 0.85), (0.34, 987.77, 0.7)], bell, 4.5)
    render("bamboo", 0.85, [(0, 587.33, 1), (0.19, 783.99, 0.8)], [(1, 1), (2.76, 0.32), (5.4, 0.07)], 12)
    render("bloom", 1.4, [(0, 523.25, 1), (0.13, 659.25, 0.65)], [(1, 1), (2, 0.1), (3, 0.04)], 4.7)
    render("droplet", 0.65, [(0, 880, 1)], [(1, 1), (2, 0.09)], 10, bend=600)
    render("glass", 1.45, [(0, 1174.66, 1)], [(1, 1), (2.01, 0.22), (3.98, 0.04)], 5.2)
    render("orbit", 1.25, [(0, 440, 0.9), (0.24, 659.25, 0.75)], [(1, 1), (2, 0.16)], 5.8)
    tones = [
        ("aurora", "Aurora", "Three bright, rising notes", "sparkles"),
        ("bamboo", "Bamboo", "Two soft wooden taps", "leaf"),
        ("bloom", "Bloom", "A warm, rounded chime", "sun.max"),
        ("droplet", "Droplet", "A playful water drop", "drop"),
        ("glass", "Glass", "One clear, delicate bell", "bell"),
        ("orbit", "Orbit", "A mellow two-note pulse", "circle.dotted"),
    ]
    catalog = [dict(id=key, title=title, detail=detail, symbol=symbol, filename=f"bb-{key}.wav")
               for key, title, detail, symbol in tones]
    (OUTPUT / "message-tones.json").write_text(json.dumps(catalog, indent=2) + "\n")


if __name__ == "__main__":
    main()
