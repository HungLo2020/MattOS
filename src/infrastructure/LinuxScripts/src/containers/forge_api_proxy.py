"""A1111-compatible API shim applying model-specific Forge settings for Open WebUI."""

from __future__ import annotations

import http.client
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit


UPSTREAM_HOST = "127.0.0.1"
UPSTREAM_PORT = 7860
API_PORT = 7862
REALVIS_TOKEN = "RealVisXL_V5.0_fp16.safetensors"
FLUX_TOKEN = "flux1-dev-Q5_1.gguf"
FLUX_MODULES = [
    "/app/models/VAE/ae.safetensors",
    "/app/models/text_encoder/clip_l.safetensors",
    "/app/models/text_encoder/t5xxl_fp16.safetensors",
]
REALVIS_NEGATIVE = "bad hands, bad anatomy, ugly, deformed, asymmetrical face, asymmetrical eyes, deformed eyes, deformed mouth, open mouth"
active_checkpoint = ""
state_lock = threading.Lock()


def is_flux(checkpoint: str) -> bool:
    return FLUX_TOKEN.lower() in checkpoint.lower()


def normalize_checkpoint(checkpoint: str) -> str:
    """Accept Open WebUI's model title, which may append a short hash."""

    for filename in (FLUX_TOKEN, REALVIS_TOKEN):
        if checkpoint.lower().startswith(filename.lower()):
            return filename
    return checkpoint


def options_for_checkpoint(payload: dict, checkpoint: str) -> dict:
    """Apply the matching Forge preset, CPU swap and model-specific components."""

    checkpoint = normalize_checkpoint(checkpoint)
    payload["sd_model_checkpoint"] = checkpoint
    payload["forge_unet_storage_dtype"] = "Automatic"
    payload["forge_inference_memory"] = 2048
    payload["forge_async_loading"] = "Queue"
    payload["forge_pin_shared_memory"] = "CPU"
    if is_flux(checkpoint):
        payload["forge_preset"] = "flux"
        payload["forge_additional_modules"] = FLUX_MODULES
    else:
        payload["forge_preset"] = "xl"
        payload["forge_additional_modules"] = []
    return payload


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args) -> None:
        print(f"[Forge API bridge] {self.address_string()} {format % args}", flush=True)

    def do_GET(self) -> None:
        self.forward()

    def do_POST(self) -> None:
        self.forward()

    def do_PUT(self) -> None:
        self.forward()

    def do_DELETE(self) -> None:
        self.forward()

    def do_OPTIONS(self) -> None:
        self.forward()

    def forward(self) -> None:
        global active_checkpoint

        raw_body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        path = urlsplit(self.path).path
        payload = None
        if raw_body and self.headers.get("Content-Type", "").startswith("application/json"):
            try:
                payload = json.loads(raw_body)
            except json.JSONDecodeError:
                payload = None

        with state_lock:
            checkpoint = active_checkpoint
            if self.command == "POST" and path.endswith("/sdapi/v1/options") and isinstance(payload, dict):
                checkpoint = normalize_checkpoint(str(payload.get("sd_model_checkpoint") or checkpoint))
                payload = options_for_checkpoint(payload, checkpoint)
                active_checkpoint = checkpoint
                raw_body = json.dumps(payload).encode("utf-8")
            elif self.command == "POST" and path.endswith("/sdapi/v1/txt2img") and isinstance(payload, dict):
                override = payload.get("override_settings") or {}
                checkpoint = normalize_checkpoint(str(override.get("sd_model_checkpoint") or checkpoint))
                payload["steps"] = 32 if not is_flux(checkpoint) else 30
                payload["sampler_name"] = "DPM++ SDE" if not is_flux(checkpoint) else "Euler"
                payload["scheduler"] = "Karras" if not is_flux(checkpoint) else "Simple"
                payload["cfg_scale"] = 5.5 if not is_flux(checkpoint) else 1.0
                if is_flux(checkpoint):
                    payload["distilled_cfg_scale"] = 3.5
                    payload["negative_prompt"] = ""
                elif not payload.get("negative_prompt"):
                    payload["negative_prompt"] = REALVIS_NEGATIVE
                payload.setdefault("width", 1024)
                payload.setdefault("height", 1024)
                raw_body = json.dumps(payload).encode("utf-8")

        headers = {
            key: value
            for key, value in self.headers.items()
            if key.lower() not in {"host", "connection", "content-length", "transfer-encoding"}
        }
        if raw_body:
            headers["Content-Length"] = str(len(raw_body))
        upstream = http.client.HTTPConnection(UPSTREAM_HOST, UPSTREAM_PORT, timeout=1800)
        try:
            upstream.request(self.command, self.path, body=raw_body or None, headers=headers)
            response = upstream.getresponse()
            body = response.read()
            self.send_response(response.status, response.reason)
            for key, value in response.getheaders():
                if key.lower() not in {"connection", "content-length", "transfer-encoding", "date", "server"}:
                    self.send_header(key, value)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)
            if response.status >= 400:
                print(f"[Forge API bridge] upstream returned HTTP {response.status} for {self.path}", flush=True)
            elif self.command == "POST" and path.endswith("/sdapi/v1/txt2img"):
                model = "FLUX.1-dev Q5_1" if is_flux(checkpoint) else "RealVisXL V5.0"
                print(f"[Forge API bridge] {model}: {payload['steps']} steps at {payload['width']}x{payload['height']}", flush=True)
        except Exception as error:
            message = json.dumps({"detail": f"Forge backend unavailable: {error}"}).encode("utf-8")
            self.send_response(502)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(message)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(message)
        finally:
            upstream.close()


if __name__ == "__main__":
    print(f"Forge API bridge listening on 0.0.0.0:{API_PORT}; upstream {UPSTREAM_HOST}:{UPSTREAM_PORT}", flush=True)
    ThreadingHTTPServer(("0.0.0.0", API_PORT), Handler).serve_forever()
