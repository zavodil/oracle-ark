#!/usr/bin/env python3
"""Dev server with .env injection and CORS proxy."""

import http.server
import urllib.request
import urllib.error
import os
import json
from pathlib import Path

# Load .env
env = {}
env_path = Path(__file__).parent / '.env'
if env_path.exists():
    for line in env_path.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith('#') and '=' in line:
            key, value = line.split('=', 1)
            env[key.strip()] = value.strip()

# Load tokens.json from parent dir
tokens_path = Path(__file__).parent.parent / 'tokens.json'
TOKENS_JSON = '{}'
if tokens_path.exists():
    TOKENS_JSON = tokens_path.read_text()

API_URL = env.get('API_URL', 'https://testnet-api.outlayer.fastnear.com')
PROJECT_UUID = env.get('PROJECT_UUID', '')
ASSETS = env.get('ASSETS', '')
PORT = int(env.get('PORT', 8000))

class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith('/api/'):
            self.proxy_request('GET')
        elif self.path == '/' or self.path == '/index.html':
            self.serve_index()
        else:
            super().do_GET()

    def do_POST(self):
        if self.path.startswith('/api/'):
            self.proxy_request('POST')
        else:
            self.send_error(404)

    def serve_index(self):
        index_path = Path(__file__).parent / 'index.html'
        html = index_path.read_text()

        # Inject config script before </head>
        config_script = f'''<script>
window.ENV_CONFIG = {{
    apiUrl: "/api",
    projectUuid: "{PROJECT_UUID}",
    assets: "{ASSETS}"
}};
window.TOKENS = {TOKENS_JSON};
</script>'''
        html = html.replace('</head>', config_script + '\n</head>')

        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', len(html.encode()))
        self.end_headers()
        self.wfile.write(html.encode())

    def proxy_request(self, method):
        target_path = self.path[4:]  # Remove '/api' prefix
        target_url = f"{API_URL}{target_path}"

        try:
            body = None
            if method == 'POST':
                content_length = int(self.headers.get('Content-Length', 0))
                body = self.rfile.read(content_length) if content_length > 0 else None

            req = urllib.request.Request(target_url, data=body, method=method)
            req.add_header('Content-Type', self.headers.get('Content-Type', 'application/json'))

            with urllib.request.urlopen(req, timeout=30) as response:
                resp_body = response.read()
                self.send_response(response.status)
                self.send_header('Access-Control-Allow-Origin', '*')
                self.send_header('Content-Type', response.headers.get('Content-Type', 'application/json'))
                self.send_header('Content-Length', len(resp_body))
                self.end_headers()
                self.wfile.write(resp_body)
        except urllib.error.HTTPError as e:
            self.send_response(e.code)
            self.send_header('Access-Control-Allow-Origin', '*')
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(e.read())
        except Exception as e:
            self.send_response(502)
            self.send_header('Access-Control-Allow-Origin', '*')
            self.send_header('Content-Type', 'text/plain')
            self.end_headers()
            self.wfile.write(str(e).encode())

    def do_OPTIONS(self):
        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', '*')
        self.end_headers()

    def log_message(self, format, *args):
        if self.path.startswith('/api/'):
            print(f"[proxy] {self.path} -> {args[0]}")
        elif not self.path.endswith(('.js', '.css', '.ico')):
            print(f"[static] {args[0]}")

if __name__ == '__main__':
    os.chdir(Path(__file__).parent)
    print(f"Oracle Prices UI: http://localhost:{PORT}")
    print(f"API proxy: /api/* -> {API_URL}/*")
    print(f"Project: {PROJECT_UUID}")
    print(f"Assets: {len(ASSETS.split(','))} configured")
    print()
    server = http.server.HTTPServer(('', PORT), Handler)
    server.serve_forever()
