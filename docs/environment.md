# Encrypted profiles, identities and trust

```sh
pinset env init development
pinset env set API_TOKEN --profile development
pinset env use development
pinset env trust status
pinset exec -- ./project-command
```

Profiles live at `.pinset/env/<profile>.env`; names remain public and each value is independently encrypted with age. Commit the ciphertext and public recipient/grant metadata. Input uses a hidden prompt or stdin. `env list` shows names only; there is no reveal or plaintext import/export command.

Private age identities stay in the system credential store or explicit process `PINSET_IDENTITY`. The home stores public identity metadata only. `env access request` creates a public request for another device; `grant` re-encrypts the profile to include it; `revoke` removes that recipient. Revocation cannot erase a secret already read or copied by a recipient.

Each profile records `owner_recipients` separately from request-ID `grants`; `recipients` must equal their union. Repeating an identical grant is a no-op. Reusing its ID for another key fails. Revoking a grant preserves owners and other grants for the same key. Encryption and input validation finish before a mutation opens a project transaction, so rejected values do not leave recovery state.

Profiles created before this ownership field remain readable. Before editing or changing their access, explicitly confirm the owners and add `owner_recipients = ["age..."]` to the profile block in `.pinset/config.toml`, retain exactly those owners plus current grants in `recipients`, then run `pinset env trust add`. Pinset does not infer ownership from historical recipient lists. All of this metadata is public; private keys never belong in configuration.

`env access request --ci` requires an interactive TTY and displays a private identity once for manual transfer into a platform Secret. It rejects JSON/non-interactive output and never saves that identity in a file. Installation-only GitHub Action does not authorize projects or generate identities.

Profile precedence is explicit command argument, `PINSET_PROFILE`, local default, shared default. `--no-env` or `PINSET_NO_ENV` disables profile resolution. Local selection is `.pinset/local/profile.json`; `env use --project` changes the committed default; reset clears selection.

Trust binds project ID, host/directory identity and the canonical configuration plus encrypted profile bytes. Comments alone do not change canonical configuration; public policy or ciphertext changes do. External changes invalidate trust, and ordinary profile mutations cannot silently renew invalid trust. `env trust add` is the explicit authorization to trust the current contents.

Case-insensitive variable collisions fail. PATH, JAVA_HOME, Python/Go/Rust/Flutter control variables and all Pinset controls cannot be encrypted overrides. Native children do not inherit the private identity. Reports and journals do not contain decrypted values. Explicitly executed user commands can read the supplied variables and affect external systems; Pinset does not sandbox them.
