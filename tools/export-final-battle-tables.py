"""Export final-battle tables exactly as indexed by the reference executable."""
import json
import re
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile

pe = pefile.PE(str(ROOT / 'research/original/N2Game.exe'))
BASE = pe.OPTIONAL_HEADER.ImageBase
memory = pe.get_memory_mapped_image()
manifest = json.loads((ROOT / 'assets/manifest.json').read_text(encoding='utf8'))
objects = {item['sha256']: item for item in manifest['objects']}

def number(address, fmt): return struct.unpack_from(fmt, memory, address - BASE)[0]
def i16(address): return number(address, '<h')
def i32(address): return number(address, '<i')
def u32(address): return number(address, '<I')
def string(address): return pe.get_string_at_rva(address - BASE).decode('gbk', errors='strict')
def pointer_text(address): return string(u32(address))
def array(address, count, reader=i32, stride=4): return [reader(address + index * stride) for index in range(count)]

def resource(value):
    relative = value.replace('\\', '/').lower()
    matches = [candidate for candidate in [relative, 'install/' + relative, 'cd/data/' + relative, 'cd/' + relative] if candidate in manifest['resources']]
    if not matches: raise ValueError(f'Unknown final battle resource: {value}')
    source = matches[0]
    obj = objects[manifest['resources'][source]['sha256']]
    result = {'resource': source, 'assetPath': source, **obj['media']}
    if obj['media']['kind'] in ['flc', 'fli']:
        speed = obj['media']['speedRaw']
        result.update(frameCount=obj['media']['frames'], frameDurationMs=(speed * 1000 + 69) // 70 if obj['media']['kind'] == 'fli' else speed)
    return result

def resources(address, count): return [resource(pointer_text(address + index * 4)) for index in range(count)]

# Keep the original indexing, including reads that cross conventional symbol
# boundaries. Differential tests, rather than symbol names, define behavior.
data = {
    'evidence': {'initialize': '0x00441a70', 'valuation': '0x00441860',
                 'loop': '0x00443b40', 'combination': '0x00443260'},
    'careerHpMultipliers': array(0x495d28, 26),
    'careerPower': array(0x495d88, 26, i16),
    'enemyStats': [array(0x495da8 + index * 8, 4, i16, 2) for index in range(11, 21)],
    'enemyStatsFirstIndex': 11,
    'cardKinds': [array(0x4962a0 + index * 12, 6, i16, 2) for index in range(4)],
    'comboMultipliers': array(0x4962d8, 6, i16, 2),
    'cardPositions': [array(0x495a40 + index * 4, 2, i16, 2) for index in range(5)],
    'selectedCardPositions': [array(0x495a54 + index * 4, 2, i16, 2) for index in range(5)],
    'shopCardPositions': [array(0x495a68 + index * 4, 2, i16, 2) for index in range(9)],
    'heldShopPositions': [array(0x496060 + index * 4, 2, i16, 2) for index in range(10)],
    'heldBattlePositions': [array(0x496020 + index * 4, 2, i16, 2) for index in range(16)],
    'itemPrices': array(0x495be4, 10),
    'itemDescriptions': [pointer_text(0x495cb0 + index * 4) for index in range(10)],
    'introTexts': [pointer_text(address) for address in [0x495c64, 0x495cac, 0x495c68, 0x495c6c]],
    'shopTexts': {f'0x{address:08x}': pointer_text(address) for address in [0x495c70, *range(0x495c74, 0x495c98, 4), 0x495c9c, 0x495ca0, 0x495ca4]},
    'valuationLabels': [pointer_text(0x495cd8 + index * 4) for index in range(14)],
    'battleIndexLabels': [pointer_text(0x495d10 + index * 4) for index in range(7)],
    'totalValuationLabel': string(0x497ee4),
    'careerNames': [pointer_text(0x476f34 + index * 4) for index in range(26)],
    'unemployedCareerName': pointer_text(0x476f30),
    'valuationLabelPositions': [array(0x495f4c + index * 4, 2, i16, 2) for index in range(15)],
    'valuationValuePositions': [array(0x495f88 + index * 4, 2, i16, 2) for index in range(15)],
    'spouseValues': array(0x495ba4, 16),
    'furnitureValues': [array(0x495ab8 + index * 60, 15) for index in range(4)],
    'animations': {
        'enemyLeft': resources(0x495e60, 10),
        'enemyRight': resources(0x495e88, 9),
        'playerAttacks': resources(0x496160, 13),
        'enemyAttacks': resources(0x496194, 25),
    },
}
data['playerAnimations'] = data['animations']['playerAttacks']
data['enemyAnimations'] = data['animations']['enemyAttacks']
data['introAnimations'] = resources(0x49609c, 2)
data['recoveryAnimation'] = resource(pointer_text(0x4961f8))
data['shieldAnimations'] = resources(0x495eac, 2)
if 'derived/video/ffopening.webm' in manifest['resources']:
    data['openingVideo'] = 'derived/video/ffopening.webm'
    data['openingVideoMetadata'] = objects[manifest['resources'][data['openingVideo']]['sha256']]['media']
    data['openingDurationMs'] = data['openingVideoMetadata']['durationMs']
    data['openingAudio'] = 'derived/video/ffopening.wav'
addresses = set(range(0x495df0, 0x4962a0, 4))
for filename in ['0043fd10.c', '00443b40.c', '00441a70.c', '00443880.c']:
    source = (ROOT / 'research/decompiled/functions' / filename).read_text(encoding='utf8')
    addresses.update(int(value, 16) for value in re.findall(r'PTR_\w+_([0-9a-f]{8})', source))
resources_by_address = {}
for address in sorted(addresses):
    try:
        value = pointer_text(address)
        if re.search(r'\.(bmp|fl[ci]|wav|avi)$', value, re.I): resources_by_address[f'0x{address:08x}'] = resource(value)
    except (UnicodeError, ValueError, IndexError, struct.error):
        pass
data['resourcesByAddress'] = resources_by_address
from final_battle_runtime import capture
data['runtimeObjects'] = capture(pe, resource)
data['runtimeObjectEvidence'] = 'Original 0x0043fd10 executed for both sexes; allocation, resource constructors and graphics calls replaced with argument-capturing stubs.'
runtime = data['runtimeObjects']
data['playerAnimations'] = [runtime[f'0x{0x4e6574 + index*4:08x}'] for index in range(13)]
data['enemyAnimations'] = [runtime[f'0x{0x4e6664 + index*4:08x}'] for index in range(25)]
data['recoveryAnimation'] = runtime['0x004e64f0']
data['shieldAnimations'] = [runtime[f'0x{0x4e6784 + index*4:08x}'] for index in range(2)]
data['introAnimations'] = [runtime['0x004e6484'], runtime['0x004e6480']]
path = ROOT / 'assets/catalog/rules.json'
rules = json.loads(path.read_text(encoding='utf8'))
rules['finalBattle'] = data
path.write_text(json.dumps(rules, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(json.dumps({'enemyStats': data['enemyStats'], 'cardKinds': data['cardKinds'], 'resources': len(resources_by_address)}, ensure_ascii=True))
