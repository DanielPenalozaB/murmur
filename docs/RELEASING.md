# Releasing Murmur

Pushing a `v*` tag runs the Release workflow
([.github/workflows/release.yml](../.github/workflows/release.yml)), which publishes a GitHub
Release with:

- `Murmur_<version>_aarch64.dmg` and `.app.zip` for first installs;
- `Murmur_<version>_aarch64.app.tar.gz` and its `.sig`, the update the app installs itself;
- `latest.json`, which installed copies read from
  `releases/latest/download/latest.json` to find that update.

## Cutting a release

1. Set the same version in `src-tauri/tauri.conf.json`, `Cargo.toml` and `package.json`,
   and merge it.
2. `git tag v0.3.0 && git push origin v0.3.0`.

Installed copies offer the update in the sidebar within a few hours, or right away from
Settings → Updates → Check for updates.

## Two keys, both kept outside the repo

**The code-signing certificate** (`Murmur Self-Signed`) gives every release the same
identity, so macOS keeps Murmur's microphone and system-audio permissions across
updates. It's self-signed, not from Apple, so a first install still needs "Open
Anyway" (an Apple Developer ID and notarization would remove that).

**The updater key** signs each update. The app only installs updates signed with it;
its public half is `plugins.updater.pubkey` in `tauri.conf.json`.

Back up both, with their passwords, somewhere safe such as a password manager. Losing
the updater key means installed copies can't update any more: everyone would have to
reinstall by hand a build with a new public key. Losing the certificate means one more
round of permission prompts after the next update.

### Repository secrets

| Secret | Value |
|---|---|
| `MACOS_CERTIFICATE` | the `.p12` file, base64 (`base64 -i signing.p12`) |
| `MACOS_CERTIFICATE_PASSWORD` | the `.p12` password |
| `TAURI_SIGNING_PRIVATE_KEY` | the contents of the updater private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |

A tag build fails without them; pull request builds skip signing.

### Making new ones

Only if they're lost; every installed copy depends on the current ones.

```sh
# Updater key: then put the new public key in tauri.conf.json.
pnpm tauri signer generate -w updater.key

# Certificate: the name must stay "Murmur Self-Signed" (the workflow signs with it).
cat > signing.cnf <<'EOF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = Murmur Self-Signed
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF
openssl req -x509 -newkey rsa:3072 -nodes -keyout signing-key.pem -out signing-cert.pem \
  -days 7300 -config signing.cnf
openssl pkcs12 -export -legacy -inkey signing-key.pem -in signing-cert.pem \
  -name "Murmur Self-Signed" -out signing.p12
```
