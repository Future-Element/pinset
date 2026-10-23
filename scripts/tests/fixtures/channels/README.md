# Channel generation fixture

These small fixtures describe the public `Future-Element/pinset` v3.0.9 release used to verify offline channel generation. `release-metadata.json` retains only the fields consumed by the generator: release identity/stability plus the four ZIP assets and SHA256SUMS URL/digest. It is approximately 1.5 KiB rather than a copy of the full release response.

`SHA256SUMS` is the original 955-byte public checksum asset, retained byte for byte so its own published GitHub asset digest still matches. It includes the release's other small entries as well as the four ZIP hashes. `release.json` pins this test independently of future changes to the repository's current release marker.

The original public ZIPs were downloaded, rehashed and inspected during channel preparation; that live check is separate from these offline tests. The generator test makes network requests fail immediately and exercises successful generation, checksum corruption and a missing platform asset. It does not install Homebrew, Scoop, WinGet or aqua packages and does not establish registry acceptance.
