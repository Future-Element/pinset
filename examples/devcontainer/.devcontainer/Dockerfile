FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

ARG PINSET_VERSION=2.1.0

RUN set -eux; \
    architecture="$(dpkg --print-architecture)"; \
    case "${architecture}" in \
      amd64) archive="pinset-linux-x86_64.tar.gz" ;; \
      arm64) archive="pinset-linux-aarch64.tar.gz" ;; \
      *) echo "Unsupported architecture: ${architecture}" >&2; exit 1 ;; \
    esac; \
    release="https://github.com/Future-Element/pinset/releases/download/v${PINSET_VERSION}"; \
    curl --fail --location --silent --show-error "${release}/${archive}" --output "/tmp/${archive}"; \
    curl --fail --location --silent --show-error "${release}/SHA256SUMS" --output /tmp/SHA256SUMS; \
    expected="$(awk -v name="${archive}" '$2 == name { print $1 }' /tmp/SHA256SUMS)"; \
    actual="$(sha256sum "/tmp/${archive}" | awk '{ print $1 }')"; \
    test -n "${expected}"; \
    test "${actual}" = "${expected}"; \
    tar -xzf "/tmp/${archive}" -C /usr/local/bin pinset pinset-shim; \
    rm -f "/tmp/${archive}" /tmp/SHA256SUMS
