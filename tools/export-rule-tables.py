"""Extract verified data tables from the local reference executable."""
import hashlib
import json
import struct
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
source = ROOT / 'research/original/N2Game.exe'
pe = pefile.PE(str(source))
base = pe.OPTIONAL_HEADER.ImageBase
def read(address, size): return pe.get_data(address - base, size)
def u32(address): return struct.unpack('<I', read(address, 4))[0]
def i32(address): return struct.unpack('<i', read(address, 4))[0]
def i16(address): return struct.unpack('<h', read(address, 2))[0]
def text(address): return pe.get_string_at_rva(address - base).decode('gbk', errors='strict')
stages = []
for i in range(7):
    stages.append({'id': i, 'mapPath': text(u32(0x47718c + i * 4)).replace('\\', '/'),
                   'musicPath': text(u32(0x4771a8 + i * 4)).replace('\\', '/'),
                   'backgroundColumns': i32(0x477204 + i * 4),
                   'backgroundRows': i32(0x477220 + i * 4),
                   'stageTurns': i32(0x47723c + i * 4)})
result = {'formatVersion': 1, 'sourceSha256': hashlib.sha256(source.read_bytes()).hexdigest(),
          'evidence': {'stageMapPaths': '0x0047718c', 'stageMusicPaths': '0x004771a8',
                       'backgroundColumns': '0x00477204', 'backgroundRows': '0x00477220',
                       'stageTurns': '0x0047723c', 'initializePlayer': '0x004144c0',
                       'turnProgression': '0x004159a0', 'statDisplay': '0x00401550'},
          'stages': stages,
          'tableSegments': [
              {'baseAddress': '0x004738fc', 'wordWidth': 2,
               'values': [i16(0x4738fc + i * 2) for i in range(14)]},
              {'baseAddress': '0x00477574', 'wordWidth': 2,
               'values': [i16(0x477574 + i * 2) for i in range(622)]},
          ],
          'skills': [
              {'id': i, 'trigger': i32(0x479ab8 + i * 16),
               'attribute': i32(0x479abc + i * 16),
               'levelValues': [i16(0x479ac0 + i * 16 + level * 2) for level in range(4)],
               'name': text(u32(0x476f34 + i * 4)),
               'evidenceFunction': '0x0041c2a0'} for i in range(26)
          ],
          'characters': {'directories': [text(u32(0x477448 + i * 4)).replace('\\', '/') for i in range(62)],
                         'animationSuffixes': [text(u32(0x472060 + i * 4)) for i in range(12)],
                         'portraits': [text(u32(0x477350 + i * 4)).replace('\\', '/') for i in range(62)],
                         'evidenceFunction': '0x00401000',
                         'transparentColor': 'First decoded frame pixel at (0,0), sampled before playback'},
          'playerFields': {'0x004b6484': 'maxHp', '0x004b6486': 'hp', '0x004b6488': 'experience',
                           '0x004b648a': 'money', '0x004b648e': 'stamina', '0x004b6490': 'morality',
                           '0x004b6492': 'intelligence', '0x004b6494': 'charm', '0x004b6496': 'affection',
                           '0x004b6498': 'sex', '0x004b649a': 'age', '0x004b649c': 'year',
                           '0x004b649e': 'stage', '0x004b64a2': 'profession', '0x004b64a4': 'lifeLevel',
                           '0x004b64a6': 'professionLevel', '0x004b64a8': 'stageTurn', '0x004b64aa': 'totalTurn'}}
out = ROOT / 'assets/catalog/rules.json'
out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(json.dumps(stages, ensure_ascii=False, indent=2))
