# Cross-platform Native Access Library Index Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically discover locally installed Native Access libraries and build Deckard's own searchable, tagged library on Windows, macOS, and Linux/Wine without Maschine.

**Architecture:** A shared `deckard-ni-content` crate discovers installed products and reads portable content metadata. Platform adapters identify installations; `deckard-library` scans selected roots into separate native-content tables in its existing database, and the viewer shares the resulting catalog with artwork, preview, and sample resolution. The current optional Maschine source remains compatible but is never a prerequisite.

**Tech Stack:** Existing Rust workspace and pinned mise toolchain, SQLite/rusqlite, plist, Windows registry APIs, a read-only Wine registry parser, bounded RIFF/AIFF/ID3 and NKS metadata parsing, existing egui/Lua controller UI.

**Spec:** [Native Access library discovery and indexing](../specs/2026-11-26-native-access-library-index-design.md). Read both documents before execution. Implementation authorized by the user on 2026-11-26 on its own branch. No permission to open a PR.

## Execution outcome

Implemented on `javiermgc/native-access-library-index` from `origin/main` (`2793af2`). The checklist below records the original proposed steps; the [verification record](../../native-access-libraries-verification.md) records actual coverage, review fixes, decisions, and remaining native-platform validation.

## Global Constraints

- Support native Windows, native macOS, and Linux with Wine; Maschine is never required.
- Read NI registrations, metadata, artwork, and content without modifying them; write only Deckard-owned settings/cache/index files.
- Discovery and indexing must not launch Native Access, Maschine, Wine, or plug-in instances, and must not require network access or elevated privileges.
- Use existing playback/plug-in support; discovery does not promise new host compatibility or decryption of protected formats.
- Preserve manual library folders, saved user content, favorites, and existing Maschine-database users without duplicate browser entries.
- Keep registry traversal, filesystem scanning, metadata decoding, and database writes off the UI and audio threads.
- Test with synthetic fixtures; real installed-library checks are explicit, read-only, optional locally, and required before claiming native-platform validation.
- Do not create or publish any pull request without the user's explicit consent for that specific PR.

## Review Focus

1. A configured parent `Libraries` overlaps an automatically discovered expansion: one correctly named result, existing favorites retained, even with unchanged mtimes (Tasks 5–6).
2. An external disk disappears or a registry cannot be read: cached metadata and favorites survive, unavailable content is distinguished from uninstall (Tasks 2–3, 5–7).
3. Two Wine prefixes register the same product, including drive mappings and case differences: deterministic source selection, no guessed or cross-prefix file path (Task 3).
4. A malformed/truncated/oversized tag block precedes a large audio/state payload: bounded work, no crashes, no audio decoding, and a fallback browser entry (Task 4).
5. Startup opens an expansion project before the browser; refresh interrupts a scan; another product installs while Deckard stays open: samples resolve, old results remain usable, and stale scans cannot overwrite new choices (Tasks 6–9).

## Execution baseline and file map

Before execution, inspect `git status`, current HEAD and applicable instructions; use
the worktree skill if implementation needs isolation. The planning checkout has
unrelated untracked documents and an existing branch; do not stage or remove them.
Create no worktree for this planning-only task. Every command below runs at the
`Deckard` repository root. Use `mise exec -- cargo ...` for targeted commands and
`mise run test`/`mise run lint` for repository tasks. Update `Cargo.lock` intentionally
when introducing dependencies, then use `--locked` for final verification.

