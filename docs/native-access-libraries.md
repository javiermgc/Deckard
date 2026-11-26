# Native Access libraries

Deckard discovers **installed** Native Access content and builds its own browser index. Maschine and `komplete.db3` are optional. There is no Native Access login, catalog download, Wine subprocess, or plug-in instantiation during discovery/indexing.

Open **Preferences → Library** to see discovered products, their availability, and refresh status. **Refresh** discovers newly installed or moved products and indexes them in the background. Opening Browser also checks for changes (debounced for five seconds). There is no filesystem watcher.

- **Windows:** read-only `HKLM\SOFTWARE\Native Instruments` registrations in both registry views, using `ContentDir` and optional `ContentVersion`. Executable-only `InstallDir` is not a content library.
- **macOS:** system `/Library/Preferences/com.native-instruments.*.plist` registrations (XML or binary). HFS paths preserve the named volume; Deckard does not guess that an external volume is the startup disk.
- **Linux:** read-only Wine `system.reg`. Prefix priority is explicit paths in Preferences, `WINEPREFIX`, prefixes identified from yabridgectl's `plugin_dirs`, then `~/.wine`. Drive letters use that prefix's `dosdevices` mappings. Ambiguous case matches and unmapped drives are reported, not guessed. No recursive search of the home directory is performed.

“Additional library folders” remains available for other sample packs. A parent folder overlapping an installed expansion does not create duplicate entries or replace its registered product name with “Libraries”. User-saved entries retain their User classification. Favorites use product-relative identities so a library can move without losing them.

A missing disk or unreadable source does not erase cached native metadata or favorites. Successful per-product scans publish atomically. Queries run between bounded scan batches, and a newer refresh cancels unfinished work. Refresh completion and issues appear in Preferences; “refreshed” is not reported before scanning finishes.

## Metadata and resources

Supported inventory: Maschine projects/groups/sounds/instruments/effects; WAV, AIFF/AIFC and MP3 samples; NKS instrument/effect presets; NKI, NKSN, NMSV, NFM8, NRKT, ENS and RKPLR files. Native formats without a portable metadata reader use a filename-based entry under the registered product. Battery NBKT and packed archives are not indexed in this release; internal ensemble snapshots are not independently enumerated.

Deckard reads portable SoundInfo and NKS metadata for names, banks, vendor/author, types, characters, kind and tempo. Embedded valid kind/tempo take precedence over filename heuristics. NKS fields take precedence over NISound fallback fields. Audio and plug-in state are skipped; metadata blocks are limited to 4 MiB and total metadata to 16 MiB. Damaged metadata retains an inventory entry and produces a diagnostic.

Artwork is discovered from `DECKARD_NI_IMAGE_DIR`, product-local `PAResources/image`, and installation-scoped shared NI Resources. Resource discovery and PNG decoding run on the catalog worker. Adjacent `.previews` are preferred; a registered Native Browser Preview Library can supply previews when an existing index provides a reliable preview subpath/UUID. No preview IDs are guessed.

Browser type/character facets feed the existing MK3 touch overlays. Discovery does not add new Kontakt/Reaktor playback support or decrypt protected content.

## Developer verification

Synthetic fixtures contain no proprietary database, library content, registry dumps, credentials or serials:

```
cargo test -p deckard-ni-content
cargo test -p deckard-library
cargo test -p deckard ni_library_tests
cargo test -p deckard-engine catalog_tests
```

Opt-in read-only checks on a consenting installation:

```
cargo run -p deckard-ni-content --example catalog -- /optional/wine/prefix
cargo run -p deckard-library --example native_catalog -- /optional/wine/prefix
```

The second command uses a temporary Deckard index and never changes the user's index or NI files. Compare names, kinds, tags and counts with a reference Maschine database if available; do not use that database as an installation prerequisite. Native Windows/macOS live runtime validation still needs those platforms; synthetic tests and Windows cross-compilation do not substitute for it. See [evidence](native-access-libraries-evidence.md).
