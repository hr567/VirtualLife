"""Export the original daily inventory labels, previews and healing percentages."""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile

PE = pefile.PE(str(ROOT / 'research/original/N2Game.exe'))
BASE = PE.OPTIONAL_HEADER.ImageBase
MANIFEST = json.loads((ROOT / 'assets/manifest.json').read_text(encoding='utf8'))
OBJECTS = {item['sha256']: item for item in MANIFEST['objects']}

def i32(address): return struct.unpack('<i', PE.get_data(address - BASE, 4))[0]
def text(address): return PE.get_string_at_rva(address - BASE).decode('gbk')
def item(index, record, descriptions, lines):
    animation = 'install/' + text(record).replace('\\', '/').lower()
    media = OBJECTS[MANIFEST['resources'][animation]['sha256']]['media']
    return {'id': index, 'name': text(record + 30),
            'animation': animation, 'frameCount': media['frames'], 'frameDurationMs': 100,
            'descriptions': [text(i32(descriptions + row * 4)) for row in range(lines)]}

ordinary = []
for index in range(13):
    record = 0x48df40 + index * 70 if index < 10 else 0x48dd98 + (index - 10) * 70
    row = item(index, record, 0x472120 + index * 8, 2)
    row['healingPercent'] = i32((0x48e254 if index < 10 else 0x48e1ec) + index * 4)
    ordinary.append(row)

path = ROOT / 'assets/catalog/rules.json'
catalog = json.loads(path.read_text(encoding='utf8'))
inventory = catalog.setdefault('inventory', {})
inventory.update(ordinary=ordinary,
                 special=[item(index, 0x48faf8 + index * 70, 0x472188 + index * 8, 2) for index in range(27)],
                 equipment=[item(index, 0x48e3f0 + index * 70, 0x472260 + index * 12, 3) for index in range(18)],
                 evidenceFunctions=['0x00404630', '0x00405c60', '0x00406a30'])
path.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print('Exported 13 ordinary, 27 career and 18 equipment inventory definitions')