| Files | Responsibility |
|---|---|
| New `crates/ni-content/{Cargo.toml,src/lib.rs,src/catalog.rs}` | Shared discovery/identity contracts, provider results, installation selection |
| New `crates/ni-content/src/{windows.rs,macos.rs,wine.rs,paths.rs}` | Platform registrations, Wine parsing, path translation |
| New `crates/ni-content/src/metadata/{mod.rs,nks.rs,id3.rs,audio.rs}` | Metadata-only parsing, shared NKS description type, bounded sample tag readers |
| New `crates/ni-content/tests/{discovery.rs,metadata.rs,fixtures/}` | Synthetic portable fixtures and provider tests |
| `Cargo.toml`, `Cargo.lock`, dependent crate manifests | Add the shared crate without a host/UI dependency cycle |
| `crates/plugins/src/nks.rs` | Delegate NISI description decoding to shared parser, preserve public API/load behavior |
| New `crates/library/src/{native_index.rs,native_scan.rs,native_tests.rs}` | Native tables, generations, scanning and migration tests |
| `crates/library/src/{lib.rs,user.rs}` | Merge sources, stable favorite keys, legacy migration, normal user/manual behavior |
| New `apps/viewer/src/{ni_library.rs,ni_library_tests.rs,ni_library_preferences.rs}` | Catalog worker, status, lifecycle, settings UI and integration tests |
| `apps/viewer/src/{main.rs,browser.rs,settings.rs,plugin_pictures.rs}` | Hook catalog/index into browser, settings, previews and controller resources |
| `crates/engine/src/library.rs` and sample-cache call sites | Catalog-aware sample roots and cache invalidation |
| `crates/surface/src/tests.rs` | Verify existing browse tag overlays with discovered metadata |
| `docs/native-access-libraries.md`, `docs/browser.md`, `DEVELOPMENT.md`, `README.md` | Installed-library behavior, limitations, diagnostics and test procedure |

Keep implementation in these focused modules; do not grow the large viewer `main.rs`
beyond the hooks it needs. New crate dependencies: reuse workspace versions of
`plist`, `mxprj`, and target-gated `windows-sys`; use `tempfile` for tests. Add a small
TOML parser for yabridgectl configuration only if one is not already usable; do not
parse its TOML with line splitting. No dependency on the plug-in host from ni-content.

## Shared interface decisions

These signatures are planned contracts, not names claimed to exist today. Each
task owns the types it introduces; later tasks consume these same names.

`deckard_ni_content` exports:

```rust
pub struct DiscoveryRequest {
    pub explicit_wine_prefixes: Vec<PathBuf>,
    pub env_wine_prefix: Option<PathBuf>,
    pub yabridge_plugin_dirs: Vec<PathBuf>,
    pub home: PathBuf,
}
pub enum Availability { Available, Missing, Unreadable }
pub struct Installation {
    pub id: String, pub product_id: String, pub name: String,
    pub root: Option<PathBuf>, pub version: Option<String>,
    pub availability: Availability, pub source: String,
    pub image_roots: Vec<PathBuf>,
}
pub struct DiscoveryIssue { pub source: String, pub message: String }
pub struct DiscoveryReport {
    pub installations: Vec<Installation>,
    pub complete_sources: Vec<String>, pub issues: Vec<DiscoveryIssue>,
}
pub fn discover(request: &DiscoveryRequest) -> DiscoveryReport;
pub fn select_installations(report: &DiscoveryReport) -> Vec<Installation>;
pub fn wine_to_host(prefix: &Path, path: &str) -> Result<PathBuf, String>;
```

`Installation.root == None` means the registered path could not be resolved, never
the process working directory. `source` is stable provider/prefix provenance;
`id` adds the registration key. Provider results preserve precedence order. Native
host environment collection is separate from pure parsers, so fixtures run on any OS.

`metadata` exports `ContentKind` (the seven existing library kinds),
`ContentMetadata` (optional name/vendor/author/tempo/kind/UUID plus bank/types/characters),
`NksSoundInfo` (the current `deckard_plugins::nks::SoundInfo` fields unchanged),
and `MetadataRead { metadata: ContentMetadata, warnings: Vec<String> }`:

```rust
pub const READER_VERSION: u32 = 1;
pub fn read_content(path: &Path) -> Result<MetadataRead, String>;
pub fn read_nks_description(bytes: &[u8]) -> Result<NksSoundInfo, String>;
```

`read_content` inspects only known metadata structures. Native formats without a
reader return empty metadata, not a fabricated description. Corrupt metadata returns
warnings and usable fallback metadata; failure to open/read a file returns `Err`.
Keep f64 tempo outside `NksSoundInfo` to preserve its existing `Eq` contract.

