# Native Access library discovery and indexing

Status: proposed design accompanying the requested implementation plan; no implementation authorized by this document.

## Intent and agreed scope

Discover and index locally installed Native Access libraries on Windows, macOS,
and Linux through Wine, without requiring Maschine to be installed, run, or to
generate a database. Native Access's installed-product registrations identify the
content; Deckard owns the resulting searchable index. Native Access need not be
running. This is a cross-platform feature, not a Wine-specific workaround.

The user wants products such as Rhythm Source to appear automatically with their
actual names, artwork, available Type/Sub-type/Character tags, and previews. The
existing controller tag overlays should receive those facets. Manual library
folders remain a separate source for other content and existing configurations.

## Evidence and present behavior

Inspected checkout: `07c719d`-based running build and the current `Deckard` checkout
on 2026-11-26. Recheck the exact execution baseline before implementing.

- `crates/library/src/ni.rs::default_path` searches native Maschine 3/2 databases
  on Windows/macOS and returns `None` on Linux. Its factory fallback scanner still
  depends on roots registered in that database.
- `apps/viewer/src/main.rs::library_roots` assigns each manually configured root's
  basename as its product. Adding a parent `Libraries` folder therefore collapses
  multiple products under that name.
- `crates/library/src/user.rs` indexes Maschine formats and supported audio, with
  NKS reading enabled only for user roots. It ignores embedded sample tags and
  skips unchanged files using mtime alone.
- `crates/engine/src/library.rs` already reads Windows NI `ContentDir` registrations
  and macOS product plists for sample resolution. This is not browser discovery.
- The user's custom Wine prefix registers Rhythm Source, Studio Drummer, and TRK-01
  under `Software\\Native Instruments`, each with `ContentDir` and `ContentVersion`.
  `Z:` maps to `/`; yabridgectl's configured plug-in directory identifies the prefix.
- Rhythm Source supplies artwork under `PAResources/image/Rhythm Source`. An
  inspected WAV contains ID3v2.4 GEOB records named
  `com.native-instruments.nks.soundinfo` and
  `com.native-instruments.nisound.soundinfo`. They carry product/name/type metadata,
  including `Percussion / Other`; the second payload resembles the existing
  `mxprj::sound_info::SoundInfo` format. Parser compatibility must be tested.
- The current local index contains 12 projects, 51 groups, 62 sounds, 424 loops,
  and 683 one-shots, all labeled `Libraries`. The Maschine files have Type tags;
  the audio rows do not. These counts describe the inspected installation, not
  constants to hardcode into production or CI.
- Product-category databases under `PAResources/database/PAL` are not per-sample
  tag databases. The inspected sample tables were empty. Do not infer sample tags
  from them or from folder names.

