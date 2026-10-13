"""Load test: simulated visitors browsing PokeChecks at the same time.

Each visitor has their own cookies and clicks random links, fills in the damage calc and
speed tiers, searches the ladder, and saves and deletes teams. Every response is checked for
errors, and for another visitor's saved team (which would mean data leaking between people).

    python scripts/loadtest.py 50 --seconds 120 --think 1 3    # 50 visitors, 1-3s per click
    python scripts/loadtest.py 50 --seconds 60 --think 0 0     # flat out
    python scripts/loadtest.py --burst 60                       # 60 cold requests at once

Run it against a local server (`cargo run`, or the Docker image). To mimic Render Starter:

    docker build -t pokechecks .
    docker run --rm --cpus=0.5 --memory=512m -p 10000:10000 pokechecks
    python scripts/loadtest.py 200 --base http://127.0.0.1:10000

It refuses other hosts unless given --allow-remote: a load test on the live site slows it
down for real visitors. Exits with status 1 if any request failed or leaked.
Only the Python standard library is needed.
"""

import argparse
import html
import http.cookiejar
import json
import random
import re
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

FORMATS = ["vgc", "vgc-bo3", "bss"]
CUTOFFS = [0, 1500, 1630, 1760]
PAGES = ["/", "/builder", "/tournaments", "/tournaments?arch=rain", "/calc", "/speed", "/speed?mode=meta"]
HREF = re.compile(r'href="(/[^"#]*)"')
OPTION = re.compile(r'<option value="([^"]+)"')
# Saved team names the test uses, so a visitor can spot someone else's.
SAVED_NAME = re.compile(r"LT-u(\d+)-\d+")


class Results:
    def __init__(self):
        self.lock = threading.Lock()
        self.rows = []  # (route, status, seconds, ok)
        self.problems = []
        self.leaks = []

    def add(self, url, status, secs, problem=None):
        with self.lock:
            self.rows.append((urllib.parse.urlparse(url).path or "/", status, secs, problem is None))
            if problem and len(self.problems) < 200:
                self.problems.append(f"{status} {problem} {url[:160]}")

    def leak(self, text):
        with self.lock:
            self.leaks.append(text)


class Visitor:
    def __init__(self, uid, base, think, results):
        self.uid = uid
        self.base = base
        self.think = think
        self.results = results
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar())
        )
        self.page = ""
        self.url = ""
        self.saves = 0
        self.rng = random.Random(uid)

    def fetch(self, url, form=None):
        data = urllib.parse.urlencode(form).encode() if form else None
        # Browsers mark their own form posts as same-origin; PokeChecks refuses cross-site ones.
        headers = {"Sec-Fetch-Site": "same-origin"} if form else {}
        start = time.perf_counter()
        try:
            req = urllib.request.Request(self.base + url, data=data, headers=headers)
            with self.opener.open(req, timeout=60) as r:
                body = r.read().decode("utf-8", "replace")
                status, final = r.status, r.geturl()[len(self.base):]
        except urllib.error.HTTPError as e:
            self.results.add(url, e.code, time.perf_counter() - start, "http-error")
            return
        except Exception as e:  # timeouts, refused or reset connections
            self.results.add(url, 0, time.perf_counter() - start, type(e).__name__)
            return
        secs = time.perf_counter() - start
        problem = None
        if "</html>" not in body:
            problem = "truncated"
        elif "Usage stats are unavailable" in body:
            problem = "error-page"
        self.results.add(url, status, secs, problem)
        for owner in set(SAVED_NAME.findall(body)):
            if int(owner) != self.uid:
                self.results.leak(f"visitor {self.uid} saw visitor {owner}'s team on {url[:120]}")
        if problem is None:
            self.page, self.url = body, final

    def links(self):
        return [
            html.unescape(h)
            for h in HREF.findall(self.page)
            if not h.startswith(("/app.css", "/sprites.png", "/fonts/"))
        ]

    def start(self):
        page = self.rng.choice(PAGES)
        sep = "&" if "?" in page else "?"
        self.fetch(f"{page}{sep}fmt={self.rng.choice(FORMATS)}")

    def calc(self):
        if "/calc" not in self.url:
            self.fetch(f"/calc?fmt={self.rng.choice(FORMATS)}")
        mons = [n for n in OPTION.findall(self.page) if n[:1].isupper()][:80] or ["Incineroar"]
        q = {"fmt": self.rng.choice(FORMATS), "a": self.rng.choice(mons), "d": self.rng.choice(mons)}
        self.fetch("/calc?" + urllib.parse.urlencode(q))

    def speed(self):
        if "/speed" not in self.url or "mode=meta" in self.url:
            self.fetch(f"/speed?fmt={self.rng.choice(FORMATS)}")
        names = OPTION.findall(self.page)
        if not names:
            return
        # Like the "Type a Pokémon" form: the current state plus one `add`.
        path, _, query = self.url.partition("?")
        params = [(k, v) for k, v in urllib.parse.parse_qsl(query) if k != "add"]
        params.append(("add", self.rng.choice(names)[:5]))
        self.fetch(f"{path}?{urllib.parse.urlencode(params)}")

    def search(self):
        q = {"fmt": self.rng.choice(FORMATS), "q": self.rng.choice(["in", "ga", "mega", "z", "ri"])}
        self.fetch("/?" + urllib.parse.urlencode(q))

    def save(self):
        team_links = [h for h in self.links() if "team=" in h]
        if "team=" not in self.url and team_links:
            self.fetch(self.rng.choice(team_links))
        m = re.search(r'name="team" value="([^"]*)"', self.page)
        if not m or not m.group(1):
            return
        self.saves += 1
        self.fetch("/saved/save", {"fmt": "vgc", "team": html.unescape(m.group(1)),
                                   "back": "/builder", "name": f"LT-u{self.uid}-{self.saves}"})

    def delete(self):
        slots = re.findall(r'name="slot" value="(\d+)"', self.page)
        if slots:
            self.fetch("/saved/delete", {"slot": self.rng.choice(slots), "back": "/builder"})

    def step(self):
        roll = self.rng.random()
        links = self.links()
        if not self.page or roll < 0.05:
            self.start()
        elif roll < 0.62 and links:
            self.fetch(self.rng.choice(links))
        elif roll < 0.75:
            self.calc()
        elif roll < 0.85:
            self.speed()
        elif roll < 0.91:
            self.search()
        elif roll < 0.97:
            self.save()
        else:
            self.delete()

    def run(self, until):
        self.start()
        while time.time() < until:
            self.step()
            if self.think[1]:
                time.sleep(self.rng.uniform(*self.think))


