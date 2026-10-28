"""Export native card-battle artwork and FLC timing from 0042fd00/0042f400.

Only rules.battle.rendering is replaced; numeric rules have a separate owner.
"""
import json, struct, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
pe = pefile.PE(str(ROOT / 'research/original/N2Game.exe'))
base = pe.OPTIONAL_HEADER.ImageBase
manifest = json.loads((ROOT / 'assets/manifest.json').read_text(encoding='utf8'))
objects = {row['sha256']: row for row in manifest['objects']}

def i16(address): return struct.unpack('<h', pe.get_data(address-base, 2))[0]
def i32(address): return struct.unpack('<i', pe.get_data(address-base, 4))[0]
def u32(address): return struct.unpack('<I', pe.get_data(address-base, 4))[0]
def raw(address): return pe.get_string_at_rva(address-base).decode('gbk')
def resolve(path):
    path = path.replace('\\', '/').lower().lstrip('./')
    while '//' in path: path = path.replace('//', '/')
    for candidate in [path, 'install/'+path, 'cd/data/'+path, 'cd/'+path]:
        if candidate in manifest['resources']: return candidate
    raise ValueError(f'Missing native battle resource: {path}')
def pointer(address): return resolve(raw(u32(address)))
def animation(resource, x=0, y=0, interval=None, multiplier=1, key='firstPixel'):
    asset = objects[manifest['resources'][resource]['sha256']]
    data = (ROOT / asset['path']).read_bytes()
    kind, frames, width, height = struct.unpack_from('<4H', data, 4)
    speed = struct.unpack_from('<H', data, 16)[0]
    # The native 457a80 loader stores a 16-bit interval, including for AF12.
    native = (speed * 1000 // 7) & 0xffff if kind == 0xaf11 else speed
    return {'resource': resource, 'x': x, 'y': y, 'width': width, 'height': height,
            'frameCount': frames, 'interval': native*multiplier if interval is None else interval,
            'transparent': key}
def bitmap(resource, x=0, y=0, key=1):
    media = objects[manifest['resources'][resource]['sha256']]['media']
    return {'resource': resource, 'x': x, 'y': y, 'width': media['width'], 'height': media['height'], 'transparent': key}
def positioned_animations(paths, positions, count, **kwargs):
    return [animation(pointer(paths+n*4), i16(positions+n*4), i16(positions+n*4+2), **kwargs) for n in range(count)]

rendering = {
    'versusDoors': [bitmap(pointer(0x4831fc+n*4),key=1) for n in range(2)],
    'versusEnemyPortraits': [bitmap(pointer(0x482dfc+n*4)) for n in range(15)],
    'versusPlayerPortraits': [[bitmap(pointer(0x48bc4c+(sex*29+n)*4)) for n in range(29)] for sex in range(2)],
    'classNames': [[raw(u32(0x482e74+(sex*31+n)*4)) for n in range(31)] for sex in range(2)],
    'actors': [[[animation(pointer(0x4810c0+(sex*31+actor)*20+pose*4), 428, 149, interval=0 if pose in [1,2,3] else None)
                  for pose in range(5)] for actor in range(31)] for sex in range(2)],
    'enemyActors': [[animation(pointer(0x4815c0+enemy*20+pose*4), 84, 46, interval=0 if pose in [1,2,3] else None,
                              multiplier=6 if pose in [0,4] else 1) for pose in range(5)] for enemy in range(15)],
    'playerStrongEffects': [[animation(resolve(raw(0x480238+(sex*31+actor)*60)), i32(0x48026c+(sex*31+actor)*60)-40,
                                       i32(0x480270+(sex*31+actor)*60), interval=0) for actor in range(31)] for sex in range(2)],
    'enemyStrongEffects': [[animation(resolve(raw(0x4816f0+(enemy*2+effect)*60)), i32(0x481724+(enemy*2+effect)*60)+100,
                                      i32(0x481728+(enemy*2+effect)*60), interval=0) for effect in range(2)] for enemy in range(15)],
    'elementEffects': positioned_animations(0x481598, 0x4815ac, 5, interval=0),
    'cardCombo': positioned_animations(0x482d78, 0x482d64, 5),
    'godEffects': positioned_animations(0x4831d0, 0x4831c4, 3, key=1),
    'playerCardFlips': [animation(pointer(0x48324c), i16(0x47ffd8+n*4), i16(0x47ffda+n*4)) for n in range(5)],
    'enemyCardFlips': [animation(pointer(0x483250), i16(0x47ffec+n*4), i16(0x47ffee+n*4)) for n in range(5)],
    'healthFlares': [animation(pointer(0x48323c),16,45,key=1), animation(pointer(0x483240),477,45,key=1)],
    'shields': positioned_animations(0x4831e4,0x4831f4,2,key=1),
    'effectOverlays': [animation(pointer(0x483214+n*4),key=1) for n in range(10)],
    'backgrounds': [bitmap(pointer(0x4801f8+n*4),key=None) for n in range(9)],
    'backgroundOverlays': [bitmap(pointer(0x48021c+n*4),key=1) for n in range(5)],
    'panels': [bitmap(resolve(raw(0x4800c0+n*40)),i32(0x4800b8+n*40),i32(0x4800bc+n*40)) for n in range(8)],
    'selectedCardPositions': [[i16(0x47ffc4+n*4),i16(0x47ffc6+n*4)] for n in range(5)],
    'comboCardPositions': [[[i16(0x482d8c+(n*5+m)*4),i16(0x482d8e+(n*5+m)*4)] for m in range(5)] for n in range(5)],
    'timeDigits': bitmap(resolve(raw(0x48b190))),
    'cardDescriptions': [[raw(u32(0x48281c+n*8)),raw(u32(0x482820+n*8))] for n in range(4,15)],
    'cardAttackLabel':raw(0x48b1c4),'cardDefenseLabel':raw(0x48b1bc),
    'damagePrefix':raw(0x48b1b4),'damageSuffix':raw(0x48b1b0),
    'bonus':{'money':bitmap(resolve(raw(0x48b1cc)),173,96),'experience':bitmap(resolve(raw(0x48b1f4)),173,96),
             'digits':bitmap(resolve(raw(0x48b20c))),'gain':animation(resolve(raw(0x48b244)),224,158,key=1),
             'loss':animation(resolve(raw(0x48b1e4)),224,158,key=1)},
    'cards': [bitmap(pointer(0x48003c+n*4)) for n in range(15)],
    'selectedCards': [bitmap(pointer(0x480078+n*4)) for n in range(15)],
    'attackButton': bitmap(resolve(raw(0x48b1a0)),465,373),
    'words': [bitmap(pointer(0x483180+n*4),key='firstPixel') for n in range(16)],
    'healthBar': bitmap(pointer(0x4831e0)),
    'cross': bitmap(pointer(0x4831c0)),
    'hpDigits': [bitmap(pointer(0x482fa8+n*4)) for n in range(10)],
    'enemyPortraits': [[bitmap(pointer(0x482c74+(enemy*4+pose)*4)) for pose in range(4)] for enemy in range(15)],
    'careerPortraits': [[[bitmap(pointer(0x4828b4+((sex*30+career)*4+pose)*4)) for pose in range(4)] for career in range(25)] for sex in range(2)],
    'studentPortraits': [[[bitmap(pointer(address+sex*0x78*4+pose*4)) for pose in range(4)] for address in [0x482a84,0x482a54,0x482a64,0x482a74,0x482a44]] for sex in range(2)],
    'sounds': {
        'music':pointer(0x483254),'closeDoor':pointer(0x4831dc),
        'cardSelect': [pointer(0x4831ec+n*4) for n in range(2)],
        'cardCombo': [pointer(0x48316c+n*4) for n in range(5)],
        'cooldownReady': pointer(0x483204), 'shield': pointer(0x483208),
        'card': pointer(0x48320c), 'showCards': pointer(0x483210),
        'enemyDamage': [pointer(0x4827fc+n*4) for n in range(15)],
        'enemyAttack': [[pointer(0x482748+(enemy+effect*15)*4) for effect in range(2)] for enemy in range(15)],
        'enemyDefence': [pointer(0x4827c0+n*4) for n in range(15)],
        'playerAttack': [pointer(0x483000+n*4) for n in range(31)],
        'playerVoices': [[pointer(0x482fd0+(sex*6+n)*4) for n in range(6)] for sex in range(2)],
        'playerCareerVoice': [[pointer(0x48307c+(sex*30+n)*4) for n in range(30)] for sex in range(2)],
    },
    'evidenceFunctions': ['0x0042fd00','0x0042f400','0x00430ac0','0x00433680'],
}
path = ROOT / 'assets/catalog/rules.json'
rules = json.loads(path.read_text(encoding='utf8'))
rules.setdefault('battle', {})['rendering'] = rendering
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'actors':62,'enemyActors':15,'animations':sum(len(row) for sex in rendering['actors'] for row in sex)+75,
                  'renderingKeys':list(rendering)}))