`deckard_library` gains `Item.native_key: Option<String>` (Default `None`),
`ScanRequest { generation: u64, discovery: DiscoveryReport, manual: Vec<user::Root> }`,
and `ScanProgress { generation: u64, processed: usize, finished: bool, issues: Vec<String> }`.
Store the current chosen installation catalog independently from visible item rows.

```rust
impl Library {
    pub fn begin_scan(&mut self, request: ScanRequest) -> Result<(), String>;
    pub fn scan_step(&mut self, max_files: usize) -> Result<ScanProgress, String>;
    pub fn cancel_scan(&mut self) -> Result<(), String>;
}
```

These methods wrap manual scan compatibility and native staging; do not replace
the current `Library::scan` used by existing tests until its callers are migrated.

## Task 1: Lock down installation and metadata evidence

**Files:** Create `docs/native-access-libraries-evidence.md` and synthetic fixture manifest `crates/ni-content/tests/fixtures/README.md` (crate created in Task 2).

**Interfaces:** Consumes the design's observed fields. Produces an evidence matrix used by the provider/format tests; no product code.

- [ ] Record Windows native registry views and macOS system plists from consenting local test installations, or clearly mark each unverified field pending native validation. Include content path, registration name, version, install/relocate/uninstall behavior, and relevant resource layouts. Never dump serials/authentication fields.
- [ ] Verify a Windows fixture with `ContentDir`, a modern macOS binary/XML plist and external-volume path, and a Wine fixture with escaped Unicode and nonstandard drive mapping. Fixture manifests distinguish observed layouts from proposed fallbacks. Missing native access does not block pure-code planning, but blocks claiming that platform verified.
- [ ] Inspect representative `.mxgrp`, `.mxsnd`, WAV/AIFF/MP3, NKS and native preset metadata read-only. Record which fields are independently extractable without a host, whether per-file previews exist, and which formats are inventory-only. Do not confuse product `categories.db` with per-item tags.
- [ ] Build minimal synthetic fixture descriptions reproducing these structures; do not copy a proprietary library, registry dump, or Native Access catalog into the repository. Record metadata chunk precedence and payload limits from the spec.
- [ ] Review the evidence against this design. Resolve field/layout mismatches in the plan before implementing the affected adapter. Suggested checkpoint commit: `docs: define Native Access discovery evidence and fixtures`.

## Task 2: Shared catalog plus native Windows/macOS discovery

**Files:** Create ni-content manifest, `lib.rs`, `catalog.rs`, `windows.rs`, `macos.rs`, `paths.rs`, and `tests/discovery.rs`; modify workspace manifest/lockfile.

**Interfaces:** Produces the discovery types/functions above; `discover` dispatches to native adapters on Windows/macOS. Linux initially produces an empty report until Task 3. `select_installations` applies the spec's identity/selection rules.

- [ ] Write failing fixture tests: `native_registry_views_merge`, `catalog_records_do_not_prove_installation`, `binary_and_xml_plists_agree`, `external_hfs_volume_resolves`, `missing_root_is_not_uninstalled`, `failed_provider_is_not_complete`, and `same_product_copies_select_first_available`. Assert actual registered names survive and a generic parent directory does not become the product name. Ignore executable-only `InstallDir` entries.
- [ ] Run `mise exec -- cargo test -p deckard-ni-content --test discovery`; expect missing crate/API failures before implementation.
- [ ] Implement native enumeration with read-only APIs, pure adapters for fixtures, reliable provider completeness, and canonical path deduplication. Share legacy path conversion behavior with the engine rather than introducing competing macOS conversions. Preserve original strings for diagnostics.
- [ ] Add fixture tests for invalid plists, access-denied registry branches, expandable registry paths, duplicate aliases and case-sensitive filesystems. Each failure must leave other valid installations discoverable.
- [ ] Run the discovery suite on the current host and native Windows/macOS CI. Native live tests must use read-only enumeration and be opt-in. Suggested checkpoint commit: `feat: discover registered NI content on Windows and macOS`.

## Task 3: Wine discovery with explicit prefix and drive semantics

