#!/usr/bin/env python3
"""Local transport, offline delivery and report acceptance. Uses only loopback servers."""
import copy
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import ssl
import subprocess
import sys
import tarfile
import tempfile
import threading

binary = Path(sys.argv[1]).resolve(strict=True)


class Handler(BaseHTTPRequestHandler):
    def do_HEAD(self):
        self.server.requests.append(self.path)
        self.send_response(self.server.response_status)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, *_):
        pass


with tempfile.TemporaryDirectory(prefix="pinset-team-") as temporary:
    root = Path(temporary)
    project, home = root / "project", root / "home"
    project.mkdir()
    home.mkdir()
    env = {key: value for key, value in os.environ.items() if key.upper() not in (
        "PINSET_IDENTITY", "PINSET_IDENTITY_FILE", "PINSET_ENV_PROFILE", "PINSET_CA_BUNDLE",
        "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
    )}
    env.update(PINSET_HOME=str(home), PINSET_LANG="en", NO_PROXY="localhost,127.0.0.1",
               HTTP_PROXY="http://127.0.0.1:1", HTTPS_PROXY="http://127.0.0.1:1")

    def run(*args, expected=(0,), extra=None):
        result = subprocess.run([str(binary), *args], cwd=project, env=env | (extra or {}),
                                capture_output=True, text=True, timeout=25)
        assert result.returncode in expected, (args, result.stdout, result.stderr)
        return json.loads(result.stdout) if "--json" in args else result.stdout

    content = b"disposable offline SDK bytes"
    digest = hashlib.sha256(content).hexdigest()
    config = '''schema = 6
project-id = "565652e4-0000-4000-8000-000000000025"
[tools]
node = "24.0.0"
[requirements]
platforms = ["linux-x86_64", "windows-x86_64"]
'''
    (project / "pinset.toml").write_text(config)
    lock = '''schema = 5
generated_by = "loopback acceptance"
[[tool]]
name = "node"
requested = "24.0.0"
version = "24.0.0"
provider = "nodejs-official"
[tool.metadata]
signature_primary_fingerprint = "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356"
signed_manifest = "SHASUMS256.txt.asc"
manifest_source = "official"
'''
    for target, platform, extension in [("linux-x86_64", "linux-x64", "tar.xz"),
                                        ("windows-x86_64", "win-x64", "zip")]:
        archive = f"node-v24.0.0-{platform}"
        lock += f'''[[tool.artifact]]
target = "{target}"
canonical_url = "https://nodejs.org/dist/v24.0.0/{archive}.{extension}"
artifact_path = "v24.0.0/{archive}.{extension}"
sha256 = "{digest}"
format = "{extension}"
archive_root = "{archive}"
verification = "nodejs-openpgp-sha256"
'''
    (project / "pinset.lock").write_text(lock)
    (project / "package.json").write_text('{"engines":{"node":">=30"}}')
    blocked = run("setup", "--plan", "--json", expected=(1,))["data"]
    assert any("compatibility.node.engines" in reason for reason in blocked["blockers"])
    assert not (home / "installs").exists()
    (project / "package.json").unlink()

    def sources(url, fallback=None):
        text = f'''schema = 1
[providers.node]
active = "private-selected"
fallback = {json.dumps(["private-fallback"] if fallback else [])}
[providers.node.sources.private-selected]
base_url = "{url}"
allow_insecure = {str(url.startswith("http:")).lower()}
[providers.node.sources.registered-only]
base_url = "http://127.0.0.1:1/never-query/"
allow_insecure = true
'''
        if fallback:
            text += f'''[providers.node.sources.private-fallback]
base_url = "{fallback}"
allow_insecure = true
'''
        (home / "sources.toml").write_text(text)

    def network(expected=(0,), extra=None):
        result = run("check", "--network", "--target", "linux-x86_64", "--json", expected=expected, extra=extra)["data"]
        encoded = json.dumps(result)
        for private in ("private-selected", "private-fallback", "registered-only", "localhost", str(root)):
            assert private not in encoded, private
        assert result["project_dependencies_verified"] is False
        return result

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.response_status, server.requests = 200, []
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{server.server_port}/"
    try:
        sources(url)
        assert network()["network"]["requests"] == 1
        for status, reason in [(401, "source_authentication_failed"), (407, "proxy_authentication_required"),
                               (404, "artifact_not_found_at_source"), (503, "source_server_failed")]:
            server.response_status = status
            result = network(expected=(1,))
            assert result["network"]["attempts"][0]["reason"] == reason
        server.response_status = 405
        assert network(expected=(1,))["network"]["attempts"][0]["state"] == "unknown"
        server.response_status = 200
        sources("http://127.0.0.1:1/unavailable/", url)
        # Preserve the existing order: selected, automatic official, configured fallback.
        assert network()["network"]["requests"] == 3
    finally:
        server.shutdown()
        server.server_close()
        thread.join()

    certificate, key = root / "ca.pem", root / "server.key"
    ca_key, request, server_certificate = root / "ca.key", root / "server.csr", root / "server.pem"
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                    "-subj", "/CN=Disposable Pinset Test CA", "-addext", "basicConstraints=critical,CA:TRUE",
                    "-keyout", str(ca_key), "-out", str(certificate)], check=True, capture_output=True, timeout=15)
    subprocess.run(["openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost",
                    "-keyout", str(key), "-out", str(request)], check=True, capture_output=True, timeout=15)
    extensions = root / "server.ext"
    extensions.write_text("basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\n")
    subprocess.run(["openssl", "x509", "-req", "-in", str(request), "-CA", str(certificate), "-CAkey", str(ca_key),
                    "-CAcreateserial", "-days", "1", "-extfile", str(extensions), "-out", str(server_certificate)],
                    check=True, capture_output=True, timeout=15)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.response_status, server.requests = 200, []
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(server_certificate, key)
    server.socket = tls.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        sources(f"https://localhost:{server.server_port}/")
        result = network(expected=(1,))
        assert result["network"]["attempts"][0]["reason"] == "tls_or_certificate_verification_failed"
        assert network(extra={"PINSET_CA_BUNDLE": str(certificate)})["passed"] is True
    finally:
        server.shutdown()
        server.server_close()
        thread.join()

    offline = run("check", "--offline", "--json", expected=(1,))["data"]["offline"]
    assert len(offline["artifacts"]) == 2 and offline["network_requests"] == 0
    archive_file = root / "archive"
    archive_file.write_bytes(content)
    run("cache", "import", str(archive_file), "--sha256", digest)
    offline = run("check", "--offline", "--json")["data"]["offline"]
    assert offline["ready"] and not offline["project_dependencies_verified"]
    bundle = root / "valid.bundle.tar.gz"
    run("bundle", "export", "--target", "linux-x86_64", "--output", str(bundle))
    imported_home = root / "valid-import"
    run("bundle", "import", str(bundle), extra={"PINSET_HOME": str(imported_home)})
    assert (imported_home / "downloads" / "sha256" / f"{digest}.archive").read_bytes() == content
    with tarfile.open(bundle) as archive:
        entries = {entry.name: archive.extractfile(entry).read() for entry in archive if entry.isfile()}
    for alteration in ("missing-artifact", "empty-manifest", "extra-file", "corrupt-content"):
        modified = copy.deepcopy(entries)
        artifact = next(name for name in modified if name.startswith("artifacts/"))
        if alteration == "missing-artifact":
            del modified[artifact]
        elif alteration == "empty-manifest":
            manifest = json.loads(modified["manifest.json"])
            manifest["artifacts"] = []
            modified["manifest.json"] = json.dumps(manifest).encode()
        elif alteration == "extra-file":
            modified["undeclared-file"] = b"extra"
        else:
            modified[artifact] = b"changed"
        bad_bundle = root / f"{alteration}.tar.gz"
        with tarfile.open(bad_bundle, "w:gz") as archive:
            for name, data in modified.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        imported_home = root / alteration
        run("bundle", "import", str(bad_bundle), expected=(2,),
            extra={"PINSET_HOME": str(imported_home)})
        assert not (imported_home / "downloads").exists(), alteration
    cache = home / "downloads" / "sha256" / f"{digest}.archive"
    cache.write_bytes(b"corrupted")
    assert run("check", "--offline", "--json", expected=(1,))["data"]["offline"]["ready"] is False
    print("Loopback source failures, trusted CA, complete offline delivery and tampered bundles passed")
