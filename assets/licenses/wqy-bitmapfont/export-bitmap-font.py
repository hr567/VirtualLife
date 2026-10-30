"""Build the game's compact WenQuanYi bitmap subset without host font APIs.

The default source is the committed, SHA-256-pinned upstream archive. Its three
medium BDF strikes and copyright files are read directly in memory. An optional
--source-dir also accepts previously extracted source files. Normal client
builds use only the committed catalog and content-addressed font resource.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / 'assets/catalog/glyphs.json'
RESOURCE = 'derived/font/wqy-bitmaps.bin'
SOURCE_RESOURCE = 'derived/font/wqy-source.tar.gz'
SOURCE_URL = 'https://archive.debian.org/debian/pool/main/x/xfonts-wqy/xfonts-wqy_0.9.9.orig.tar.gz'
SOURCE_SHA256 = '16ec5207c2a0b9f555637c0721ec17408d7429b0950f0b728d99a03daae3f630'
SOURCE_BYTES = 6610742
SOURCES = (
    ('wenquanyi_9pt.bdf', 12, 'bc06146b5f9eee183303000aaec79dc1d1fc4acc6fcbab63975f8ee637d1e295'),
    ('wenquanyi_10pt.bdf', 13, '8c17392c6c188a713fbb159d197141323c3868c0431d6347da39fe2df0f29b88'),
    ('wenquanyi_12pt.bdf', 16, '7c5b1c562902efd8defdd6903f00a89c3a51fd482a676dde8cf50f11a8d2275c'),
)
LICENSE_DIRECTORY = ROOT / 'assets/licenses/wqy-bitmapfont'
# Layout categories are shared by every strike and size. These retain the
# game's previous advances, independently of the replacement glyph drawings.
HALF_WIDTH = ''.join(chr(code) for code in range(32, 127)) + 'àáèéêìíòóùúüāēěīōūǎǐǒǔǖǘǚǜ'
NARROW = '\ufffd'
FONT_EXCEPTION = '''As a special exception, if you create a document which uses this
font, and embed this font or unaltered portions of this font into
the document, this font does not by itself cause the resulting
document to be covered by the GNU General Public License. This
exception does not however invalidate any other reasons why the
document might be covered by the GNU General Public License. If you
modify this font, you may extend this exception to your version of
the font, but you are not obligated to do so. If you do not wish to
do so, delete this exception statement from your version.'''


def sha(data):
    return hashlib.sha256(data).hexdigest()


def source_files(archive, directory=None):
    names = [source[0] for source in SOURCES] + ['COPYING', 'AUTHORS']
    if directory is not None:
        return {name: (directory / name).read_bytes() for name in names}
    result = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as source:
        for name in names:
            member = source.getmember(f'wqy-bitmapfont-gb18030/{name}')
            if not member.isfile():
                raise ValueError(f'Font source member must be a regular file: {name}')
            with source.extractfile(member) as stream:
                result[name] = stream.read()
    return result


def collect_strings(value, result):
    if isinstance(value, str):
        result.update(char for char in value if ord(char) >= 32 and not 0xd800 <= ord(char) <= 0xdfff)
    elif isinstance(value, list):
        for item in value:
            collect_strings(item, result)
    elif isinstance(value, dict):
        for item in value.values():
            collect_strings(item, result)


def characters():
    result = set(chr(code) for code in range(32, 127))
    for high in range(0xa1, 0xf8):
        for low in range(0xa1, 0xff):
            try:
                result.add(bytes((high, low)).decode('gb2312'))
            except UnicodeDecodeError:
                pass
    catalog_characters = set()
    for path in sorted((ROOT / 'assets/catalog').glob('*.json')):
        if path.name != 'glyphs.json':
            collect_strings(json.loads(path.read_text(encoding='utf8')), catalog_characters)
    result.update(catalog_characters)
    result.update('?\u25a1\ufffd')
    return ''.join(sorted(result, key=ord)), catalog_characters


def parse_bdf(data, filename, expected_hash, selected):
    if sha(data) != expected_hash:
        raise ValueError(f'Unexpected source SHA-256: {filename}')
    properties = {}
    glyphs = {}
    glyph = None
    bitmap = False
    for line in data.decode('utf8').splitlines():
        fields = line.split()
        if not fields:
            continue
        name = fields[0]
        if name == 'STARTCHAR':
            glyph = {'rows': []}
            bitmap = False
        elif name == 'ENDCHAR':
            code = glyph.get('encoding', -1)
            if code in selected:
                if code in glyphs:
                    raise ValueError(f'Duplicate source encoding: {filename} U+{code:04X}')
                width, height, _, _ = glyph['bbx']
                row_bytes = (width + 7) // 8
                # Some upstream rows keep the wider strike's zero padding even
                # for a narrow BBX. Only the declared BBX pixels are decoded.
                if len(glyph['rows']) != height or any(len(row) < row_bytes for row in glyph['rows']):
                    raise ValueError(f'Invalid BDF rows: {filename} U+{code:04X}')
                glyphs[code] = glyph
            glyph = None
            bitmap = False
        elif glyph is not None:
            if name == 'ENCODING':
                glyph['encoding'] = int(fields[1])
            elif name == 'DWIDTH':
                glyph['advance'] = int(fields[1])
            elif name == 'BBX':
                glyph['bbx'] = tuple(map(int, fields[1:]))
            elif name == 'BITMAP':
                bitmap = True
            elif bitmap:
                glyph['rows'].append(bytes.fromhex(line))
        elif name in ('FONT_ASCENT', 'FONT_DESCENT', 'PIXEL_SIZE'):
            properties[name] = int(fields[1])
    if glyph is not None or not all(name in properties for name in ('FONT_ASCENT', 'FONT_DESCENT', 'PIXEL_SIZE')):
        raise ValueError(f'Incomplete BDF source: {filename}')
    return glyphs, properties, len(data)


def samples(destination, source_size, destination_size):
    """Disjoint OR bins when shrinking; nearest-neighbor when enlarging."""
    if source_size > destination_size:
        return range(
            (destination * source_size + destination_size - 1) // destination_size,
            ((destination + 1) * source_size + destination_size - 1) // destination_size,
        )
    first = destination * source_size // destination_size
    return range(first, first + 1)


def resize_mask(source, source_width, source_height, width, height):
    """Every foreground source point contributes to its shrinking-axis bin.

    In particular, thin horizontal strokes cannot vanish between sampled rows.
    Each shrinking axis partitions its source pixels without overlap, so no
    source point is accidentally thickened into two adjacent output pixels.
    """
    output = bytearray(width * height)
    for y in range(height):
        for x in range(width):
            output[y * width + x] = any(
                source[source_y * source_width + source_x]
                for source_y in samples(y, source_height, height)
                for source_x in samples(x, source_width, width)
            )
    return output


def cell(glyph, metrics, height, width):
    """Place BBX at the font-wide baseline, then uniformly resample the mask.

    The source line canvas includes the whole global ascent and descent. This
    avoids cropping descenders merely because PIXEL_SIZE is smaller than that
    line height. Horizontal normalization uses DWIDTH, never per-glyph tuning.
    """
    box_width, box_height, box_x, box_y = glyph['bbx']
    left = min(0, box_x)
    source_width = max(1, glyph['advance'], box_x + box_width) - left
    source_height = metrics['FONT_ASCENT'] + metrics['FONT_DESCENT']
    source = bytearray(source_width * source_height)
    source_has_ink = any(row[x // 8] & (0x80 >> (x % 8))
                         for row in glyph['rows'] for x in range(box_width))
    top = metrics['FONT_ASCENT'] - box_y - box_height
    for y, row in enumerate(glyph['rows']):
        destination_y = top + y
        if not 0 <= destination_y < source_height:
            continue
        for x in range(box_width):
            destination_x = box_x + x - left
            if 0 <= destination_x < source_width and row[x // 8] & (0x80 >> (x % 8)):
                source[destination_y * source_width + destination_x] = 1
    mask = resize_mask(source, source_width, source_height, width, height)
    if source_has_ink and not any(mask):
        raise ValueError('A visible source glyph became empty during normalization')
    output = bytearray(height * 2)
    for y in range(height):
        for x in range(width):
            if mask[y * width + x]:
                output[y * 2 + x // 8] |= 0x80 >> (x % 8)
    return output


def source_width(char, height):
    half = (height + 1) // 2
    return max(1, half - 1) if char in NARROW else half if char in HALF_WIDTH else height


def write_or_check(path, content, check):
    if check:
        previous = path.read_bytes()
        if path.suffix not in ('.bin', '.gz'):
            # Git text=auto may materialize CRLF on Windows. Text provenance
            # remains identical; BDF/archive inputs and font pixels stay exact.
            previous = previous.replace(b'\r\n', b'\n')
            content = content.replace(b'\r\n', b'\n')
        if previous != content:
            raise ValueError(f'Generated font output differs: {path.relative_to(ROOT)}')
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-dir', type=Path, help='Optional extracted BDF/COPYING/AUTHORS directory; defaults to reading the source archive in memory')
    parser.add_argument('--source-archive', type=Path, default=ROOT / f'assets/game/objects/{SOURCE_SHA256[:2]}/{SOURCE_SHA256}.tar.gz', help='Pinned upstream archive; defaults to the committed content-addressed source resource')
    parser.add_argument('--check', action='store_true', help='Rebuild in memory and verify all saved font outputs')
    arguments = parser.parse_args()
    archive = arguments.source_archive.read_bytes()
    if len(archive) != SOURCE_BYTES or sha(archive) != SOURCE_SHA256:
        raise ValueError('Unexpected original font source archive')
    files = source_files(archive, arguments.source_dir)
    chars, catalog_characters = characters()
    selected = set(map(ord, chars))
    output = bytearray()
    strikes = []
    sources = []
    for filename, height, expected_hash in SOURCES:
        glyphs, metrics, source_bytes = parse_bdf(files[filename], filename, expected_hash, selected)
        if metrics['PIXEL_SIZE'] != height:
            raise ValueError(f'Unexpected strike pixel size: {filename}')
        fallback = glyphs.get(0x25a1) or glyphs.get(63)
        if fallback is None:
            raise ValueError(f'Source has neither square nor question-mark fallback: {filename}')
        missing = [ord(char) for char in chars if ord(char) not in glyphs]
        strikes.append({'height': height, 'offset': len(output), 'rowBytes': 2, 'bytesPerGlyph': height * 2})
        for char in chars:
            normalized = cell(glyphs.get(ord(char), fallback), metrics, height, source_width(char, height))
            if 33 <= ord(char) <= 126 and not any(normalized):
                raise ValueError(f'Printable ASCII became empty: {filename} U+{ord(char):04X}')
            output.extend(normalized)
        sources.append({
            'file': filename, 'sha256': expected_hash, 'bytes': source_bytes,
            'pixelSize': height, 'fontAscent': metrics['FONT_ASCENT'], 'fontDescent': metrics['FONT_DESCENT'],
            'missingSourceCodepoints': missing,
            'missingCatalogCodepoints': [code for code in missing if chr(code) in catalog_characters],
        })
    digest = sha(output)
    path = f'assets/game/objects/{digest[:2]}/{digest}.bin'
    source_path = f'assets/game/objects/{SOURCE_SHA256[:2]}/{SOURCE_SHA256}.tar.gz'
    metadata = {
        'formatVersion': 2, 'resource': RESOURCE, 'pixelFormat': 'mono1',
        'sha256': digest, 'bytes': len(output), 'glyphCount': len(chars),
        'characters': chars,
        'halfWidthCharacters': ''.join(char for char in chars if char in HALF_WIDTH),
        'narrowCharacters': NARROW, 'strikes': strikes,
        'provenance': {
            'font': 'WenQuanYi Bitmap Song', 'version': '0.9.9',
            'upstreamVersionLabel': '0.9.9 (1.0 beta) [Hero], release 8',
            'upstreamRelease': 'https://sourceforge.net/projects/wqy/files/wqy-bitmapfont/0.9.9/',
            'sourceArchive': {'resource': SOURCE_RESOURCE, 'url': SOURCE_URL, 'sha256': SOURCE_SHA256, 'bytes': SOURCE_BYTES},
            'sources': sources,
            'license': 'GPL v2.0 with the upstream font embedding exception',
            'licenseFiles': ['assets/licenses/wqy-bitmapfont/COPYING', 'assets/licenses/wqy-bitmapfont/NOTICE.txt', 'assets/licenses/wqy-bitmapfont/AUTHORS'],
            'method': 'BDF BBX positioned using FONT_ASCENT in a horizontal DWIDTH/BBX envelope by (FONT_ASCENT + FONT_DESCENT) cell; normalize to shared width categories and strike height using disjoint-bin foreground OR on every shrinking axis and nearest-neighbor on enlarging axes; fixed two-byte rows, MSB first, unused bits zero',
            'layout': 'Retain previous advances: full=size, half=floor(size/2), narrow=max(1,floor(size/2)-1), then add one for bold; keep scene line height, spacing and clip unchanged',
            'runtime': 'Nearest native strike (ties prefer the smaller); nearest-neighbor scale to requested size, then horizontal one-pixel bold dilation',
            'fallback': 'Missing source glyphs use the same-strike U+25A1 square or U+003F question mark; unknown runtime characters use the same fallback',
            'boundary': 'An intentionally similar open bitmap typeface; not historical Windows GDI pixels. Original ascfont.16 and stdfont.16 resources remain unchanged.',
        },
    }
    notice = f'''WenQuanYi Bitmap Song 0.9.9 — generated subset for Virtual Life

Copyright (C) 2004-2007, The WenQuanYi Project Board of Trustees and
Qianqian Fang. The original contributors and previous font sources are
credited in the accompanying unmodified AUTHORS file.

Upstream release: {metadata['provenance']['upstreamRelease']}
Upstream source archive: {SOURCE_URL}
Source archive SHA-256: {SOURCE_SHA256}
The source files call this version "0.9.9 (1.0 beta) [Hero], release 8".

Complete upstream source archive on this site:
../../game/objects/{SOURCE_SHA256[:2]}/{SOURCE_SHA256}.tar.gz
Generation source on this site: export-bitmap-font.py
Project source: https://github.com/hr567/VirtualLife

Changes made 2026-10-30: select the game's shared character repertoire
from the original 12, 13 and 16 pixel medium BDF strikes; place each BBX
at the source font-wide baseline, uniformly normalize its line cell to
the requested strike using disjoint-bin foreground OR on shrinking axes
and nearest-neighbor on enlarging axes, and pack fixed cells as one-bit rows.
Missing glyphs
use the source square/question mark. The runtime scales large sizes and
generates bold. No individual glyph drawings are manually adjusted.

The font and generated subset are distributed under GNU GPL version 2
with the upstream font embedding exception. See COPYING for the complete
GPL version 2 text. The following upstream exception is retained and
extended to this generated font subset:

{FONT_EXCEPTION}

The font carries no warranty; see COPYING. This notice describes the
font resource's terms and does not replace licenses of other project
code or original game assets.
'''
    serialized = (json.dumps(metadata, ensure_ascii=False, indent=2) + '\n').encode('utf8')
    write_or_check(ROOT / path, output, arguments.check)
    write_or_check(ROOT / source_path, archive, arguments.check)
    write_or_check(CATALOG, serialized, arguments.check)
    for filename in ('COPYING', 'AUTHORS'):
        write_or_check(LICENSE_DIRECTORY / filename, files[filename], arguments.check)
    write_or_check(LICENSE_DIRECTORY / 'NOTICE.txt', notice.encode('utf8'), arguments.check)
    write_or_check(LICENSE_DIRECTORY / 'export-bitmap-font.py', Path(__file__).read_bytes(), arguments.check)
    manifest_path = ROOT / 'assets/manifest.json'
    manifest = json.loads(manifest_path.read_text(encoding='utf8'))
    old_hash = manifest['resources'].pop('derived/font/gdi-alpha.bin', {}).get('sha256')
    previous_hash = manifest['resources'].get(RESOURCE, {}).get('sha256')
    manifest['resources'][SOURCE_RESOURCE] = {'sha256': SOURCE_SHA256, 'derivation': {'sourceUrl': SOURCE_URL, 'license': metadata['provenance']['license']}}
    manifest['resources'][RESOURCE] = {'sha256': digest, 'derivedFrom': SOURCE_RESOURCE, 'derivation': {'tool': 'tools/export-bitmap-font.py', 'font': metadata['provenance']['font'], 'version': metadata['provenance']['version'], 'license': metadata['provenance']['license']}}
    referenced = {value['sha256'] for value in manifest['resources'].values()}
    removed = [obj for obj in manifest['objects'] if obj['sha256'] in (old_hash, previous_hash) and obj['sha256'] not in referenced]
    manifest['objects'] = [obj for obj in manifest['objects'] if obj not in removed]
    for record in (
        {'sha256': SOURCE_SHA256, 'path': source_path, 'bytes': len(archive), 'media': {'kind': 'font-source-archive'}},
        {'sha256': digest, 'path': path, 'bytes': len(output), 'derivedFrom': SOURCE_SHA256, 'media': {'kind': 'bitmap-font', 'pixelFormat': 'mono1', 'glyphCount': len(chars)}},
    ):
        if not any(obj['sha256'] == record['sha256'] for obj in manifest['objects']):
            manifest['objects'].append(record)
    manifest['objects'].sort(key=lambda value: value['sha256'])
    manifest['catalogs']['glyphs'] = 'assets/catalog/glyphs.json'
    sizes = {value['sha256']: value['bytes'] for value in manifest['objects']}
    source_bytes = sum(sizes[value['sha256']] for value in manifest['resources'].values())
    stored_bytes = sum(sizes.values())
    manifest['statistics'].update(resourceCount=len(manifest['resources']), uniqueObjectCount=len(sizes), sourceBytes=source_bytes, storedBytes=stored_bytes, deduplicatedBytes=source_bytes - stored_bytes, derivedResourceCount=sum(key.startswith('derived/') for key in manifest['resources']))
    write_or_check(manifest_path, (json.dumps(manifest, ensure_ascii=False, indent=2) + '\n').encode('utf8'), arguments.check)
    if not arguments.check:
        for obj in removed:
            (ROOT / obj['path']).unlink(missing_ok=True)
    print(json.dumps({'glyphCount': len(chars), 'strikes': strikes, 'bytes': len(output), 'sha256': digest, 'metadataBytes': len(serialized), 'metadataLines': serialized.count(b'\n'), 'missing': [{'height': source['pixelSize'], 'codepoints': source['missingSourceCodepoints'], 'catalogCodepoints': source['missingCatalogCodepoints']} for source in sources], 'checked': arguments.check}))


if __name__ == '__main__':
    main()
