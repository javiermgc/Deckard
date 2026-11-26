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