**Files:** Create `crates/ni-content/src/wine.rs`; extend `paths.rs`, `catalog.rs`, and discovery fixtures/tests.

**Interfaces:** Consumes `DiscoveryRequest`; completes Linux `discover` and `wine_to_host`. Provider namespaces include canonical prefix paths.

- [ ] Write failing tests `wine_registry_content_dirs`, `wine_prefix_precedence`, `yabridge_paths_reveal_custom_prefix`, `custom_drive_mapping_is_used`, `exact_path_precedes_case_fold`, `ambiguous_case_match_is_diagnostic`, `wine_symlink_prefix_is_deduplicated`, and `missing_prefix_preserves_source_failure`. Assert a `Z:` mapping to a temp directory is honored rather than replaced with `/`.
- [ ] Run `mise exec -- cargo test -p deckard-ni-content --test discovery wine`; expect the fixtures to fail with Linux's empty report.
- [ ] Implement escaped string/section parsing for Wine `system.reg`, both NI registry views, and canonical prefix discovery. Locate prefixes by checking ancestors for prefix markers, not by assuming all paths contain the same fixed number of components. Read only product content/version fields. Apply Task 1's evidence decision for any per-user provider.
- [ ] Resolve drive paths, Unicode and case mismatches without Wine subprocesses. Reject unresolved drive/UNC paths with a clear issue. Handle `c:` and mapped external drives without resolving relative paths against Deckard's cwd.
- [ ] Run the full discovery suite and a read-only check against the user's prefix using an opt-in test. Expected products include Rhythm Source, Studio Drummer and TRK-01 at their actual host paths; this list belongs in the local acceptance record, not production code. Suggested commit: `feat: discover Native Access content in Wine prefixes`.

## Task 4: Read portable tags without loading instruments

**Files:** Create ni-content metadata modules and `tests/metadata.rs`; modify `crates/plugins/{Cargo.toml,src/nks.rs}` and lockfile.

**Interfaces:** Produces the metadata API above. Re-export `NksSoundInfo` as the existing `deckard_plugins::nks::SoundInfo` or convert losslessly; preserve `comment`, `modes`, UUID and current writing semantics.

- [ ] Write failing tests `wav_geob_types_and_product`, `nisound_fills_missing_nks_fields`, `embedded_loop_kind_beats_filename`, `embedded_tempo_beats_filename`, `nks_factory_description_without_state`, and `metadata_free_native_file_is_inventory_only`. Synthetic sample assertions include product `Rhythm Source`, name `Shuffle Woo 1`, and types `[["Percussion", "Other"]]`; no hardcoded production special case.
- [ ] Run `mise exec -- cargo test -p deckard-ni-content --test metadata`; expect missing parser/results failures.
- [ ] Extract shared NKS description decoding from `plugins/src/nks.rs` without moving plug-in state/COM logic. Reuse the current bounded MessagePack reader by extracting its generic decoding utility into ni-content and importing it from the plug-in parser; preserve PLID/NICA behavior and existing tests. Add optional tempo handling in `ContentMetadata` separately. Reuse `mxprj::sound_info` for NISound only after a fixture proves payload compatibility.
- [ ] Implement streaming metadata extraction for RIFF/WAVE, AIFF ID3 chunks and MP3 ID3 tags. Seek over audio and PCHK state. Extend shared MessagePack decoding for observed floating-point tempo while retaining bounds. Test ID3v2.3/v2.4 encodings, extended headers, unsynchronization, odd RIFF padding, conflicting/empty fields, bogus lengths, truncation and 4 MiB/block–16 MiB/file limits. A counting reader must prove a large audio/state body is not read.
- [ ] Run metadata tests and `mise exec -- cargo test -p deckard-plugins nks`; expected existing preset parse/write/load-fixture tests still pass. No live plug-in host is required for these tests. Suggested commit: `feat: index NI metadata embedded in samples and presets`.

## Task 5: Native index persistence, migration and source merging

**Files:** Create `crates/library/src/native_index.rs` and `native_tests.rs`; modify `lib.rs`, `user.rs`, library manifest, and schema fixtures.