Public corroboration: NI's [macOS uninstall documentation](https://support.native-instruments.com/support/solutions/articles/69000879356-how-to-uninstall-native-instruments-software-from-a-mac-computer)
identifies system product plists and Service Center records; its [legacy Native
Access reinstall instructions](https://support.native-instruments.com/support/solutions/articles/69000879347-reinstalling-a-product-with-native-access-on-os-x-10-9)
connect system product plists with installed status. These corroborate the discovery
family, not a stable public Native Access API. Windows/Wine fields above are grounded
in Deckard's existing resolver and the observed local registry. Native Windows/macOS
installation validation remains a release requirement.

## Approaches and decision

1. Reuse Maschine's database inside Wine: rejected as the primary approach because
   it fails the explicit requirement to work without Maschine on every platform.
2. Read Native Access's installed-product registrations and index the actual content:
   selected direction. Share indexing and metadata handling; isolate platform discovery.
3. Scan a global content directory or use Native Access's entire product catalog as
   the installed list: rejected. Installs can span disks, and a catalog can include
   products that are not installed. Filesystem names are not product identities.

Detailed choices below are engineering proposals, not additional user requirements.

## Global constraints

- Support native Windows, native macOS, and Linux with Wine; Maschine is never required.
- Read NI registrations, metadata, artwork, and content without modifying them; write only Deckard-owned settings/cache/index files.
- Discovery and indexing must not launch Native Access, Maschine, Wine, or plug-in instances, and must not require network access or elevated privileges.
- Use existing playback/plug-in support; discovery does not promise new host compatibility or decryption of protected formats.
- Preserve manual library folders, saved user content, favorites, and existing Maschine-database users without duplicate browser entries.
- Keep registry traversal, filesystem scanning, metadata decoding, and database writes off the UI and audio threads.
- Test with synthetic fixtures; real installed-library checks are explicit, read-only, optional locally, and required before claiming native-platform validation.
- Do not create or publish any pull request without the user's explicit consent for that specific PR.

## Discovery and identity

Introduce a small dependency-light `deckard-ni-content` crate shared by the viewer,
library indexer, and engine. It discovers installations, translates their paths,
and reports resource roots. It does not know about widgets, audio, SQLite, or
plug-in hosting. Reuse `plist`; use Windows registry APIs behind `cfg(windows)`.
Keep Wine registry parsing pure and testable on all hosts.

Windows: enumerate NI product registrations in both machine registry views under
`SOFTWARE\\Native Instruments`. A nonempty `ContentDir` is the baseline evidence
of content installation; validate supported alternate fields with fixtures before
accepting them. Do not treat `InstallDir` for a standalone executable as a library.
Handle expandable registry values with native expansion, preserving read failures.

macOS: enumerate system `/Library/Preferences/com.native-instruments.*.plist`
records containing content locations. Read binary/XML plists, POSIX paths, and
legacy volume-qualified HFS paths. Resolve mounted external volumes correctly;
never guess that every volume is the startup disk. User preference files and
Service Center/catalog files may enrich a confirmed installation but do not prove
installation on their own. Validate against a real modern Native Access install.

Linux: find prefixes in this precedence order: explicitly configured prefixes,
`WINEPREFIX`, prefixes enclosing configured yabridge Windows plug-in paths (sorted),
then `~/.wine`. Deduplicate canonical prefix paths. Read their `system.reg` product
registrations, including the 32-bit registry view. Inspect `user.reg` only for
per-user content registrations if Task 1 confirms that layout; otherwise omit it.
No whole-home scan and no execution of Wine. Paths resolve using `dosdevices/<drive>:`;
never assume `Z:` maps to `/`. Handle escaped strings and Unicode, and bounded
case-insensitive component lookup when an exact spelling does not exist. An
ambiguous match, unmapped drive, unsupported UNC mapping, or unreadable registry
produces a diagnostic rather than a guessed path.

A product ID is `ni:` plus the normalized exact NI registration name (Unicode
lowercase, surrounding whitespace trimmed); retain the original name for display.
Do not strip `Library`, `Factory`, or punctuation from identity. A verified NI ID
may be stored as an alias, but this release must not depend on catalog/XML IDs.
An installation ID combines provider namespace/prefix with the registration key;
moving its content directory does not change it. Same-name installations are kept
separately. For duplicate physical content, scan once; for different copies of the
same product, choose the first available installation by the precedence above and
show the other location in Preferences. Retain provenance so later precedence
changes are deterministic. Do not union different versions of the same product.

Discovery distinguishes `Available`, `Missing`, and `Unreadable` installations and
records which provider enumerations completed successfully. A missing mounted path
is not proof of uninstall. Only a complete provider enumeration may retire absent
registrations. Product and installation counts include products with no browseable
entries; such products appear in Preferences with an explanation, not fake presets.

## Shared content and metadata pipeline

Index the selected roots in the background into Deckard's own database. Content
format support is shared across platforms; host/loading support is separate.

| Family | Initial indexing contract |
|---|---|
| `.mxprj`, `.mxgrp`, `.mxsnd`, `.mxinst`, `.mxfx` | Existing readers supply names, banks, types, characters, tempo, author/vendor. |
| `.wav`, `.aif`, `.aiff`, `.aifc`, `.mp3` | Retain existing audio support. Read bounded ID3 metadata where present, including NI GEOB payloads; use RIFF/AIFF chunk readers or MP3 ID3 header reading without decoding audio. |
| `.nksf`, `.nksfx` | Enable metadata-only parsing for installed factory content, independent of the existing user-root restriction. Do not allocate/copy plug-in state merely to get tags. |
| `.nki`, `.nksn`, `.nmsv`, `.nfm8`, `.nrkt`, `.ens`, `.rkplr` | Index one entry per recognized file using existing host routing. Use a tested metadata reader when available, otherwise filename and registered product. Do not instantiate a plug-in during indexing. |
| Packed containers, `.nbkt`, unknown formats | Report unsupported content counts where useful; do not invent loadable browser entries. |

Enumerating internal snapshots inside a single Reaktor ensemble is not an initial
deliverable; list the ensemble itself. Existing UUID-based snapshot rows from the
optional Maschine index remain distinct and retain their favorites/previews.
Adding `.nksn` to the index does not enable Windows-only Kontakt loading mechanisms
on Linux/macOS; existing specific loading errors remain, never a false success.

Registered product name wins over the outer directory name. Preserve embedded bank
chains. Read embedded NKS sample metadata first, fill absent fields from NISound,
then use current filename-based fallbacks. A present-but-empty field is not license
to synthesize tags. Only a valid declared loop/one-shot kind overrides filename
classification; valid positive finite embedded tempo overrides a filename BPM.
Unrecognized device type does not erase a known extension-based kind.

Handle ID3v2.3/v2.4 sizes, padding, encodings, extended headers, and unsynchronization
according to their formats. Cap extracted metadata at 4 MiB per block and 16 MiB
total per file; seek over audio/state payloads, check every offset, and fall back
without dropping an otherwise browseable file if metadata is malformed. Shared
NKS MessagePack parsing must remain bounded; preserve existing preset load behavior.

Skip `.previews` as browser content while retaining it for previews. Traverse content
symlinks with a visited canonical-directory set to prevent cycles; avoid the current
silent depth-eight truncation. Use explicit diagnostics for unreadable directories.
Never descend into an instrument's packed sample archive to manufacture samples.

## Persistence, merging, and refresh

Add separate `native_*` tables to `library.db3`; leave existing user/manual tables
and path-based favorites readable. Store product/installation provenance, relative
path, content fingerprint (size + mtime), metadata reader version, UUID, resource
locations, scan generation, and availability. Schema upgrades are transactional.

Native entry identity is an unambiguously encoded tuple of product ID and relative
path; include UUID only for a distinct snapshot inside a multi-snapshot ensemble.
An ordinary single-entry file keeps its identity when UUID metadata is first learned
or changes. Retain its UUID as preview/metadata information. Keep path spelling from
disk and distinguish real case-sensitive files. Root relocation preserves identity
and favorite state. Capture an existing
favorite/suppression by physical path when a manual entry first becomes native;
make this a one-time migration so a later toggle is not overridden by legacy state.

Merge ordinary single-entry files by canonical physical file regardless of absent
or different UUID metadata. For ensemble snapshot rows only, add UUID to the merge
key; an ensemble's whole-file entry uses a separate sentinel. Do not merge by display
name. Recognize legacy manual rows referring to the same physical content. Native product identity
and nonempty native metadata win; an existing Maschine row can fill absent tags,
preview identifiers, and favorites. A freshly saved user override keeps user edits
and User classification. Preserve distinct UUID snapshots sharing one ensemble.
A whole-file ensemble entry must not erase snapshot rows. The optional Maschine
source continues to work on native Windows/macOS; adding Wine database discovery
is not needed for this feature.

Scanning uses per-product staging generations, publishing only complete scans.
Commit staging in bounded batches and discard unfinished generations after restart.
Cache hits must also check product identity, root selection, and reader version:
unchanged mtimes must not preserve `Libraries` or previously absent sample tags.
Failed/unreadable/missing roots keep their last complete cache and favorites, but
are excluded from loadable results and shown as unavailable in Preferences. On a
complete scan, remove disappeared entries; do not delete the user's files or
favorite history. Removing a manual folder cannot remove an auto-discovered product.

First Library open per launch runs discovery and an incremental scan. Subsequent
opens refresh discovery with a five-second debounce; explicit Refresh always runs.
No filesystem watcher or Native Access process integration in the first release.
Coalesce repeated refresh requests, cancel obsolete work between files, and service
pending browser queries after at most 64 processed files. Retain old complete results
while scanning. Scan completion, counts, and errors are actual worker results;
remove the current premature `Library refreshed` success and swallowed scan errors.

## Artwork, previews, and sample lookup

Search explicit `DECKARD_NI_IMAGE_DIR` first, then the selected product's
`PAResources/image`, then platform/Wine `NI Resources/image`, followed by verified
registered host resource folders. Match the registered product name, not its parent
folder. Prefer `MST_artwork`/`MST_logo`, then existing VB alternatives. Keep existing
built-in pictures. Key resource caches by discovery revision so moved/new products
work without restarting. Cache misses must not live forever.

Use existing adjacent `.previews` naming rules. Resolve Native Browser Preview
Library through the same installed-product catalog when present; use its preview
paths only when reliable IDs/relative paths are available. Missing previews disable
prehear normally and do not hide an entry.

Feed selected discovered roots to the engine's existing sample resolver and clear
its cached root choices on catalog changes, including the all-installed-roots
fallback used by expansion kits naming `Maschine 2.0 Library`. Sample resolution
must work before the browser is first opened: share an asynchronous catalog refresh
at app startup, and reconcile pending project/sample loading with its completion.
Never run discovery from the audio callback. Preserve existing native fallback
resolution and manual roots while adding the shared catalog.

## User-facing behavior and acceptance

Preferences > Library gains a `Native Access libraries` section with detected product,
location, and status (`Indexed`, `Scanning`, `Unavailable`, `No supported content`,
or `Scan failed`), plus Refresh. Keep the existing manual list as `Additional library
folders`. Linux additionally offers explicit Wine-prefix folders, not per-library
paths. Status details explain missing hosts/loading limitations separately from
indexing failures. No account sign-in, activation, installation, or download controls.

Desktop Library and MK3/JAM receive the same Results/facets. A known tagged sample
must show its Type/Sub-type; touching MK3 knob 5 shows Types, and after selecting one,
knob 6 shows its sub-types. Character appears only if the content supplies it.
Do not redesign popup behavior in this feature.

Acceptance requires on each platform: no Maschine/database; a registered expansion
in a custom path; correct product name/artwork; tagged kit/sound/sample results;
preview where present; kit sample resolution; refresh/restart stability; relocation;
and no duplication with an overlapping manual folder. Linux adds custom drive maps
and multiple prefixes. Windows/macOS native APIs must be validated on those hosts,
not claimed from Linux fixtures or cross-compilation alone.

## Scope limits and remaining evidence

Not included: new popup designs, cloud catalog/entitlement lookup, library installation,
license checks, unpacking encrypted archives, implementing new plug-in hosts, or
guaranteeing every NI product's proprietary snapshot format. Discovery must still
report such installed products even when they have no supported entries.

Task 1 confirms modern native macOS/Windows registration fields, name casing and
legacy path forms, representative GEOB/NKS metadata, and preview layouts. Store only
minimal synthetic fixtures and an evidence matrix. If those observations contradict
the adapters above, update this design/plan before implementation of that adapter;
do not silently substitute a Maschine requirement. Native Access internal caches
are not an assumed public API and are not necessary to the proposed baseline.

## Deliverable sequence

1. Evidence matrix and platform discovery with path translation.
2. Shared metadata readers and installed-content indexing with migration/refresh.
3. Artwork, previews, sample resolution, and browser/preferences integration.
4. Cross-platform regression tests and installed-content acceptance.

All four are part of the planned feature. Any deferred compatibility issue must be
reported explicitly; successful indexing alone is not evidence that a host can load
every displayed native preset.

## Design review record

A fresh builder review identified UUID enrichment as a migration hazard because
legacy manual entries do not persist UUIDs. The identity and merging rules above
were tightened accordingly: single-entry identity is path-based, snapshot identity
is UUID-qualified. No further material design questions were raised. This review
does not substitute for user approval or native-platform runtime validation.
