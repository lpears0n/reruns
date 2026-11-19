# Releasing

How a release is made, and the one-time setup that turns on updates and
Windows code signing. Until that setup is done, releases build exactly as
before: unsigned installers, and apps that never look for updates.

## Making a release

1. Bump the version in three places to the same number:
   `Cargo.toml` (`[workspace.package] version`), `app/package.json` and
   `app/src-tauri/tauri.conf.json`. The Release workflow refuses a tag
   that doesn't match this version.
2. Write the notes in `docs/release-notes/v<version>.md`; they become the
   draft release's description.
3. Commit, tag and push from your machine:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

4. The Release workflow builds the Windows and Linux installers and puts
   them in a **draft** release. Write the notes and publish it. On Windows,
   `Reruns-Setup-<version>.exe` is the one to download: Reruns' own setup
   window (`app/setup`), which runs the NSIS installer it carries silently.
   That NSIS installer (`Reruns_<version>_x64-setup.exe`) stays on the
   release because the updater downloads it.

Installed copies find a published release the next time they start, or
within six hours while running, or straight away with Settings → General
→ Check now. A tag with a dash
(`v0.2.0-beta.1`) is published as a pre-release. Apps don't update to
pre-releases, because they only read the latest full release.

## Updates (one-time setup)

Every update is signed with a key that only you hold. The app checks the
signature against the public half, which is built into it. The private key
never goes into the repository.

1. On your machine, from `app/`, make the key pair and pick a password:

   ```bash
   pnpm tauri signer generate -w ~/.tauri/rerunsctl.key
   ```

   This writes `~/.tauri/rerunsctl.key` (private) and `~/.tauri/rerunsctl.key.pub`
   (public). Back the private key up somewhere safe. **If it is lost,
   installed copies can never update again**, and everyone has to reinstall
   by hand.

2. In the repository's Settings → Secrets and variables → Actions:

   | Kind | Name | Value |
   | --- | --- | --- |
   | Secret | `TAURI_SIGNING_PRIVATE_KEY` | the contents of `rerunsctl.key` |
   | Secret | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |
   | Variable | `UPDATER_PUBKEY` | the contents of `rerunsctl.key.pub` |

The next tagged release then also:

- signs the Windows installer and the AppImage for updating;
- adds `latest.json` to the release, which is what the apps read.

The first release with updates on has to be installed by hand. Builds
from before it can't update.

How the app uses it:

- **Checks:** one small request as it starts, then every six hours. A check
  that fails, such as at login before the network is up, is tried again a
  minute later, then less often. "Check automatically" in Settings turns
  this off.
- **Downloads:** nothing downloads until you press **Update and restart**,
  so an update never competes with a game for bandwidth.
- **Installing:** Windows runs the installer in passive mode, which shows
  progress and asks nothing. The AppImage replaces itself. The `.deb` and
  `.rpm` packages don't self-update; they get new versions from the
  releases page.

The workflow writes all of this into a release-only config
(`scripts/release-config.mjs`). A build without the secrets has no updater
key, so the app turns its updater off ("This build can't update itself").

## Windows code signing (one-time setup)

Unsigned installers work, but SmartScreen warns about them ("Windows
protected your PC"). Signing removes the "unknown publisher" part of the
warning. SmartScreen reputation still builds up over the first downloads,
even with an EV certificate.

Since 2023, code signing keys must live in hardware or a cloud HSM, so a
certificate usually can't be exported as a file. Two routes are wired up;
set up one.

### Azure Trusted Signing (cheapest)

A Microsoft-run signing service, about $10 a month, for individuals and
companies in the regions it supports. Microsoft verifies your identity.

1. Create a Trusted Signing account and a certificate profile in the Azure
   portal. Then create an app registration that has the "Trusted Signing
   Certificate Profile Signer" role on the profile.
2. Add secrets `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` and
   `AZURE_TENANT_ID` from that app registration.
3. Add a variable `WINDOWS_SIGN_COMMAND`. Use your account's endpoint,
   account name and profile name:

   ```
   trusted-signing-cli -e https://eus.codesigning.azure.net -a <account> -c <profile> -d Reruns %1
   ```

The workflow installs `trusted-signing-cli` when the command starts with
it. Tauri runs the command on every file it signs, with `%1` replaced by
the file's path.

### A certificate file (`.pfx`)

Use this for an older certificate you can still export, or for a
self-signed one while testing.

1. Encode the file as base64, e.g. in PowerShell:
   `[Convert]::ToBase64String([IO.File]::ReadAllBytes('cert.pfx'))`.
2. Add secrets `WINDOWS_CERTIFICATE` (the base64 text) and
   `WINDOWS_CERTIFICATE_PASSWORD`.
3. Optional: add a variable `WINDOWS_TIMESTAMP_URL`. The default is
   DigiCert's timestamp server.

The workflow imports the certificate on the build machine and signs with
its thumbprint. When `WINDOWS_SIGN_COMMAND` is also set, the command wins.

## What lives where

| What | Where | Who holds it |
| --- | --- | --- |
| Updater private key | your machine, plus a backup; a repository secret | you |
| Updater public key | repository variable; built into release apps | public |
| Signing credentials | repository secrets | you |
| Release config | written by the workflow at build time, ignored by git | — |
