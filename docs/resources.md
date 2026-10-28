# Original resource extraction

The resource pipeline reads the optical disc without executing its installer or game. It uses Node.js built-ins only.

```powershell
node tools/extract-assets.mjs --disc E:\
node --test tools/legacy-formats.test.mjs
```

Use `--only-exe` to extract only the original executable for analysis. The executable is written to `research/original/N2Game.exe`; `research/original/provenance.json` records its SHA-256 and the cabinet SHA-256. This directory is excluded from Git and the published website.

## Outputs and provenance

- `assets/manifest.json` maps case-normalized original paths to immutable content-addressed objects. `install/...` represents the cabinet's installed files; `cd/data/...` represents the separate optical-disc Data directory. These namespaces are kept distinct so a similarly named installed file never silently replaces a disc file.
- `assets/game/objects/<first-two-hash-characters>/<sha256>.<format>` stores original resource bytes, deduplicated by SHA-256. Binary objects use Git LFS. The extension reflects detected content; map `.Mrc` files are stored as BMP objects while their original names remain available in the manifest.
- `assets/catalog/*.json` contains reviewable decoded structures in ordinary Git. Every table identifies its original resource ID. These JSON files are extracted data, not guessed implementations of game rules.

The observed disc has 3,726 cabinet descriptors, including three deleted descriptors with flag `8`. The valid cabinet entries plus 2,947 disc Data files produce **6,669 public logical resources and 5,075 unique objects**. The original executable is excluded from those public resources. The resource bytes total **575,481,772 bytes** after deduplication, saving **240,682,033 bytes**. Website code, catalogs, manifests, and any later converted media add to the published size; the final site build must independently enforce its size budget.

Including the two ending videos and the final-battle opening video/audio derivatives, the current manifest contains **6,673 logical resources, 5,079 objects and 625,990,137 stored bytes**. `pnpm verify:assets` recomputes these counts and all SHA-256 hashes; the generated site has a separate 1,000,000,000-byte budget check.

`manifest.resources[resourceId].sha256` selects an entry in `manifest.objects`. Each object contains `{sha256, path, bytes, media}`. `path` starts with `assets/` and is relative to the deployed application root; resolve it against the application's base URL, including the repository prefix on GitHub Pages. Do not preload every object.

The source metadata records original spelling, cabinet entry index, compressed length, and expanded length where relevant. Every public source's SHA-256 is its referenced object's hash. Content collisions and differing bytes at an existing output path fail extraction. Rerunning the same disc produces stable JSON without wall-clock timestamps.

## Decoded map interface

`assets/catalog/maps.json` contains `{formatVersion: 1, logicalViewport: {width: 640, height: 480}, maps: [...]}`. The viewport is independently confirmed by the original executable's `SetDisplayMode(640, 480, 16, 0, 0)` call at VA `0x4234DB`.

Each map includes:

```text
id: original lowercase name, including the original spelling "acient"
sourceResource: cd/data/map/<id>/<id>.mmi
dimensions: {width, height}
chipOrigin: {x, y}
chipSize: {width, height}
start: {row, column}
chips: [{row, column, eventId, next: [{row, column}], backgroundCells: [{row, column}]}]
coordinateIndex: [{row, column}]
backgroundTiles: [{row, column, x, y, width, height, resourceId}]
legacySections: original MMI sections with decoded values and original rawHex
validation: {chipCount, coordinateCount, startExists, unresolvedEdges}
```

`start` is the entry declared by each map. It does not establish which map a new game initially uses. `next` copies the original `NextChip1/2Row/Column` values, excluding paired `-1` values; it does not invent movement direction, random rules, or reverse edges. `eventId` is retained as a number until executable analysis determines its dispatch semantics. `legacyEditorLabel`, when present, comes from the older `.MAP` editor metadata and is not an authoritative runtime event implementation.

The eight maps contain 683 cells. All MMI checksums pass, all next-cell references resolve, all declared entry cells exist, and MCT coordinate sets match the cells. Map BMP names `.Mrc` encode row then column: each image is 640×480, with `x = column * 640` and `y = row * 480`. This interpretation is checked against every complete map's declared dimensions.

## Decoded event interface

`assets/catalog/events.json` contains:

- `dialogues`: 4,549 records `{id, byteLength, text}` decoded from fixed 256-byte records. Text remains exactly as encoded, including original inline markers.
- `portraits`, `backgrounds`, `waves`: 464, 114, and 64 records respectively, with the same public record fields and 24-byte source records.
- `additionalLists`: backdrop, dialog frame, MIDI, and initial-mode tables. Original sentinel records whose length is `0xffff` retain `rawHex` instead of fabricated text.
- `abc`: 294 records of five original uint16 words; the original header words are retained.
- `conditions`: 79 records of 29 original uint16 words, including the first sentinel record.
- `results`: 1,002 records of 59 original uint16 words.
- `switches`: 20 records of 14 original uint16 words.
- `program`: 13,913 six-word nodes from the 30-byte-header `N1 Group` program, stored as `{index, words}`. Header fields and source resource ID are retained.

Unknown word meanings and opcode behavior are explicitly marked `requires-disassembly`. Numeric values such as `0xf000` and `0xffff` remain unsigned raw words, so their meaning is not silently changed to a guessed signed value.

