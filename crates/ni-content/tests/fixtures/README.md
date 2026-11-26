# Synthetic fixtures

Tests construct small independent fixtures in temporary directories: registry registrations, XML/binary plists, Wine drive symlinks, RIFF/AIFF/ID3 metadata and malformed bounds. No proprietary database, registry dump, serial, credential, audio or plug-in state is included.

Windows/macOS fixture layouts are adapter contracts pending native runtime validation. Wine ContentDir and product-local PAResources layouts were observed locally. NKS descriptions use the existing documented parser fields. Tests must distinguish missing roots from failed providers and metadata-free inventory from decoded tags.
