# GitHub attestation verification fixture

`reusable-workflow.sigstore.json` is an unmodified public Sigstore bundle from
GitHub CLI's verification test data:

- Source: https://github.com/cli/cli/blob/trunk/pkg/cmd/attestation/test/data/reusable-workflow-attestation.sigstore.json
- Retrieved: 2026-10-23
- SHA256: 49a3aa6075e0f49f82843e74b5baa614ad2a588e6675612bf108a0a008c5ac25
  is the signed artifact subject digest, not the bundle file digest.
- Certificate source repository: `malancas/attest-demo`.
- Certificate signer workflow: `github/artifact-attestations-workflows`.
- License: GitHub CLI MIT License, reproduced in `LICENSE-cli`.

It carries a production Fulcio certificate and public Rekor inclusion evidence.
Tests verify it against the production trust root embedded by sigstore-verify,
without network access or test roots. The different source and signer
repositories exercise the reusable workflow authorization boundary.

`pinset-v3.0.9.sigstore.json` is the unmodified public bundle returned by
https://api.github.com/repos/Future-Element/pinset/attestations/sha256:2bc89cbee5b40191ab37f71914e90c03c5ac304411d52832cbac20ab7fa39de4
on 2026-10-23, serialized as JSON. It attests the real Pinset 3.0.9 Linux x86_64
release archive (8,400,974 bytes):

- Artifact: https://github.com/Future-Element/pinset/releases/download/v3.0.9/pinset-v3.0.9-linux-x86_64.zip
- Artifact SHA256: `2bc89cbee5b40191ab37f71914e90c03c5ac304411d52832cbac20ab7fa39de4`.
- Bundle-file SHA256: `1849eaac4537d17f418fe62acd482cfce7d60241ce9be6febd3f0f578d772ea4`.
- Certificate source repository: `Future-Element/pinset`.
- License: Pinset's MIT License, in the repository root.

`scripts/tests/github_attestation.py` serves this bundle and the unchanged ZIP
through a local verified-TLS fixture. Only transport is redirected; production
Sigstore roots and signature checks remain in force. The archive is downloaded
with its pinned SHA256 or supplied with `--archive`. This acceptance test is
outside the fast contracts suite.
