# Native Access indexing evidence

Read-only local examination, 2026-11-26. The supplied Maschine database is a comparison reference, never a runtime dependency or repository fixture.

| Source | Evidence | Validation |
| --- | --- | --- |
| Wine system.reg | Software\\Native Instruments product keys, ContentDir and ContentVersion; mapped Z: content outside prefix | Observed locally |
| Windows HKLM | Both registry views, ContentDir; executable-only InstallDir excluded | Existing resolver plus synthetic fixtures; native runtime pending |
| macOS system plists | com.native-instruments.<product>.plist ContentDir; XML/binary, POSIX/HFS paths | Existing resolver plus synthetic fixtures; native runtime pending |
| NativeAccess.xml | Catalog includes uninstalled products | Never installation evidence |
| Product PAResources/image | Product-local MST/VB artwork and logos | Observed Rhythm Source |
| Product categories.db | Product classifications, empty per-sample tables in inspected expansion | Not per-file sample tags |
| WAV | ID3 GEOB NKS metadata and NISound metadata | Observed sample types, bank, kind and tempo |
| Maschine files | SoundInfo metadata | Existing mxprj parser |
| NKS | RIFF NIKS NISI MessagePack description | Existing preset parser |
| Native instrument files | Not all formats have a portable metadata reader | Inventory-only fallback; no plug-in launched |

Reference database Rhythm Source counts: Project 12, Group 51, Sound 62, Instrument 118, Loop 443, Oneshot 658. Disk inventory includes 1107 WAV files and 118 native instrument files (55 nmsv, 15 nrkt, 48 nbkt). Battery nbkt is outside the supported format set, so instrument count parity is not expected. Database differences must be explained, never hardcoded into discovery.

Safety and precedence: all external sources read-only; fixtures generated independently; NKS nonempty fields win over NISound, filename is final fallback. Metadata blocks capped at 4 MiB, total metadata 16 MiB; audio/state payloads skipped. Discovery does not grant playback support for protected/native formats.

## Implementation acceptance comparison

The final read-only scan indexed 4,086 supported files across eight locally registered products, with zero metadata warnings. Rhythm Source yielded 12 projects, 51 groups, 62 sounds, 70 supported native instruments, 447 loops and 660 one-shots.

All 1,107 on-disk Rhythm Source WAV filenames match the reference database's main `k_sound_info` table. Its main table classifies 443 as loops and 664 as one-shots; four Shuffle WAVs carry embedded NKS `deviceType=LOOP` despite the reference's one-shot flags. Deckard follows the embedded metadata rather than hardcoding the reference database. The reference's per-kind views additionally omit six rows, explaining the initial 1,101 view total. The 48 Battery NBKT files are intentionally outside the supported set.

The reference database was opened only with SQLite URI `mode=ro`; installed files were read only. The test index lived in a temporary directory, not the user's Deckard index. See [verification](native-access-libraries-verification.md).
