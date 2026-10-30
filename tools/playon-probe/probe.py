#!/usr/bin/env python3
"""Play-on probe (Cratarr step 0): a page that takes commands over SSE and reports what happened.
Run: probe.py <static dir> <port>. Commands: GET /probe/cmd?c=play|pause|resume. Log: GET /probe/log."""
import json, os, queue, sys, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

STATIC, PORT = sys.argv[1], int(sys.argv[2])
TYPES = {".html": "text/html; charset=utf-8", ".js": "text/javascript", ".webmanifest": "application/manifest+json",
         ".png": "image/png", ".mp3": "audio/mpeg"}
pages, lock, log = {}, threading.Lock(), []

def note(s):
    line = time.strftime("%H:%M:%S ") + s
    with lock:
        log.append(line)
        del log[:-500]
    print(line, flush=True)

class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *a): pass
    def send(self, code, body, ctype="text/plain; charset=utf-8", extra=()):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        for k, v in extra: self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)
    def do_GET(self):
        u = urlparse(self.path); q = parse_qs(u.query); p = u.path
        if p == "/probe/events":
            page = q.get("page", ["?"])[0]; box = queue.Queue()
            with lock: pages[page] = box
            note(f"page {page} connected")
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream"); self.send_header("Cache-Control", "no-store")
            self.end_headers()
            try:
                while True:
                    try: msg = f"event: command\ndata: {box.get(timeout=15)}\n\n"
                    except queue.Empty: msg = ": keep-alive\n\n"
                    self.wfile.write(msg.encode()); self.wfile.flush()
            except Exception:
                with lock:
                    if pages.get(page) is box: del pages[page]
                note(f"page {page} disconnected")
            return
        if p == "/probe/cmd":
            c = q.get("c", [""])[0]
            with lock: boxes = list(pages.items())
            for _, b in boxes: b.put(c)
            note(f"sent {c} to {len(boxes)} page(s)")
            return self.send(200, f"sent {c} to {len(boxes)} page(s)\n".encode())
        if p == "/probe/log":
            with lock: body = "\n".join(log) + "\n"
            return self.send(200, body.encode())
        name = "index.html" if p in ("/probe", "/probe/") else p.removeprefix("/probe/")
        f = os.path.join(STATIC, os.path.basename(name))
        if p.startswith("/probe") and os.path.isfile(f):
            with open(f, "rb") as fh: body = fh.read()
            extra = [("Service-Worker-Allowed", "/probe/")] if name == "sw.js" else []
            return self.send(200, body, TYPES.get(os.path.splitext(f)[1], "application/octet-stream"), extra)
        self.send(404, b"not found\n")
    def do_POST(self):
        if urlparse(self.path).path != "/probe/report": return self.send(404, b"")
        n = int(self.headers.get("Content-Length") or 0)
        try: r = json.loads(self.rfile.read(n) or b"{}")
        except ValueError: r = {}
        if r.get("beat"): note(f"page {r.get('page')} alive ({r.get('mode')}, {r.get('visible')}, {'paused' if r.get('paused') else 'playing'})")
        else: note(f"page {r.get('page')} ({r.get('mode')}): {r.get('line')}")
        self.send(204, b"")

ThreadingHTTPServer.daemon_threads = True
note(f"probe on :{PORT}")
ThreadingHTTPServer(("0.0.0.0", PORT), H).serve_forever()