def percentile(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(len(xs) * p))] if xs else 0


def summary(results, elapsed):
    by_route = {}
    for route, _, secs, ok in results.rows:
        by_route.setdefault(route, []).append((secs, ok))
    ms = lambda s: round(s * 1000)
    lat = [secs for _, _, secs, _ in results.rows]
    statuses = {}
    for _, status, _, _ in results.rows:
        statuses[str(status)] = statuses.get(str(status), 0) + 1
    return {
        "requests": len(results.rows),
        "seconds": round(elapsed, 1),
        "per_second": round(len(results.rows) / elapsed, 1) if elapsed else 0,
        "failures": sum(1 for *_, ok in results.rows if not ok),
        "statuses": statuses,
        "p50_ms": ms(percentile(lat, 0.5)),
        "p95_ms": ms(percentile(lat, 0.95)),
        "p99_ms": ms(percentile(lat, 0.99)),
        "max_ms": ms(max(lat or [0])),
        "leaks": len(results.leaks),
        "routes": {
            route: {
                "requests": len(rows),
                "failures": sum(1 for _, ok in rows if not ok),
                "p50_ms": ms(percentile([s for s, _ in rows], 0.5)),
                "p95_ms": ms(percentile([s for s, _ in rows], 0.95)),
                "max_ms": ms(max(s for s, _ in rows)),
            }
            for route, rows in sorted(by_route.items())
        },
        "problems": results.problems[:30],
        "leak_examples": results.leaks[:20],
    }


def report(s):
    print(f"{s['requests']} requests in {s['seconds']}s ({s['per_second']}/s), "
          f"{s['failures']} failed, {s['leaks']} leaks")
    print(f"latency: p50 {s['p50_ms']}ms, p95 {s['p95_ms']}ms, p99 {s['p99_ms']}ms, "
          f"max {s['max_ms']}ms; statuses {s['statuses']}")
    for route, r in s["routes"].items():
        print(f"  {route:16} {r['requests']:6} requests  {r['failures']:4} failed  "
              f"p50 {r['p50_ms']:5}ms  p95 {r['p95_ms']:5}ms  max {r['max_ms']:6}ms")
    for p in s["problems"][:10]:
        print("  problem:", p)
    for leak in s["leak_examples"][:5]:
        print("  LEAK:", leak)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("visitors", type=int, nargs="?", default=50,
                    help="simulated visitors at once (default 50)")
    ap.add_argument("--base", default="http://127.0.0.1:3000", help="server to test")
    ap.add_argument("--seconds", type=float, default=120, help="how long to run (default 120)")
    ap.add_argument("--think", type=float, nargs=2, default=(1, 3), metavar=("MIN", "MAX"),
                    help="seconds between a visitor's clicks (default 1 3; 0 0 = flat out)")
    ap.add_argument("--burst", type=int, metavar="N",
                    help="instead, send N requests for every format and cutoff at once")
    ap.add_argument("--json", metavar="FILE", help="also write the full results here")
    ap.add_argument("--allow-remote", action="store_true",
                    help="allow a server other than localhost")
    args = ap.parse_args()

    base = args.base.rstrip("/")
    host = urllib.parse.urlparse(base).hostname
    if host not in ("127.0.0.1", "localhost", "::1") and not args.allow_remote:
        sys.exit(f"{base} isn't a local server. Load testing the live site slows it down for "
                 "real visitors; pass --allow-remote if you really mean it.")
    try:
        urllib.request.urlopen(base + "/healthz", timeout=30)
    except OSError:
        sys.exit(f"PokeChecks isn't answering at {base}. Start it first.")

    results = Results()
    start = time.time()
    if args.burst:
        # Pages for every format and cutoff, so most need data nobody has loaded yet.
        urls = [f"/?fmt={f}&cut={c}" for f in FORMATS for c in CUTOFFS]
        urls += ["/tournaments", "/tournaments?add=incin&arch=tailwind", "/calc?fmt=vgc-bo3"]
        gate = threading.Barrier(args.burst)

        def one(i):
            v = Visitor(10_000 + i, base, (0, 0), results)
            gate.wait()
            v.fetch(urls[i % len(urls)])

        threads = [threading.Thread(target=one, args=(i,)) for i in range(args.burst)]
    else:
        until = time.time() + args.seconds
        threads = [
            threading.Thread(target=Visitor(i, base, tuple(args.think), results).run,
                             args=(until,), daemon=True)
            for i in range(args.visitors)
        ]
    for t in threads:
        t.start()
        if not args.burst:
            time.sleep(0.02)  # ramp up over a few seconds
    for t in threads:
        t.join(timeout=args.seconds + 120)

    s = summary(results, time.time() - start)
    report(s)
    if args.json:
        with open(args.json, "w") as f:
            json.dump(s, f, indent=1)
    sys.exit(1 if s["failures"] or s["leaks"] else 0)


if __name__ == "__main__":
    main()
