//! Offline trust roots and verification for Node.js clear-signed checksum manifests.

use std::collections::BTreeSet;

use pgp::{
    composed::{CleartextSignedMessage, Deserializable, SignedPublicKey},
    types::KeyDetails,
};

use crate::{Error, Result};

const NODE_RELEASE_KEYS: &str = include_str!("../assets/node-release-keys.asc");
const NODE_RELEASE_KEYS_SOURCE: &str =
    "nodejs/release-keys@b28073028e6d6855cfb53bf7fa0137599c01f967";

// INVARIANT: These primary fingerprints are the trust decision. The armored bundle is only
// accepted when every parsed certificate belongs to this pinned allowlist.
const TRUSTED_PRIMARY_FINGERPRINTS: &[&str] = &[
    "108F52B48DB57BB0CC439B2997B01419BD92F80A",
    "114F43EE0176B71C7BC219DD50A3051F888C628D",
    "141F07595B7B3FFE74309A937405533BE57C7D57",
    "1C050899334244A8AF75E53792EF661D867B9DFA",
    "4ED778F539E3634C779C87C6D7062848A1AB005C",
    "56730D5401028683275BD23C23EFEFE93C4CFFFE",
    "5BE8A3F6C8A5C01D106C0AD820B1A390B168D356",
    "61FC681DFB92A079F1685E77973F295594EC4689",
    "655F3B5C1FB3FA8D1A0CA6BDE4A7D232B936D2FD",
    "71DCFD284A79C3B38668286BC97EC7A07EDE3FC1",
    "74F12602B6F1C4E913FAA37AD3A89613643B6201",
    "77984A986EBC2AA786BC0F66B01FBB92821C587A",
    "7937DFD2AB06298B2293C3187D33FF9D0246406D",
    "890C08DB8579162FEE0DF9DB8BEAB4DFCF555EF4",
    "8FCCA13FEF1D0C2E91008E09770F7A9A5AE15600",
    "93C7E9E91B49E432C2F75674B0A78B0A6C481CF6",
    "94AE36675C464D64BAFA68DD7434390BDBE9B9C5",
    "9554F04D7259F04124DE6B476D5A82AC7E37093B",
    "A363A499291CBBC940DD62E41F10027AF002F8B0",
    "A48C2BEE680E841632CD4E44F07496B3EB3C1762",
    "B9AE9905FFD7803F25714661B63B535A4C206CA9",
    "B9E2F5981AA6E0CD28160D9FF13993A75599653C",
    "C0D6248439F1D5604AAFFB4021D900FFDB233756",
    "C4F0DFFF4E8C1A8236409D08E73BC641CC11F4C8",
    "C82FA3AE1CBEDC6BE46B9360C43CEC45C17AB93C",
    "CC68F5A3106FF448322E48ED27F5E38D5B0A215F",
    "DD792F5973C6DE52C432CBDAC77ABFA00DDBF2B7",
    "DD8F2338BAE7501E3DD5AC78C273792F7D83545D",
    "FD3A5288F042B6850C66B31F09FE44734EB7990E",
];

// A compromised signer is added here in a security release even if its certificate remains in
// the historical bundle. Keeping revocation separate makes an emergency deny decision explicit.
const REVOKED_PRIMARY_FINGERPRINTS: &[&str] = &[];

pub(crate) struct VerifiedNodeManifest {
    pub text: String,
    pub signer_fingerprint: String,
}

pub(crate) fn verify_node_manifest(armored: &str) -> Result<VerifiedNodeManifest> {
    let (message, _) = CleartextSignedMessage::from_string(armored).map_err(|source| {
        Error::NodeSignatureInvalid {
            reason: format!("cannot parse clear-signed manifest: {source}"),
        }
    })?;
    if message.signatures().len() != 1 {
        return Err(Error::NodeSignatureInvalid {
            reason: "manifest must contain exactly one signature".to_owned(),
        });
    }

    let keys = trusted_keys()?;
    for key in &keys {
        let primary_fingerprint = fingerprint(key);
        if REVOKED_PRIMARY_FINGERPRINTS.contains(&primary_fingerprint.as_str()) {
            continue;
        }
        // Key rotation is authorized at the primary-certificate level. A valid signing subkey is
        // accepted only through its allowlisted primary, and the lock records that primary
        // fingerprint so subkey churn does not silently redefine the trust identity.
        if message.verify(&key.primary_key).is_ok()
            || key
                .public_subkeys
                .iter()
                .any(|subkey| message.verify(subkey).is_ok())
        {
            return Ok(VerifiedNodeManifest {
                text: message.signed_text(),
                signer_fingerprint: primary_fingerprint,
            });
        }
    }

    let signature = &message.signatures()[0];
    let trusted_identities = trusted_identities(&keys);
    let issuer_fingerprints = signature
        .issuer_fingerprint()
        .into_iter()
        .map(|value| hex::encode_upper(value.as_bytes()))
        .collect::<Vec<_>>();
    let issuer_ids = signature
        .issuer()
        .into_iter()
        .map(|value| hex::encode_upper(value.as_ref()))
        .collect::<Vec<_>>();
    let has_trusted_candidate = issuer_fingerprints
        .iter()
        .chain(issuer_ids.iter())
        .any(|issuer| trusted_identities.contains(issuer));
    if has_trusted_candidate || (issuer_fingerprints.is_empty() && issuer_ids.is_empty()) {
        return Err(Error::NodeSignatureInvalid {
            reason: "cryptographic verification failed for the declared signer".to_owned(),
        });
    }

    let signer = issuer_fingerprints
        .into_iter()
        .chain(issuer_ids)
        .collect::<Vec<_>>()
        .join(",");
    Err(Error::NodeSignerUntrusted { signer })
}

