#!/usr/bin/env python3
import sys
import os

# CGI Header
sys.stdout.write("Status: 200 OK\r\n")
sys.stdout.write("Content-Type: text/html; charset=utf-8\r\n")
sys.stdout.write("Set-Cookie: cgi_executed=true; Path=/; SameSite=Lax\r\n")
sys.stdout.write("\r\n")

# Read stdin until EOF
body = ""
try:
    content_length = int(os.environ.get("CONTENT_LENGTH", 0))
    if content_length > 0:
        body = sys.stdin.read(content_length)
    else:
        body = sys.stdin.read()
except Exception:
    pass

script_arg = sys.argv[1] if len(sys.argv) > 1 else "None"
method = os.environ.get("REQUEST_METHOD", "UNKNOWN")
query = os.environ.get("QUERY_STRING", "")
path_info = os.environ.get("PATH_INFO", "")
remote_addr = os.environ.get("REMOTE_ADDR", "")
cookie = os.environ.get("HTTP_COOKIE", "")

sys.stdout.write(f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>CGI Execution Output</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0b0f19; color: #f8fafc; padding: 2.5rem; }}
        .card {{ background: rgba(30, 41, 59, 0.7); border: 1px solid rgba(255, 255, 255, 0.1); border-radius: 1rem; padding: 2rem; max-width: 800px; margin: 0 auto; }}
        h1 {{ color: #38bdf8; font-size: 1.6rem; margin-bottom: 1rem; }}
        table {{ width: 100%; border-collapse: collapse; margin-top: 1rem; }}
        th, td {{ padding: 0.6rem 0.8rem; border-bottom: 1px solid rgba(255, 255, 255, 0.06); text-align: left; font-size: 0.95rem; }}
        th {{ color: #94a3b8; width: 35%; }}
        td {{ color: #38bdf8; font-family: monospace; word-break: break-all; }}
        .body-box {{ background: #050811; padding: 1rem; border-radius: 0.5rem; border: 1px solid rgba(255, 255, 255, 0.05); margin-top: 1rem; font-family: monospace; white-space: pre-wrap; }}
        a {{ color: #38bdf8; text-decoration: none; }}
    </style>
</head>
<body>
    <div class="card">
        <h1>Python CGI 1.1 Gateway Execution</h1>
        <p style="color: #94a3b8;">Executed successfully by Localhost HTTP/1.1 Engine.</p>

        <table>
            <tr><th>Script Argument (argv[1])</th><td>{script_arg}</td></tr>
            <tr><th>REQUEST_METHOD</th><td>{method}</td></tr>
            <tr><th>QUERY_STRING</th><td>{query}</td></tr>
            <tr><th>PATH_INFO</th><td>{path_info}</td></tr>
            <tr><th>REMOTE_ADDR</th><td>{remote_addr}</td></tr>
            <tr><th>HTTP_COOKIE</th><td>{cookie}</td></tr>
            <tr><th>GATEWAY_INTERFACE</th><td>{os.environ.get("GATEWAY_INTERFACE", "")}</td></tr>
            <tr><th>SERVER_PROTOCOL</th><td>{os.environ.get("SERVER_PROTOCOL", "")}</td></tr>
        </table>

        <h3 style="margin-top: 1.5rem; color: #e2e8f0;">Request Body Received via STDIN (EOF terminated):</h3>
        <div class="body-box">{body if body else "[No body payload received]"}</div>

        <p style="margin-top: 1.5rem;"><a href="/">&larr; Return to Dashboard</a></p>
    </div>
</body>
</html>
""")