**Interfaces:** Owns native-table schema and `Item.native_key`; `Item::favorite_key` prefers `native_key` before current path/UUID logic. `Library::query` merges native, manual/user and optional Maschine rows according to the spec.

- [ ] Write failing tests `legacy_database_upgrades_transactionally`, `native_index_round_trips_metadata`, `manual_parent_migrates_without_mtime_change`, `favorite_and_suppression_migrate_once`, `root_move_keeps_favorites`, `same_file_has_one_result`, `manual_without_uuid_merges_native`, `single_file_uuid_enrichment_keeps_identity`, `saved_user_override_wins`, and `ensemble_snapshots_remain_distinct`.
- [ ] Run `mise exec -- cargo test -p deckard-library native_`; expect missing native data/merge behavior failures.
- [ ] Add `native_products`, `native_installations`, `native_items`, `native_staging`, and one-time favorite-migration bookkeeping in a versioned transaction. Persist all Item fields needed for vendor/tag/preview queries plus relative identity, source/version, fingerprint, reader version, scan generation and availability. Use an internal `NativeIndex` module borrowing the existing index connection rather than parallel uncontrolled writers. Keep legacy tables readable.
- [ ] Implement canonical-path merging for single-entry files regardless of UUID metadata; add UUID to the merge key only for ensemble snapshot rows, keeping a whole-file sentinel separate. Preserve saved user overrides and distinct per-snapshot favorite keys. Encode native logical keys as an unambiguous tuple of product ID and relative path (plus snapshot UUID only when applicable), using length-prefixes rather than raw delimiter concatenation. Keys survive root relocation and single-file UUID enrichment; migrate old path favorites/suppressions once when ownership is first established. Requery through the actual browser `Library::query` in tests rather than asserting only SQL rows.
- [ ] Verify rollback leaves the previous schema/data intact, repeated migration is idempotent, changed product identity/reader version invalidates the old mtime-only cache, and `forget(manual_root)` does not delete native rows. Run all `deckard-library` tests. Suggested commit: `feat: persist and merge independently indexed NI libraries`.

## Task 6: Incremental scanner and truthful refresh lifecycle

**Files:** Create `native_scan.rs`; extend `native_index.rs`, `native_tests.rs`, `lib.rs`, and `apps/viewer/src/browser.rs`.

**Interfaces:** Implements `ScanRequest`, `ScanProgress` and the three `Library` scan methods. Viewer worker passes `max_files = 64`, checks for newer jobs between steps, and publishes generation-tagged progress/results.

- [ ] Write failing tests `installed_factory_formats_are_indexed`, `native_scan_reads_tags_without_plugin_host`, `incomplete_scan_never_replaces_last_complete`, `missing_disk_preserves_cache`, `completed_discovery_retires_removed_registration`, `changed_reader_reindexes_unchanged_file`, `symlink_cycle_terminates`, `nested_content_beyond_eight_levels`, and `refresh_query_is_serviced_between_batches`.
- [ ] Run `mise exec -- cargo test -p deckard-library native_` and `mise exec -- cargo test -p deckard browser::`; confirm failing scan/scheduling tests.
- [ ] Implement resumable traversal, per-product staging generations, short bounded staging transactions, and atomic publish. Store counts/warnings; malformed tags retain fallback entries, while an incomplete filesystem walk never triggers destructive pruning. Maintain a visited canonical-directory set. Preserve current manual source semantics through the request.
- [ ] Integrate scan scheduling into the browser worker. Coalesce refreshes, cancel/discard obsolete staging, and keep cached queries usable. Replace swallowed scan errors and immediate `Library refreshed` status with generation-matched worker events. Do not eagerly allocate an entire library's audio/preset state.
- [ ] Verify first scan, no-change refresh, newly installed product, moved root, deletion, interrupted process, provider failure and recovery. Run the library suite and browser query-order regression tests. Suggested commit: `feat: refresh NI content indexes incrementally in the background`.

## Task 7: App catalog lifecycle, preferences, and sample resolution

