# Secrets — provisioning and rotation runbook

> Operational runbook for the GitHub Actions secrets this repository's CI consumes. Audience: the maintainer setting up the publish pipeline for the first time, rotating secrets at the scheduled cadence, or responding to a compromise. Day-to-day contributors do not need this doc.

## Scope

This document covers two distinct kinds of credential material:

1. **Volumio vendor signing key.** Ed25519 keypair. The **public half** is committed in this repo at [`keys/vendor-plugin-signing-public.pem`](keys/vendor-plugin-signing-public.pem) and is read by every device that verifies plugins published under this distribution's `com.volumio.*` namespace. The **private half** lives only in the GitHub Actions repository secret `PLUGIN_SIGNING_KEY_PEM` and never leaves the runner.
2. **Artefacts-push token.** Fine-grained GitHub Personal Access Token. Stored as the repository secret `ARTEFACTS_PUSH_TOKEN`. Used by `publish.yml` and `promote.yml` workflows to push signed bytes from this repository's CI into [evo-device-volumio-artefacts](https://github.com/foonerd/evo-device-volumio-artefacts) (the workflows themselves land when the release-plane contract lands in evo-core).

This repository also bundles the [commons signing key public half](keys/commons-plugin-signing-public.pem) so the catalogue can admit `org.evoframework.*` plugins from [evo-plugins-audio](https://github.com/foonerd/evo-plugins-audio). The commons **private** key is **not** held by this distribution; it lives only in the commons repository's secrets. This distribution signs only `com.volumio.*` artefacts; the commons signs only `org.evoframework.*` artefacts.

Both repository secrets are kept distinct (compromising one does not compromise the other; defence in depth) and rotated independently.

## Inventory

| Secret name | Type | Purpose | Used by | Rotation cadence |
|-------------|------|---------|---------|------------------|
| `PLUGIN_SIGNING_KEY_PEM` | Ed25519 PKCS#8 PEM (private key) | Signs Volumio-specific plugin bundles and the release-plane manifest under the `com.volumio.*` namespace | future `publish.yml`, `promote.yml` | 12 months, or on suspected compromise |
| `ARTEFACTS_PUSH_TOKEN` | Fine-grained GitHub PAT | Cross-repo write to `evo-device-volumio-artefacts` | future `publish.yml`, `promote.yml` | 90 days (one calendar quarter) |

## Current key state

> **Deviation from the showcase pattern as of 2026-04-27.** The public-key bytes in [`keys/vendor-plugin-signing-public.pem`](keys/vendor-plugin-signing-public.pem) are currently identical to the public-key bytes of the commons signing key (the same Ed25519 keypair material). This is a single-developer setup convenience and is not the target showcase shape: the vendor key authorises `com.volumio.*`, the commons key authorises `org.evoframework.*`, and they are documented as different actor positions per [`VENDOR_CONTRACT.md`](https://github.com/foonerd/evo-core/blob/main/docs/engineering/VENDOR_CONTRACT.md). The next planned rotation (Step 1) generates a distinct vendor keypair and brings this repository's trust posture in line with the pattern. Until that rotation lands, a compromise of the commons private key would also expose `com.volumio.*` signing capability.

Current public-key fingerprint (SHA256 of the DER-encoded SubjectPublicKeyInfo, recorded in [`keys/vendor-plugin-signing-public.meta.toml`](keys/vendor-plugin-signing-public.meta.toml) once the rotation lands): `9cd7d7381ee7c2b3bfa490b39077afdc925192299dda661ef94dddba71e574da`.

After rotation, the fingerprint will differ from the commons fingerprint by construction.

---

## 1. `PLUGIN_SIGNING_KEY_PEM` (Volumio vendor signing key)

### When to do this

- **First-time setup**: when this distribution is created.
- **Key separation rotation** (current scheduled work): generate a distinct vendor keypair separate from the commons key. See **Current key state** above.
- **Routine rotation**: every 12 months thereafter, or immediately on suspected compromise.

### Step 1.1: Generate the Ed25519 keypair locally

On your workstation, in a directory **outside** any git working tree (no chance of accidentally committing the private key):

```bash
mkdir -p ~/.evo-keys && cd ~/.evo-keys
umask 077                                          # generated files are 0600

openssl genpkey -algorithm Ed25519 -out volumio-vendor-signing-private.pem
openssl pkey -in volumio-vendor-signing-private.pem \
    -pubout -out vendor-plugin-signing-public.pem
```

The private file is now `~/.evo-keys/volumio-vendor-signing-private.pem` (mode 600). Keep it offline; it never goes onto a network share, into a backup, into chat, or into a commit.

### Step 1.2: Compute the DER fingerprint

```bash
openssl pkey -pubin -in vendor-plugin-signing-public.pem -outform DER \
    | sha256sum
```

The output is a 64-character hex string. This is the fingerprint recorded in [`keys/vendor-plugin-signing-public.meta.toml`](keys/vendor-plugin-signing-public.meta.toml) for verification on key rotation.

After the key-separation rotation, this fingerprint must **differ** from the commons fingerprint (`9cd7d7...e574da`). If it matches, the keypair generation step did not produce fresh material — repeat Step 1.1.

### Step 1.3: Commit the public half to this repo

```bash
cp ~/.evo-keys/vendor-plugin-signing-public.pem \
   <path-to>/evo-device-volumio/keys/vendor-plugin-signing-public.pem
```

Update [`keys/vendor-plugin-signing-public.meta.toml`](keys/vendor-plugin-signing-public.meta.toml) with the new fingerprint comment if the key changed. Commit and push.

### Step 1.4: Store the private half as a repo secret

1. Open https://github.com/foonerd/evo-device-volumio/settings/secrets/actions
2. If `PLUGIN_SIGNING_KEY_PEM` already exists, click it and choose **Update secret**. Otherwise click **New repository secret**.
3. **Name**: `PLUGIN_SIGNING_KEY_PEM` (exactly).
4. **Value**: paste the contents of `~/.evo-keys/volumio-vendor-signing-private.pem` — the entire PEM block including the `BEGIN`/`END` lines.
5. Click **Add secret** / **Update secret**.

GitHub never shows the secret again after this; it can only be replaced.

The commons private key is **not** stored on this repository. It lives only on `evo-plugins-audio`'s repo secrets. Do not paste it here.

### Step 1.5: Verify

Trigger a workflow run that exercises signing once volumio's CI workflows are wired against the release-plane contract. Until then, the secret is provisioned but unused; the verification is a manual sign-test against the new public key, performed locally with `openssl pkeyutl -sign` if needed.

### Rotation

Rotate every 12 months. To rotate:

1. Generate a new keypair (Step 1.1) into a fresh directory.
2. Commit the new public key + updated meta.toml fingerprint (Steps 1.2 - 1.3).
3. Update the repo secret with the new private key (Step 1.4 with **Update secret**).
4. Verify (Step 1.5).
5. Securely delete the old private key from your workstation.
6. (Once `RELEASE_PLANE.md` is in place) re-sign the published release-plane manifest with the new key.

If the old key is suspected compromised, **do not rotate gradually**. Generate the new pair, swap immediately, and follow the **Compromise response** below.

### Compromise response

If the private key has leaked or is suspected to have leaked:

1. Rotate immediately (Steps 1.1 - 1.5 with no delay).
2. Add the old public key fingerprint to the revocation list documented at evo-core's `VENDOR_CONTRACT.md`.
3. Notify operators of devices running this distribution so they update their bundled vendor key material.
4. Audit the artefacts repo for unexpected pushes signed with the old key during the suspected-compromise window. Anything signed by the old key after that point should be considered untrusted.

---

## 2. `ARTEFACTS_PUSH_TOKEN` (cross-repo write)

### When to do this

- **First-time setup**: before the publish/promote workflows go live (i.e., now — the secret can be provisioned ahead of the workflows that consume it).
- **Token rotation**: every 90 days.

### Step 2.1: Create a fine-grained PAT

1. Open https://github.com/settings/tokens?type=beta (Settings → Developer settings → Personal access tokens → Fine-grained tokens).
2. Click **Generate new token**.
3. Fill the form:
   - **Token name**: `evo-device-volumio: publish to artefacts (2026-Q2)`. The `(YYYY-QN)` suffix is the rotation generation; future rotations increment to `(2026-Q3)`, `(2026-Q4)`, etc. Audit logs sort by name; consistent suffixing makes the active vs retiring generation obvious.
   - **Description**: `Cross-repo write from evo-device-volumio CI to evo-device-volumio-artefacts. Used by promote.yml and publish.yml. Stored as repo secret ARTEFACTS_PUSH_TOKEN. Rotated quarterly per project policy.`
   - **Expiration**: 90 days.
   - **Resource owner**: `foonerd`.
   - **Repository access**: **Only select repositories** → `foonerd/evo-device-volumio-artefacts`. **Do not select any other repository.**
4. Scroll to **Permissions**. Three groups will be visible:

| Group | Action |
|-------|--------|
| **Repository permissions** (the only group we touch) | Set **Contents = Read and write**. **Metadata = Read-only** is automatically enforced as a dependency once any Repository permission is set. Every other entry in this group (Actions, Administration, Code scanning alerts, Commit statuses, Custom properties, Dependabot ..., Deployments, Discussions, Environments, Issues, Merge queues, Pages, Pull requests, Secret scanning, Secrets, Variables, Webhooks, Workflows, etc.) stays at **No access**. |
| **Account permissions** | Every entry stays at **No access**. |
| **Organization permissions** (only shown if the resource owner is an organisation; not shown for personal accounts) | If shown: every entry stays at **No access**. |

5. Click **Generate token**.
6. **Copy the token** (`github_pat_...`) immediately. GitHub displays it once. Treat as sensitive: no commits, no chat, no log files.

### Step 2.2: Store as a repo secret

1. Open https://github.com/foonerd/evo-device-volumio/settings/secrets/actions
2. Click **New repository secret** (or **Update secret** if rotating).
3. **Name**: `ARTEFACTS_PUSH_TOKEN` (exactly).
4. **Value**: paste the PAT from Step 2.1.
5. Click **Add secret** / **Update secret**.

The settings page now lists two secrets: `PLUGIN_SIGNING_KEY_PEM` and `ARTEFACTS_PUSH_TOKEN`.

### Step 2.3: Verify (read-only smoke test)

```bash
GH_TOKEN=<paste-the-PAT> gh api repos/foonerd/evo-device-volumio-artefacts \
    --jq '.permissions'
# Expected: {"admin":false,"maintain":false,"push":true,"triage":false,"pull":true}
```

`push: true` confirms write access on the scoped repo.

Negative test (the PAT must not have access to anything else):

```bash
GH_TOKEN=<paste-the-PAT> gh api repos/foonerd/evo-plugins-audio-artefacts \
    --jq '.permissions'
# Expected: HTTP 404 (token has no access)
```

The 404 confirms scope isolation.

### Rotation

Rotate every 90 days. Set a calendar reminder for 80 days from issue (10-day buffer).

1. Create a fresh PAT (Step 2.1) with the next-quarter token name (e.g. `evo-device-volumio: publish to artefacts (2026-Q3)`).
2. Update the repo secret with the new PAT value (Step 2.2 with **Update secret**).
3. Verify (Step 2.3).
4. Revoke the old PAT: https://github.com/settings/tokens?type=beta → find the old token by its `(2026-QN)` suffix → **Revoke**.

The publish workflow does not need a restart; the next CI run picks up the new secret value automatically.

### Compromise response

If the PAT has leaked:

1. **Revoke immediately** at https://github.com/settings/tokens?type=beta (single click).
2. Audit the artefacts repository's commit log for unexpected pushes during the suspected-compromise window.
3. Issue a fresh PAT (Step 2.1) and update the secret (Step 2.2). The publish workflow resumes on the next CI run.

A compromised PAT can push unsigned content to the artefacts repository, but devices verify against the vendor signing key before placing — unsigned or wrongly-signed content is rejected at the device. The defence-in-depth pairing means a stolen PAT alone does not break the supply chain.

---

## CI consumption preview

When `publish.yml` is wired (after `RELEASE_PLANE.md` lands in evo-core), the secrets are consumed like this:

```yaml
- name: Sign and push pieces
  env:
    PLUGIN_SIGNING_KEY_PEM: ${{ secrets.PLUGIN_SIGNING_KEY_PEM }}
    GH_TOKEN: ${{ secrets.ARTEFACTS_PUSH_TOKEN }}
  run: |
    set -e
    umask 077
    printenv PLUGIN_SIGNING_KEY_PEM > /tmp/signing.pem

    # ... build pieces, sign with /tmp/signing.pem ...

    git config --global user.name "evo-device-volumio CI"
    git config --global user.email "ci@volumio.org"
    git clone "https://x-access-token:${GH_TOKEN}@github.com/foonerd/evo-device-volumio-artefacts.git" artefacts
    cd artefacts

    # ... copy pieces, update pieces.toml + signature, commit, push ...

    git push origin main

    shred -u /tmp/signing.pem
```

Both secrets are masked in workflow logs by GitHub Actions automatically.

---

## Audit

| Surface | What it shows |
|---------|---------------|
| https://github.com/foonerd/evo-device-volumio/actions | Every workflow run that consumed either secret. Failed runs flag credential issues. |
| https://github.com/settings/security-log | Every PAT use and every PAT lifecycle event (issue, expiration, revocation). |
| https://github.com/foonerd/evo-device-volumio-artefacts/commits | Every push from CI lands here; commit author "evo-device-volumio CI" identifies the publish path. |

Cross-reference all three when investigating any unexpected publish.

---

## Forward-looking

- **Vendor / commons key separation** (scheduled). Per the **Current key state** note above, the vendor key currently shares material with the commons key. The next rotation (Step 1) generates a distinct vendor keypair. Once landed, this Forward-looking entry can be removed.
- **GitHub App migration.** When this repository gains contributors beyond a single maintainer, migrate `ARTEFACTS_PUSH_TOKEN` from a personal access token (tied to a user) to a GitHub App installation token (tied to the organisation). The App is owned by the org, not a person; if a contributor leaves, the publish path keeps working. Workflow consumption shape is unchanged. Migration effort: roughly half a day.
- **Hardware-backed signing key.** When the project's threat model warrants, store the private signing key in a hardware security module (YubiKey, AWS KMS, etc.) and have CI sign via the HSM rather than via a PEM file. Workflow change: the sign step calls `pkcs11-tool` or AWS KMS API instead of `openssl`. The repo secret becomes a token granting signing access, not the key itself.

---

## This document is a worked example

Distributions creating new evo-device-* repositories copy this document with their own repo names, key namespaces, and trust contexts substituted. The two-secret model (signing key + cross-repo write token), the canonical token-name format (`<source-repo>: publish to artefacts (YYYY-QN)`), and the rotation cadences (12 months for signing keys, 90 days for PATs) are project-wide conventions. See [foonerd/evo-plugins-audio/SECRETS.md](https://github.com/foonerd/evo-plugins-audio/blob/main/SECRETS.md) for the parallel document at the plugin commons tier.
