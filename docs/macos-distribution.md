# macOS distribution (not currently active)

**There is currently no public AgentDeck release.** AgentDeck is maintained for
its owner's personal use: no GitHub Release exists, nothing is offered for
download, and the release workflow in this repository is never run against real
signing credentials.

This document is dormant material. It records how a signed download *would* be
identified and verified, so that the verification steps do not have to be
reinvented if distribution is ever authorized. Authorizing it requires a
separate Spectra change, which must complete live signing, notarization,
checksum and Gatekeeper acceptance before any artifact becomes public.

Until then, AgentDeck is installed by building it yourself — see
[Personal installation (macOS)](../README.md#personal-installation-macos). That
personal build inherits nothing from the material below.

## What a release would contain

If distribution were authorized, each release would be one GitHub Release
created from a `v<version>` tag, carrying exactly four assets:

| Asset | What it would be |
| ----- | ---------------- |
| `AgentDeck_<version>_aarch64.dmg` | Disk image for Apple silicon Macs |
| `AgentDeck_<version>_aarch64.dmg.sha256` | SHA-256 digest of that disk image |
| `AgentDeck_<version>_x64.dmg` | Disk image for Intel Macs |
| `AgentDeck_<version>_x64.dmg.sha256` | SHA-256 digest of that disk image |

The release workflow only makes a release visible after both architectures were
built, signed, notarized, stapled and verified from the same commit. A release
page missing an architecture or a checksum would not be a complete AgentDeck
release, and nothing should be installed from it.

## Which disk image would apply

Open the Apple menu and choose **About This Mac**:

- **Apple M1 / M2 / M3 / M4** (Apple silicon) — the `aarch64` disk image.
- **Intel Core** — the `x64` disk image.

Taking the wrong architecture is not dangerous; the application simply would not
launch as expected.

## How a download would be verified

You would download the disk image and its `.sha256` file into the same folder,
then recompute and compare:

```bash
cd ~/Downloads
shasum -a 256 -c AgentDeck_1.31.0_aarch64.dmg.sha256
```

A matching file prints `AgentDeck_1.31.0_aarch64.dmg: OK`.

To compare by eye instead, run `shasum -a 256 AgentDeck_1.31.0_aarch64.dmg` and
check the output against the checksum file. Both hold one line: 64 lowercase
hexadecimal characters, two spaces, and the disk image file name.

Differing digests would mean the file is not the file that was released. The
answer is to download it again, and — if a fresh copy still differs — not to
open it and to report the mismatch instead.

## What signing and notarization would mean

Every disk image, and the `AgentDeck.app` inside it, would be:

- signed with a **Developer ID Application** certificate belonging to the
  AgentDeck release identity, with a secure timestamp and the hardened runtime
  enabled;
- submitted to Apple for **notarization**, with the resulting ticket stapled to
  both the application and the disk image, so the check also works offline.

Gatekeeper would then accept such a build on first launch: open the disk image,
drag `AgentDeck.app` into `/Applications`, and start it normally.

A macOS refusal to open a signed download would mean verification failed — a
damaged, incomplete or substituted file. The response is to download it again
and re-check the digest. No macOS security check should ever be turned off to
open it.

## No application auto-update

AgentDeck has no application auto-update, and nothing here would add one. The
running application never queries for releases, never downloads a build and
never installs one. Hosting a signed download is not an update feed: a newer
version would always be fetched and installed by hand.

## If a release ever had to be withdrawn

A release found to be faulty would be turned back into a draft. Its assets stop
being downloadable while the tag and the record of what happened stay in place.
A fix would ship as a new patch version under a new tag — a tag is never
re-pointed, and assets are never overwritten in place.

A copy already installed keeps working, and the releases page would show whether
a replacement version exists.