**Files:** Create `apps/viewer/src/{ni_library.rs,ni_library_tests.rs,ni_library_preferences.rs}`; modify viewer manifest, `main.rs`, `settings.rs`, `browser.rs`, engine manifest, and `crates/engine/src/library.rs` plus sample-cache setup call sites.

**Interfaces:** `NiLibrary` owns a discovery worker and current catalog revision:

```rust
pub struct CatalogSnapshot { pub revision: u64, pub report: DiscoveryReport }
impl NiLibrary {
    pub fn new(request: DiscoveryRequest) -> Self;
    pub fn refresh(&mut self, request: DiscoveryRequest);
    pub fn poll(&mut self) -> Option<CatalogSnapshot>;
}
// deckard_engine::library::Resolver
pub fn set_installed_content(&mut self, revision: u64, roots: &[PathBuf]);
```

Persist repeated `ni_wine_prefix = <path>` settings on Linux; parse/preserve them
cross-platform. Build `DiscoveryRequest` from the environment, those settings, and
yabridgectl configuration using a TOML parser. Settings UI only passes requests;
it never enumerates registries or scans content synchronously.

- [ ] Write failing tests `native_catalog_starts_without_browser`, `no_maschine_database_required`, `native_settings_round_trip`, `catalog_refresh_invalidates_resolver_roots`, `refresh_after_install_updates_catalog`, and `manual_folder_removal_preserves_auto_library`.
- [ ] Run `mise exec -- cargo test -p deckard ni_library` and `mise exec -- cargo test -p deckard-engine library`; confirm failures before hooks are implemented.
- [ ] Start catalog discovery asynchronously during app startup. Share snapshots with the browser scanner, renderer and sample resolver. If startup project loading needs roots before discovery returns, defer/retry that load's sample resolution after the catalog arrives, without blocking the UI/audio thread or silently leaving an otherwise resolvable project broken. Test this ordering with a fake discovery barrier.
- [ ] Add Preferences sections/statuses from the design, Refresh, and Linux prefix selection. Refresh on first browser open and debounced later opens; manual Refresh bypasses the five-second debounce. Distinguish no supported entries, offline roots and scan errors. Keep manually configured folders intact; do not silently rewrite settings to fix `Libraries`.
- [ ] Supply discovered roots to existing per-library and all-installed sample searches, including generic `Maschine 2.0 Library` references. Invalidate relevant caches on a changed revision and preserve native/manual fallbacks. Verify a moved expansion kit loads from the new root before opening Browser. Suggested commit: `feat: integrate Native Access libraries with browser and sample loading`.

## Task 8: Product artwork and preview resources

**Files:** Modify `apps/viewer/src/{plugin_pictures.rs,browser.rs,ni_library.rs,ni_library_tests.rs,main.rs}`; extend resource fixtures in ni-content tests.

**Interfaces:** Consume `CatalogSnapshot`; add `plugin_pictures::set_content_catalog(revision: u64, installations: &[Installation])` and `browser::set_content_catalog(revision: u64, installations: &[Installation])`. These replace process-lifetime root/miss assumptions while retaining existing public preview/image entry points.

- [ ] Write failing tests `registered_name_finds_pa_artwork`, `explicit_image_root_has_priority`, `wine_shared_resources_resolve`, `missing_artwork_is_retried_after_refresh`, `same_product_other_prefix_does_not_supply_wrong_artwork`, and `native_preview_is_found_without_maschine`.
- [ ] Run `mise exec -- cargo test -p deckard ni_library` and existing `plugin_pictures`/`prehear` tests; expect new resource cases to fail first.
- [ ] Resolve product-local artwork and selected-installation shared roots using the exact precedence in the design. Scope caches to catalog revision and product/installation identity; loading a new expansion during the session must not require restart. Preserve built-in images and existing Maschine image keys.
- [ ] Reuse adjacent preview lookup; discover Native Browser Preview Library from installed records, resolving Windows paths through Wine. Fall back gracefully when UUID/product-relative preview identifiers are unavailable; do not guess one product's preview from another by filename alone.
- [ ] Run image/preview fixture tests and render an MK3 Browse screenshot for a synthetic product. Use the local Rhythm Source artwork only for an optional uncommitted acceptance render. Suggested commit: `feat: resolve NI library artwork and previews across platforms`.