These extraction-level raw-word markers are distinct from the implemented runtime: `legacy_vm.rs` supplies the recovered signed-word semantics and native-call dispatch, validated by `original-vm.json`. The top-level event catalog records that its runtime VM has parity tests. See `docs/fidelity.md` for the remaining behavior boundaries.

`assets/catalog/questionnaires.json` retains validated monotonic index offsets and original data references for `fortune.idx/.dat` and `HighTest.idx/.dat`. It includes all 247 `HighTest` questions and their four original answers; answer zero is the original correct answer before the native game shuffles its display position. It also includes all 50 fortune questionnaires with their original titles, prompts, choices, explanation text, percentage vectors, score slices, and thresholds.

Fortune runtime compatibility is evaluated separately from successful text extraction. The original loader leaves score storage uninitialized for records 33 and 37, and record 32 can read an uninitialized percentage row; those three records remain explicitly blocked. The other 47 records preserve the original scoring quirks, including overlapping packed fields and the different branches used for the displayed explanation and applied statistics. `tools/original-fortune-vectors.py` executes the original x86 scoring and clipping instructions to produce 141 comparison cases consumed by the Rust tests. Regenerate the questionnaire catalog from already-extracted resource objects with `node tools/export-questionnaires.mjs`.

The source has mixed text encodings: legacy `.MAP` labels use Big5; actual dialogue and questionnaire text uses GBK. Decoders select the known format's encoding rather than guessing a global disc encoding.

## Rendering and remaining interpretation

BMP metadata includes dimensions, pixel offset, bit depth, compression, and palette size. FLC/FLI metadata retains header magic, frame count, dimensions, bit depth, and raw timing. WAV metadata records the format tag, channels, sample rate, and bit depth. AVI metadata includes the original video FourCC; the observed opening movie uses `IV50` and cannot be assumed playable by an HTML video element.

The ending renderer uses `derived/proepi/aopen02.webm` and `derived/proepi/aopen04.webm`, generated from the original local AVI resources by `tools/transcode-ending-videos.py`. Their source files remain unchanged. Each derived manifest entry records the source resource/hash, FFmpeg version, and encoder arguments; each decoded YUV444 frame is checked against the source before the converted object is accepted. The two lossless VP9 copies add 16,063,074 bytes. The renderer preserves the original 640×180 image, displayed at `(0,60)` with one black scanline between source rows.

The final-battle opening uses `derived/video/ffopening.webm` and a PCM copy at `derived/video/ffopening.wav`. The 544 video frames and audio samples are independently compared with the original Indeo stream. Video timing metadata and the original rendering position remain in `rules.finalBattle`.

`tools/export-epilogue-tables.py` extracts the original ending subtitle timings, video segments, credits paragraphs, and reincarnation buttons into `rules.epilogue`. `tools/original-epilogue-vectors.py` executes the original subtitle and credits update functions for 10,292 comparison frames, replacing graphics/audio devices and the DirectShow clock. Credits scenarios also execute the original keyboard polling and key helpers against a simulated DirectInput device, including rapid S/Escape event sequences. The fixtures use zero-initialized object memory and a relative clock beginning at zero: the original movie constructor does not initialize its previous-subtitle timestamp, so arbitrary original heap contents cannot define a reproducible timing contract.

Original animation bytes are preserved rather than expanded into huge RGBA atlases. Runtime decoders must preserve palette changes, delta frames, and original frame timing. Color-key behavior is not inferred from an attractive-looking palette index: public media entries explicitly set `transparency: "not-established"` until executable evidence identifies the rule.

Visually checked original UI assets:

| Resource ID | Content |
|---|---|
| `install/bmp/pm0201.bmp` | 640×480 original main-menu background |
| `install/bmp/pm0202.bmp` | 155×75 new-game button image with three 25-pixel states |
| `install/bmp/pm0203.bmp`–`pm0205.bmp` | Other main-menu button images with the same dimensions |
| `install/bmp/pm0301.bmp` | 640×480 character-creation background |
| `install/bmp/pm0501.bmp` | 640×480 settings background |
| `install/bmp/pm06a010.bmp` | 640×30 map HUD top bar |
| `install/bmp/pm06a011.bmp` | 4×480 map HUD left border |
| `install/bmp/pm06a012.bmp` | 128×480 map HUD right panel |
| `install/bmp/pm06a013.bmp` | 640×4 map HUD bottom border |

## Format provenance and validation

The InstallShield reader implements this disc's legacy descriptor layout and bounded raw-DEFLATE chunks using the documented source layout in [unshield](https://github.com/twogood/unshield). It supports the observed version `0x01005100`; other cabinet layouts fail explicitly. Deleted entries are recognized before dereferencing their invalid name offsets.

The extractor checks cabinet ranges, compression block bounds, expanded byte lengths, path safety, conflicting resource names, map checksums, tile dimensions, counted-table sizes, N1 node counts, and questionnaire index bounds. Tests additionally verify all extracted map connectivity and coordinate sets, all manifest references and file sizes, selected original resource hashes, and malformed input rejection. These checks establish structural integrity; they do not establish original gameplay equivalence.
