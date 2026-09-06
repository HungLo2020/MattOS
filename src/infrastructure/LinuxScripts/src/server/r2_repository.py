"""Cloudflare R2 publisher used by the home-server repository service."""

from __future__ import annotations

import hashlib
import json
import os
import time
from pathlib import Path
from typing import Any


LOCK_KEY = "._mattos_repository_lock.json"


class R2Error(RuntimeError):
    pass


def fields(item: dict[str, Any]) -> dict[str, str]:
    return {str(f["name"]): str(f["value"]) for f in item.get("fields", []) or [] if isinstance(f, dict) and f.get("name") and f.get("value") is not None}


class R2Publisher:
    def __init__(self, config: Any, bitwarden: Any) -> None:
        try:
            import boto3
        except ImportError as exc:
            raise R2Error("boto3 is required on the repository server") from exc
        cache_path = (config.credentials_file or config.root / "r2-credentials.json").expanduser()
        cached: dict[str, str] = {}
        if cache_path.is_file() and os.environ.get(f"{config.repository.upper()}_R2_REFRESH_CREDENTIALS") != "1":
            try:
                payload = json.loads(cache_path.read_text(encoding="utf-8"))
                if isinstance(payload, dict):
                    cached = {str(key): str(value) for key, value in payload.items()}
            except (OSError, json.JSONDecodeError):
                cached = {}

        def check_destination(bucket: str, public_url: str, endpoint: str) -> None:
            if bucket != config.bucket or public_url.rstrip("/") != config.public_url.rstrip("/"):
                raise R2Error(f"R2 credentials do not match the configured destination for {config.repository}; refusing to publish")
            if config.endpoint and endpoint.rstrip("/") != config.endpoint.rstrip("/"):
                raise R2Error(f"R2 credentials do not match the configured endpoint for {config.repository}")

        if cached.get("access_key") and cached.get("secret_key") and cached.get("endpoint") and cached.get("bucket"):
            access = cached["access_key"]
            secret = cached["secret_key"]
            endpoint = cached["endpoint"]
            bucket = cached["bucket"]
            public_url = cached.get("public_url", config.public_url).rstrip("/")
            check_destination(bucket, public_url, endpoint)
        else:
            item = bitwarden.item(config.r2_item)
            login = item.get("login") or {}
            custom = fields(item)
            access = str(login.get("username") or "")
            secret = str(login.get("password") or "")
            endpoint = config.endpoint or custom.get("R2_ENDPOINT", "")
            bucket = custom.get("R2_BUCKET_NAME", config.bucket)
            public_url = custom.get("R2_PUBLIC_URL", config.public_url).rstrip("/")
            check_destination(bucket, public_url, endpoint)
            if access and secret and endpoint and bucket:
                cache_path.parent.mkdir(parents=True, exist_ok=True)
                with cache_path.open("w", encoding="utf-8") as handle:
                    os.fchmod(handle.fileno(), 0o600)
                    handle.write(json.dumps({"access_key": access, "secret_key": secret, "endpoint": endpoint, "bucket": bucket, "public_url": public_url}, indent=2) + "\n")
        if not access or not secret or not endpoint or not bucket:
            raise R2Error("R2 Bitwarden item is missing credentials, endpoint, or bucket")
        self.bucket = config.bucket
        self.public_url = config.public_url
        self.client = boto3.client("s3", endpoint_url=endpoint, aws_access_key_id=access, aws_secret_access_key=secret, region_name="auto")

    def call(self, method: str, **kwargs: Any) -> Any:
        last: Exception | None = None
        for attempt in range(4):
            try:
                body = kwargs.get("Body")
                if hasattr(body, "seek"):
                    body.seek(0)
                return getattr(self.client, method)(**kwargs)
            except Exception as exc:
                last = exc
                if attempt < 3:
                    time.sleep(0.5 * (2 ** attempt))
        raise R2Error(f"R2 operation {method} failed") from last

    def keys(self) -> set[str]:
        result: set[str] = set()
        token = None
        while True:
            args: dict[str, Any] = {"Bucket": self.bucket}
            if token:
                args["ContinuationToken"] = token
            page = self.call("list_objects_v2", **args)
            result.update(str(item["Key"]) for item in page.get("Contents", []) if str(item.get("Key", "")).startswith(("dists/", "pool/")))
            if not page.get("IsTruncated"):
                return result
            token = page.get("NextContinuationToken")

    def download(self, key: str, destination: Path) -> None:
        destination.parent.mkdir(parents=True, exist_ok=True)
        self.call("download_file", Bucket=self.bucket, Key=key, Filename=str(destination))

    def lock(self) -> str:
        owner = hashlib.sha256(f"{os.getpid()}:{time.time_ns()}".encode()).hexdigest()
        try:
            self.call("put_object", Bucket=self.bucket, Key=LOCK_KEY, Body=json.dumps({"owner": owner, "created": time.time()}).encode(), ContentType="application/json", IfNoneMatch="*")
        except R2Error as exc:
            raise R2Error("Cloudflare repository lock is already held") from exc
        return owner

    def unlock(self, owner: str) -> None:
        try:
            self.call("delete_object", Bucket=self.bucket, Key=LOCK_KEY)
        except R2Error:
            pass

    @staticmethod
    def digest(path: Path) -> str:
        h = hashlib.sha256()
        with path.open("rb") as handle:
            for block in iter(lambda: handle.read(1024 * 1024), b""):
                h.update(block)
        return h.hexdigest()

    def publish(self, root: Path, old_keys: set[str]) -> None:
        local = {path.relative_to(root).as_posix(): path for prefix in ("dists", "pool") for path in (root / prefix).rglob("*") if path.is_file()}
        changed = []
        for key, path in local.items():
            if key not in old_keys:
                changed.append(key); continue
            try:
                if self.call("head_object", Bucket=self.bucket, Key=key).get("Metadata", {}).get("sha256") != self.digest(path):
                    changed.append(key)
            except R2Error:
                changed.append(key)
        stale = old_keys - set(local)

        def upload(key: str) -> None:
            path = local[key]
            content = "application/octet-stream"
            cache = "no-cache, max-age=0, must-revalidate"
            if key.startswith("dists/"):
                content = "text/plain; charset=utf-8"
            elif key.endswith(".deb"):
                content = "application/vnd.debian.binary-package"; cache = "public, max-age=31536000, immutable"
            elif key.endswith(".gz"):
                content = "application/gzip"
            with path.open("rb") as body:
                self.call("put_object", Bucket=self.bucket, Key=key, Body=body, Metadata={"sha256": self.digest(path)}, ContentType=content, CacheControl=cache)

        for key in sorted((k for k in changed if k.startswith("pool/"))):
            upload(key)
        for key in sorted((k for k in changed if k.startswith("dists/")), key=lambda k: (k.endswith("InRelease"), k.endswith("Release.gpg"), k)):
            upload(key)
        for key in sorted(stale):
            self.call("delete_object", Bucket=self.bucket, Key=key)
