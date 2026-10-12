"""Retakes the README screenshots in docs/.

Start the server first (`cargo run`), then run `python scripts/screenshots.py`. It drives
headless Chrome or Edge over the DevTools protocol, using only the Python standard library,
and captures each page at 1440px wide in the dark theme. Set CHROME to the browser's path if
it isn't found, and PORT if the server isn't on 3000. Pass a directory to write somewhere
other than docs/.
"""

import base64
import json
import os
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request
from urllib.parse import quote

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASE = f"http://127.0.0.1:{os.environ.get('PORT', '3000')}"
DEBUG_PORT = 9333
WIDTH = 1440
TEAM = "Rillaboom,Incineroar,Sneasler,Salamence-Mega"

BROWSERS = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "microsoft-edge",
]


def find_browser():
    if os.environ.get("CHROME"):
        return os.environ["CHROME"]
    for b in BROWSERS:
        path = b if os.path.isfile(b) else shutil.which(b)
        if path:
            return path
    sys.exit("Couldn't find Chrome or Edge. Set CHROME to its path.")


class DevTools:
    """A minimal WebSocket client, just enough for DevTools protocol calls."""

    def __init__(self, url):
        host_port, path = url[len("ws://"):].split("/", 1)
        host, port = host_port.split(":")
        self.sock = socket.create_connection((host, int(port)))
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall(
            (
                f"GET /{path} HTTP/1.1\r\nHost: {host_port}\r\nUpgrade: websocket\r\n"
                f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n"
                "Sec-WebSocket-Version: 13\r\n\r\n"
            ).encode()
        )
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.sock.recv(4096)
        self.buf = buf.split(b"\r\n\r\n", 1)[1]
        self.last_id = 0

    def _read(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(1 << 20)
            if not chunk:
                raise EOFError("browser closed the connection")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def _recv(self):
        data = b""
        while True:
            b1, b2 = self._read(2)
            n = b2 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._read(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._read(8))[0]
            data += self._read(n)
            if b1 & 0x80:
                return json.loads(data)

    def _send(self, obj):
        payload = json.dumps(obj).encode()
        n = len(payload)
        head = bytearray([0x81])
        if n < 126:
            head.append(0x80 | n)
        elif n < 65536:
            head += bytes([0x80 | 126]) + struct.pack(">H", n)
        else:
            head += bytes([0x80 | 127]) + struct.pack(">Q", n)
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        self.sock.sendall(bytes(head) + mask + masked)

    def call(self, method, **params):
        self.last_id += 1
        self._send({"id": self.last_id, "method": method, "params": params})
        while True:
            msg = self._recv()
            if msg.get("id") == self.last_id:
                if "error" in msg:
                    raise RuntimeError(f"{method}: {msg['error']}")
                return msg.get("result", {})

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, awaitPromise=True, returnByValue=True)
        if "exceptionDetails" in r:
            raise RuntimeError(r["exceptionDetails"])
        return r["result"].get("value")


# Waits for the page, its fonts and every background image (the sprite sheet) to load.
SETTLE = """(async () => {
  for (let i = 0; i < 100 && document.readyState !== 'complete'; i++)
    await new Promise(r => setTimeout(r, 100));
  await document.fonts.ready;
  const urls = new Set();
  for (const el of document.querySelectorAll('*')) {
    const m = getComputedStyle(el).backgroundImage.match(/url\\("(.+?)"\\)/);
    if (m) urls.add(m[1]);
  }
  await Promise.all([...urls].map(u => new Promise(r => {
    const i = new Image(); i.onload = i.onerror = r; i.src = u;
  })));
  await new Promise(r => setTimeout(r, 400));
})()"""


def settle(dt):
    time.sleep(0.5)
    dt.js(SETTLE)


def go(dt, url):
    dt.call("Page.navigate", url=url)
    settle(dt)


def submit(dt, script):
    """Runs a script that submits a form, then waits for the next page."""
    dt.js(script)
    settle(dt)


def shot(dt, out, name, height):
    dt.call("Emulation.setDeviceMetricsOverride", width=WIDTH, height=height,
            deviceScaleFactor=1, mobile=False)
    dt.js("window.scrollTo(0, 0); new Promise(r => setTimeout(r, 300))")
    img = dt.call("Page.captureScreenshot", format="png",
                  clip={"x": 0, "y": 0, "width": WIDTH, "height": height, "scale": 1})
    with open(os.path.join(out, name), "wb") as f:
        f.write(base64.b64decode(img["data"]))
    print("wrote", os.path.join(out, name))


