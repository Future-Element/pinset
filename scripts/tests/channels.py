"""Offline channel generation and failure-atomicity contracts; no installations."""
from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
from argparse import Namespace
from pathlib import Path


def verify_channels(repository: Path | None = None) -> dict:
    repository = repository or Path(__file__).resolve().parents[2]
    source = repository / "scripts/generate_channels.py"
    fixtures = repository / "scripts/tests/fixtures/channels"
    spec = importlib.util.spec_from_file_location("pinset_channel_generator", source)
    generator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generator)

    def forbid_network(*_args, **_kwargs):
        raise AssertionError("offline channel generation attempted a network request")

    generator.download = forbid_network

    def arguments(destination: Path, metadata: Path | None = None, checksums: Path | None = None):
        return Namespace(
            release_json=fixtures / "release.json",
            release_metadata=metadata or fixtures / "release-metadata.json",
            checksums=checksums or fixtures / "SHA256SUMS",
            output_root=destination,
            verify_archives=False,
        )

    def manifests(destination: Path) -> dict[str, bytes]:
        return {
            path.relative_to(destination).as_posix(): path.read_bytes()
            for path in (destination / "packaging").rglob("*")
            if path.is_file()
        }

    def reject(destination: Path, **inputs):
        try:
            generator.generate(arguments(destination, **inputs))
        except ValueError as error:
            assert not destination.exists(), "a failed release check wrote partial manifests"
            return str(error)
        raise AssertionError("channel generation accepted invalid release inputs")

    checks = []
    with tempfile.TemporaryDirectory(prefix="pinset-channels-") as temporary:
        temporary = Path(temporary)
        first, second = temporary / "first", temporary / "second"
        report = generator.generate(arguments(first))
        generator.generate(arguments(second))
        assert report["version"] == "3.0.9"
        generated = manifests(first)
        assert len(generated) == 7 and generated == manifests(second)
        scoop = json.loads(generated["packaging/scoop/pinset.json"])
        assert scoop["checkver"] == {"github": "https://github.com/Future-Element/pinset"}
        assert scoop["architecture"]["64bit"]["url"].endswith("/pinset-v3.0.9-windows-x86_64.zip")
        assert scoop["architecture"]["64bit"]["hash"] == "f26ed89fcdf060447d6b7721f54f2ab7ce612682967ab080cc1a7574999c4c1a"
        checks.append("fixed published inputs generate seven reproducible channel manifests without network")

        changed_checksums = temporary / "SHA256SUMS.changed"
        checksum_bytes = (fixtures / "SHA256SUMS").read_bytes()
        changed_checksums.write_bytes(bytes([ord("0") if checksum_bytes[0] != ord("0") else ord("1")]) + checksum_bytes[1:])
        assert "SHA256SUMS does not match" in reject(temporary / "reject-checksum", checksums=changed_checksums)
        checks.append("tampered SHA256SUMS is rejected before any output is written")

        metadata = json.loads((fixtures / "release-metadata.json").read_text(encoding="utf-8"))
        missing = copy.deepcopy(metadata)
        missing["assets"] = [asset for asset in missing["assets"] if asset["name"] != "pinset-v3.0.9-windows-x86_64.zip"]
        missing_file = temporary / "release-metadata.missing.json"
        missing_file.write_text(json.dumps(missing), encoding="utf-8")
        assert "metadata and SHA256SUMS disagree" in reject(temporary / "reject-platform", metadata=missing_file)
        checks.append("a missing published platform is rejected before any output is written")

    return {"passed": True, "checks": checks, "scope": "offline generator only; no package-manager installation or publication"}


if __name__ == "__main__":
    print(json.dumps(verify_channels(), indent=2))
