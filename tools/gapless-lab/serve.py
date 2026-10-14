import http.server, os
class H(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()
    def do_GET(self):
        if self.path.startswith('/job'):
            if not os.path.exists('job.json'):
                self.send_response(204); self.end_headers(); return
            data = open('job.json', 'rb').read()
            self.send_response(200); self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data); return
        return super().do_GET()
    def do_POST(self):
        n = int(self.headers.get('Content-Length') or 0)
        raw = self.rfile.read(n)
        name = self.path.split('?', 1)[1] if '?' in self.path else 'x'
        if self.path.startswith('/capture'):
            open('capture-' + name + '.f32', 'wb').write(raw)
            self.send_response(204); self.end_headers(); return
        body = raw.decode('utf-8', 'replace')
        open('result-' + name + '.json', 'w', encoding='utf-8').write(body)
        self.send_response(204); self.end_headers()
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(('0.0.0.0', 5180), H).serve_forever()
