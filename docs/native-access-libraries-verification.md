# Native Access indexing verification

Completed 2026-11-26 on branch `javiermgc/native-access-library-index`, based on `2793af260ef44e63d3f6f60681a77fe39950b2b0`. No push, merge, PR, deployment, or replacement of the running application was performed.

## Final checks

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 3,092 passed, 48 ignored, zero failures; 72 reported suites including doc tests |
| `cargo build -p deckard --locked` | Passed on Linux; debug executable `target/debug/deckard` |
| `cargo fmt --all --check` | Passed |
| `rustfmt --edition 2024 --check apps/viewer/src/ni_pictures.rs` | Passed (hand-written helper included by generated module) |
| `git diff --check` | Passed |
| `cargo check -p deckard-ni-content --target x86_64-pc-windows-msvc --locked` | Passed |
| Explicit live discovery and temporary index | Eight products, 4,086 supported files, zero issues; no Maschine dependency |

Commands used the installed pinned Rust 1.99.1 toolchain because `mise` was unavailable. Audio-dependent builds used the existing local ALSA development pkg-config file. An earlier test command without the engine `output` feature could not compile its pre-existing audio examples; the final workspace command supplies the normal feature union and passes.

## Coverage and independent review

The independent whole-branch review requested corrections. All reported correctness/regression findings were addressed, with tests covering:

- Saved User overrides surviving overlapping manual refreshes; registered product ownership across symlink aliases; alias favorites migrating once and surviving relocation.
- Re-favoriting after a migrated legacy suppression; keeping optional Maschine UUID/preview metadata; excluding unavailable content even when the optional database still lists it.
- Connection-local staging for simultaneous Deckard instances; canceled scans; removed provider scope and confirmed uninstall; unavailable directory walks retaining cached data without exposing loadable entries.
- Unchanged manual files being skipped, uppercase NKS extensions, deep folders and directory cycles, and forgetting a manual folder canceling its pending scan.
- SQL kind/product filtering before native row decoding, skipping native rows on the User tab, and caching canonical merge paths across queries.
- Queued favorites/saves preceding the query they refresh, scan completion status, and native metadata flowing through the Browser into the existing MK3 Type/Sub-type touch overlays.
- Product-local/shared artwork, invalidation of existing surface assets on a new resource revision, and preservation of hand-written helpers when regenerating built-in artwork.
- Portable XML/binary plist and Wine fixtures; prefix/drive semantics; isolated Wine tests on Unix; no native macOS host enumeration from fixture tests.
- Streaming metadata reads, a sparse 128 MiB audio payload, NKS/NISound precedence, WAV/AIFF/MP3/NKS/Maschine metadata, malformed lengths, and retaining good metadata preceding a damaged ID3 frame.

Native files without a supported metadata reader remain inventory-only. Internal ensemble snapshots are not independently enumerated. Discovery does not add new plug-in playback/decryption support. Native Windows/macOS live installations and physical controller behavior were not exercised on this Linux host; cross-compilation and synthetic/GUI integration tests do not replace those checks.

## Implementation decisions

1. Reused the clean tracked checkout on a new branch, preserving unrelated untracked documents. No additional worktree was necessary. Risk: the current checkout now points at the new branch; the previous branch is preserved.
2. Used the installed pinned Rust binaries instead of unavailable `mise`. Risk: local dependency setup differs from mise automation; the compiler version is unchanged and final commands are recorded above.
3. Maintained a local execution ledger because the skill helper's dependent script was not executable; no skill permissions were changed. Risk: workflow bookkeeping is manual; verification and decisions are preserved here.
4. Committed Windows/macOS and Wine adapters together because they share the catalog contract and portable fixture suite. Risk: those adapter changes share a revert boundary; no runtime platform requirement was removed.
5. Stored native Item payloads as JSON alongside indexed identity/product/path/fingerprint columns. This retains all metadata without delimiter ambiguity. Risk: larger cache rows. Kind/product filtering occurs before decoding; staging is connection-local TEMP state, with atomic product publication.
6. Preserved manual/saved tables and the optional read-only Maschine database API; native ownership and metadata are separate. Risk: source merging remains more complex, covered by integration regressions above.
7. Kept native Windows/macOS, actual hardware and plug-in playback verification explicitly unclaimed, consistent with the reviewer's declined-to-judge list. Risk: platform-specific runtime issues require testing on those hosts.

No review findings were deferred as minor issues. The branch is retained locally as requested; no PR consent was requested or inferred.

## Artwork fallback correction (2026-11-26)

Massive's installed artwork was in Komplete Kontrol's optional bundled resources, outside the previous search roots. Ozone's registration name includes its vendor, while its artwork directory does not. The resolver now preserves all existing directory/key matches, then tries explicit `.meta` name/vendor aliases, and only then host-bundled artwork. No generic vendor stripping or dependency on Komplete Kontrol/Maschine is introduced.

Validation for this correction:

- Content and library suites: 68 tests passed.
- Artwork fallback tests: 2 synthetic regressions plus 1 opt-in read-only live test passed. Both Massive and iZotope-Ozone 12 Equalizer resolve and decode thumbnails/logos for the browser and plug-in slots using the installed Wine assets. Existing matches and metadata aliases beat host fallbacks; an unrelated vendor does not match.
- Existing resource-resolution regression: 1 test passed; native library UI integration: 3 tests passed.
- Windows discovery cross-check (`x86_64-pc-windows-msvc`) passed. Windows common-files path fixtures, macOS plist discovery, and Wine case-insensitive mapped-drive fixtures passed. Native Windows/macOS runtime testing remains pending.
- Metadata is bounded to 64 KiB and malformed/non-image metadata is ignored. Resources remain read-only and are decoded on the catalog worker.

Repeat the installed-assets check with `DECKARD_TEST_WINE_PREFIX=/path/to/prefix cargo test -p deckard installed_massive_and_ozone_artwork --locked -- --ignored --nocapture`. It does not launch the app or modify preferences, library databases, or NI files.