def take_all(dt, out):
    team = quote(TEAM, safe="")

    # Usage: three Pokémon side by side.
    go(dt, f"{BASE}/?fmt=vgc&cut=1630&pick={quote('Rillaboom,Sneasler,Incineroar', safe='')}")
    shot(dt, out, "screenshot.png", 1180)

    # Team builder, with the team also saved so the saved teams panel has an entry.
    go(dt, f"{BASE}/builder?fmt=vgc&cut=1630&team={team}")
    dt.js(f"""fetch('/saved/save', {{method: 'POST', body: new URLSearchParams({{
        fmt: 'vgc', team: '{TEAM}', back: '/builder', name: 'Grass Tailwind'}})}})""")
    go(dt, f"{BASE}/builder?fmt=vgc&cut=1630&team={team}")
    shot(dt, out, "team-builder.png", 1320)

    # Damage calc: Sneasler's Close Combat into a Chople Berry Kingambit.
    go(dt, f"{BASE}/calc?fmt=vgc&cut=1630&a=Sneasler&d=Kingambit")
    submit(dt, """(() => {
        document.querySelector('[name="mv"]').value = 'Close Combat';
        document.querySelector('[name="d_item"]').value = 'Chople Berry';
        document.querySelector('[name="mv"]').form.requestSubmit();
    })()""")
    shot(dt, out, "damage-calc.png", 1300)

    # Speed tiers mid-match: four opponents, Tailwind on your side, rain.
    go(dt, f"{BASE}/speed?fmt=vgc&cut=1630&team={team}")
    for foe in ["Dragonite-Mega", "Pelipper", "Incineroar", "Kingambit"]:
        submit(dt, f"""(() => {{
            const i = document.querySelector('input[list]');
            i.value = '{foe}'; i.form.requestSubmit();
        }})()""")
    for label in ["Your side", "Rain"]:
        href = dt.js(f"""[...document.querySelectorAll('a')]
            .find(a => a.textContent.trim() === '{label}').href""")
        go(dt, href)
    shot(dt, out, "speed-tiers.png", 1300)

    # Meta teams, with the rain team's game plan open.
    go(dt, f"{BASE}/teams?fmt=vgc&cut=1630")
    dt.js("""[...document.querySelectorAll('.team-card')]
        .find(c => c.querySelector('h3').textContent.includes('Archaludon'))
        .querySelector('details.plan').open = true""")
    shot(dt, out, "game-plan.png", 1640)

    # The Ko-fi donation pop-up, when the server has a Ko-fi SUPPORT_URL.
    go(dt, f"{BASE}/?fmt=vgc&cut=1630")
    if not dt.js("!!document.querySelector('#support')"):
        print("skipped support-popup.png: start the server with a Ko-fi SUPPORT_URL")
        return
    dt.call("Emulation.setDeviceMetricsOverride", width=WIDTH, height=900,
            deviceScaleFactor=1, mobile=False)
    loaded = dt.js("""new Promise(r => {
        const frame = document.querySelector('#support iframe');
        frame.addEventListener('load', () => r(true), {once: true});
        document.querySelector('header .support-pill').click();
        setTimeout(() => r(false), 20000);
    })""")
    if not loaded:
        print("warning: Ko-fi's panel didn't load within 20 seconds")
    time.sleep(3)  # Ko-fi draws its form after the frame loads
    dt.js("document.getAnimations().forEach(a => a.finish())")
    img = dt.call("Page.captureScreenshot", format="png")
    with open(os.path.join(out, "support-popup.png"), "wb") as f:
        f.write(base64.b64decode(img["data"]))
    print("wrote", os.path.join(out, "support-popup.png"))


def main():
    out = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "docs"))
    try:
        urllib.request.urlopen(BASE, timeout=30)
    except OSError:
        sys.exit(f"PokeChecks isn't running at {BASE}. Start it with `cargo run` first.")

    profile = tempfile.mkdtemp(prefix="pokechecks-shots-")
    browser = subprocess.Popen(
        [find_browser(), "--headless=new", f"--remote-debugging-port={DEBUG_PORT}",
         f"--user-data-dir={profile}", "--no-first-run", "--hide-scrollbars",
         f"--window-size={WIDTH},1300", "about:blank"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        page = None
        for _ in range(50):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{DEBUG_PORT}/json") as r:
                    page = next(t for t in json.load(r) if t["type"] == "page")
                break
            except (OSError, StopIteration):
                time.sleep(0.2)
        if page is None:
            sys.exit("The browser didn't start its DevTools server.")
        dt = DevTools(page["webSocketDebuggerUrl"])
        dt.call("Page.enable")
        # Ko-fi's bot protection blanks its panel for browsers that say they're headless.
        version = dt.call("Browser.getVersion")["userAgent"].replace("HeadlessChrome", "Chrome")
        dt.call("Network.setUserAgentOverride", userAgent=version)
        # "System" theme in dark mode, so the theme switch looks untouched.
        dt.call("Emulation.setEmulatedMedia",
                features=[{"name": "prefers-color-scheme", "value": "dark"}])
        take_all(dt, out)
    finally:
        browser.terminate()
        browser.wait(timeout=10)
        shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    main()
