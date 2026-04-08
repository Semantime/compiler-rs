#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import subprocess
import webbrowser
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from generate_viewer import DEFAULT_COMPILER_BIN, ROOT, render_html


class ViewerHTTPServer(ThreadingHTTPServer):
    allow_reuse_address = True


def main() -> int:
    parser = argparse.ArgumentParser(description="Serve a live regression viewer locally.")
    parser.add_argument(
        "--compiler-bin",
        type=Path,
        default=DEFAULT_COMPILER_BIN,
        help="Path to the built compiler-rs binary.",
    )
    parser.add_argument(
        "--cases-dir",
        type=Path,
        default=None,
        help="Optional regression cases directory.",
    )
    parser.add_argument(
        "--host",
        default="127.0.0.1",
        help="Host to bind.",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8765,
        help="Port to bind.",
    )
    parser.add_argument(
        "--open",
        action="store_true",
        help="Open the viewer in the default browser.",
    )
    args = parser.parse_args()

    compiler_bin = args.compiler_bin.resolve()
    if not compiler_bin.exists():
        raise SystemExit(f"compiler binary not found: {compiler_bin}")

    handler = build_handler(compiler_bin, args.cases_dir, args.host, args.port)
    server = ViewerHTTPServer((args.host, args.port), handler)
    url = f"http://{args.host}:{args.port}"

    if args.open:
        webbrowser.open(url)

    print(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()

    return 0


def build_handler(
    compiler_bin: Path,
    cases_dir: Path | None,
    host: str,
    port: int,
):
    class ViewerHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            parsed = urlparse(self.path)
            if parsed.path in ("", "/"):
                self.respond_html(compiler_bin, cases_dir, host, port)
                return
            if parsed.path == "/viewer.json":
                self.respond_json(compiler_bin, cases_dir)
                return

            self.send_error(HTTPStatus.NOT_FOUND, "not found")

        def log_message(self, format: str, *args: Any) -> None:  # noqa: A003
            return

        def respond_html(
            self,
            compiler_bin: Path,
            cases_dir: Path | None,
            host: str,
            port: int,
        ) -> None:
            try:
                envelope = load_viewer_data(compiler_bin, cases_dir)
                body = render_html(
                    envelope,
                    live_preview=True,
                    server_url=f"http://{host}:{port}/viewer.json",
                ).encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except Exception as exc:  # noqa: BLE001
                self.send_error(HTTPStatus.INTERNAL_SERVER_ERROR, str(exc))

        def respond_json(self, compiler_bin: Path, cases_dir: Path | None) -> None:
            try:
                envelope = load_viewer_data(compiler_bin, cases_dir)
                body = json.dumps(envelope, ensure_ascii=False, indent=2).encode("utf-8")
                self.send_response(HTTPStatus.OK)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Cache-Control", "no-store")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except Exception as exc:  # noqa: BLE001
                self.send_error(HTTPStatus.INTERNAL_SERVER_ERROR, str(exc))

    return ViewerHandler


def load_viewer_data(compiler_bin: Path, cases_dir: Path | None) -> dict[str, Any]:
    command = [str(compiler_bin), "viewer-json"]
    if cases_dir is not None:
        command.append(str(cases_dir))
    completed = subprocess.run(command, capture_output=True, text=True, check=True, cwd=ROOT)
    return json.loads(completed.stdout)


if __name__ == "__main__":
    raise SystemExit(main())
