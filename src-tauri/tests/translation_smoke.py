"""End-to-end translation smoke test; downloads the real local model on first run."""
import json
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKER = ROOT / "src-tauri" / "translation_worker.py"

requests = [
    {
        "name": "English to Tamil",
        "text": "Good morning. The meeting will start in five minutes.",
        "source": "en",
        "target": "ta",
        "expected_range": (0x0B80, 0x0BFF),
    },
    {
        "name": "English to Sinhala",
        "text": "Good morning. The meeting will start in five minutes.",
        "source": "en",
        "target": "si",
        "expected_range": (0x0D80, 0x0DFF),
    },
    {
        "name": "Tamil to English",
        "text": "வணக்கம். இன்று நீங்கள் எப்படி இருக்கிறீர்கள்?",
        "source": "ta",
        "target": "en",
        "expected_range": (0x0041, 0x024F),
    },
    {
        "name": "Sinhala to English",
        "text": "ආයුබෝවන්. අද ඔබට කොහොමද?",
        "source": "si",
        "target": "en",
        "expected_range": (0x0041, 0x024F),
    },
    {
        "name": "Mixed English and Tamil to English",
        "text": "Good morning, வணக்கம்!",
        "source": "mul",
        "target": "en",
        "expected_range": (0x0041, 0x024F),
    },
]

payload = "".join(
    json.dumps(
        {"text": item["text"], "source": item["source"], "target": item["target"]},
        ensure_ascii=False,
    ) + "\n"
    for item in requests
)

env = os.environ.copy()
env["PYTHONUNBUFFERED"] = "1"
env["PYTHONIOENCODING"] = "utf-8"
env["PYTHONUTF8"] = "1"
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
process = subprocess.Popen(
    [sys.executable, str(WORKER)],
    cwd=str(ROOT),
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=None,
    text=True,
    encoding="utf-8",
    env=env,
)
try:
    stdout, _ = process.communicate(payload, timeout=900)
except subprocess.TimeoutExpired:
    process.kill()
    process.communicate()
    raise SystemExit("Translation smoke test timed out after 15 minutes")

if process.returncode != 0:
    raise SystemExit(f"Translation worker exited with code {process.returncode}")

lines = [line for line in stdout.splitlines() if line.strip()]
if len(lines) != len(requests):
    raise SystemExit(
        f"Expected {len(requests)} JSON responses, received {len(lines)}. "
        f"Worker stdout was: {stdout!r}"
    )

for item, line in zip(requests, lines):
    response = json.loads(line)
    if "error" in response:
        raise SystemExit(f"{item['name']} failed: {response['error']}")
    translated = response.get("translation", "").strip()
    if not translated or translated == item["text"]:
        raise SystemExit(f"{item['name']} returned empty or unchanged text: {translated!r}")
    low, high = item["expected_range"]
    if not any(low <= ord(character) <= high for character in translated):
        raise SystemExit(
            f"{item['name']} did not produce the target script: {translated!r}"
        )
    if item["name"] == "English to Sinhala":
        words = re.findall(r"\w+", translated)
        if len(words) >= 5 and max(Counter(words).values()) >= 4:
            raise SystemExit(
                f"{item['name']} produced repetitive output: {translated!r}"
            )
    print(f"PASS: {item['name']} -> {translated}")

print("All real-model translation smoke tests passed.")