## Task 9: End-to-end acceptance, documentation, and platform validation

**Files:** Extend `ni_library_tests.rs`, `apps/viewer/src/browser_tests.rs`, `crates/surface/src/tests.rs`; create `docs/native-access-libraries.md`; update browser/development docs and README. Extend existing CI only if the new crate/tests are not covered by its workspace matrix.

**Interfaces:** Consumes the completed catalog, index, resources and controller Results. Produces a reproducible acceptance record distinguishing fixture, native API, live content, and host-loading verification.

- [ ] Write integration fixtures with no `komplete.db3` and no Maschine installation: one registered expansion carrying a tagged kit, sound, sample, artwork and preview; one inventory-only native preset; an overlapping manual parent; and a moved/offline root. Assert correct product, metadata, deduplication, favorites and status after refresh/restart.
- [ ] Add `native_library_tags_reach_mk3_overlay`: use actual indexed Results to form the browser view; Type contains `Percussion`, touch CAP_5 reveals it, selecting it exposes `Other` on CAP_6, and release dismisses normally. An untagged entry must not fabricate Character options. Check desktop and JAM consume the same facets without introducing a new popup design.
- [ ] Run focused suites: `mise exec -- cargo test -p deckard-ni-content -p deckard-library`; `mise exec -- cargo test -p deckard ni_library`; `mise exec -- cargo test -p deckard browser`; `mise exec -- cargo test -p deckard-engine library`; `mise exec -- cargo test -p deckard-surface browse`.
- [ ] Run required final checks: `mise run lint`, `mise exec -- cargo check --workspace --all-targets --locked`, and `mise run test`. Use the repository's native Windows/macOS/Linux CI matrix; `scripts/check-platforms.sh` is an additional type check, not proof of runtime behavior. Record environment-dependent live/audio test failures separately, never label unrun tests passed.
- [ ] Perform the design's live acceptance on Windows/macOS without Maschine and on Linux/Wine. Include an external content disk and a fresh-install/relocation refresh. Use the user's Rhythm Source setup for Linux acceptance; verify sample preview and a kit's sample resolution on the actual controller. Confirm unsupported Kontakt/Reaktor loading reports a specific limitation rather than being misreported as an indexing failure.
- [ ] Document discovery sources, refresh behavior, native preset coverage, lack of a Maschine requirement, additional folders, Linux prefix configuration, missing-host limitations and error recovery. Mark unsupported embedded-snapshot enumeration explicitly. Suggested final commit: `test: verify independent Native Access library browsing on all platforms`.

## Dependency order and completion criteria

Tasks 1 → 2 → 3 establish trustworthy installation records. Task 4 is portable
metadata work after the contract/fixtures are fixed. Tasks 5 → 6 consume both;
Task 7 connects the catalog and scanning to app lifecycle, Task 8 connects resources,
and Task 9 verifies the whole behavior. Keep commits reviewable; merge unrelated
refactors into neither this feature nor its tests.

The feature is complete only when no-Maschine discovery/indexing acceptance succeeds
on all three platforms, the same tagged content reaches desktop/controller facets,
the existing manual and optional Maschine sources regress cleanly, and migration,
offline handling, refresh and loading of already-supported formats are verified.
Do not broaden that claim into support for every NI encrypted or snapshot format.

## Planning review and implementation handoff

Planning deliverables are this file and its design. No product code, settings,
installed libraries or index contents should be changed while producing the plan.
Review the design's proposed defaults, especially multiple installations, unavailable
content, refresh timing, and initial native-preset coverage, before execution.

Recommended execution: inline in the current chat after approval, using an isolated
implementation checkout if needed and one whole-change review. The modules share
identity and migration contracts, so sequential integration is simpler than parallel
implementation. A subagent-driven implementation remains an option if requested.
No PR is authorized by approving this plan or by completing its tasks.