fn trusted_keys() -> Result<Vec<SignedPublicKey>> {
    let (keys, _) = SignedPublicKey::from_reader_many(NODE_RELEASE_KEYS.as_bytes()).map_err(
        |source| Error::NodeTrustStoreInvalid {
            reason: format!("cannot parse {NODE_RELEASE_KEYS_SOURCE}: {source}"),
        },
    )?;
    let allowlist = TRUSTED_PRIMARY_FINGERPRINTS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut verified = Vec::new();
    for key in keys {
        let key = key.map_err(|source| Error::NodeTrustStoreInvalid {
            reason: format!("cannot parse certificate from {NODE_RELEASE_KEYS_SOURCE}: {source}"),
        })?;
        key.verify()
            .map_err(|source| Error::NodeTrustStoreInvalid {
                reason: format!("certificate self-signature verification failed: {source}"),
            })?;
        let fingerprint = fingerprint(&key);
        if !allowlist.contains(fingerprint.as_str()) || !seen.insert(fingerprint) {
            return Err(Error::NodeTrustStoreInvalid {
                reason: "certificate fingerprint is missing, duplicated, or not allowlisted"
                    .to_owned(),
            });
        }
        verified.push(key);
    }
    if seen.len() != allowlist.len() {
        return Err(Error::NodeTrustStoreInvalid {
            reason: "embedded certificate bundle does not match the complete allowlist".to_owned(),
        });
    }
    Ok(verified)
}

fn fingerprint(key: &SignedPublicKey) -> String {
    hex::encode_upper(key.fingerprint().as_bytes())
}

fn trusted_identities(keys: &[SignedPublicKey]) -> BTreeSet<String> {
    let mut identities = BTreeSet::new();
    for key in keys {
        identities.insert(fingerprint(key));
        identities.insert(hex::encode_upper(key.primary_key.key_id().as_ref()));
        for subkey in &key.public_subkeys {
            identities.insert(hex::encode_upper(subkey.fingerprint().as_bytes()));
            identities.insert(hex::encode_upper(subkey.key_id().as_ref()));
        }
    }
    identities
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFICIAL_MANIFEST: &str =
        include_str!("../tests/fixtures/node-v24.19.0-SHASUMS256.txt.asc");
    const UNTRUSTED_MANIFEST: &str = include_str!("../tests/fixtures/untrusted-cleartext.asc");

    #[test]
    fn verifies_an_official_clear_signed_manifest_before_exposing_checksums() {
        let verified = verify_node_manifest(OFFICIAL_MANIFEST).expect("official signature");
        assert_eq!(verified.signer_fingerprint.len(), 40);
        assert!(verified.text.contains("node-v24.19.0-linux-arm64.tar.xz"));
    }

    #[test]
    fn rejects_tampered_and_untrusted_clear_signed_manifests() {
        let tampered = OFFICIAL_MANIFEST.replacen("node-v24.19.0", "node-v24.19.1", 1);
        assert!(matches!(
            verify_node_manifest(&tampered),
            Err(Error::NodeSignatureInvalid { .. })
        ));
        assert!(matches!(
            verify_node_manifest(UNTRUSTED_MANIFEST),
            Err(Error::NodeSignerUntrusted { .. })
        ));
    }

    #[test]
    fn rejects_missing_and_malformed_signatures() {
        assert!(matches!(
            verify_node_manifest("not a clear-signed manifest"),
            Err(Error::NodeSignatureInvalid { .. })
        ));
        let malformed = OFFICIAL_MANIFEST.replacen("iHUEARYI", "jHUEARYI", 1);
        assert!(matches!(
            verify_node_manifest(&malformed),
            Err(Error::NodeSignatureInvalid { .. })
        ));
    }
}
