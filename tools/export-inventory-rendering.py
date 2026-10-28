"""Original item-menu artwork and coordinates from 00404630/405c60/406a30."""
import json, struct, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
pe = pefile.PE(str(ROOT / 'research/original/N2Game.exe'))
base = pe.OPTIONAL_HEADER.ImageBase
manifest = json.loads((ROOT / 'assets/manifest.json').read_text(encoding='utf8'))
objects = {row['sha256']: row for row in manifest['objects']}

def raw(address): return pe.get_string_at_rva(address-base).decode('gbk')
def resolve(path):
    path = path.replace('\\', '/').lower().lstrip('./')
    for candidate in [path, 'install/'+path, 'cd/data/'+path, 'cd/'+path]:
        if candidate in manifest['resources']: return candidate
    raise ValueError('Missing inventory resource: '+path)
def bitmap(path, x=0, y=0, key=1):
    resource = resolve(path); media = objects[manifest['resources'][resource]['sha256']]['media']
    return {'resource': resource, 'x': x, 'y': y, 'width': media['width'], 'height': media['height'], 'transparent': key}
def animation(path, x=0, y=0, interval=None):
    resource = resolve(path); obj = objects[manifest['resources'][resource]['sha256']]
    data = (ROOT/obj['path']).read_bytes(); kind, frames, width, height = struct.unpack_from('<4H', data, 4)
    speed = struct.unpack_from('<H', data, 16)[0]
    native = (speed*1000//7)&0xffff if kind == 0xaf11 else speed
    return {'resource':resource,'x':x,'y':y,'width':width,'height':height,'frameCount':frames,'interval':native if interval is None else interval,'transparent':1}

rendering = {
    'evidence': ['0x00404630','0x00405c60','0x00406a30','0x0040e580'],
    'backgrounds': [bitmap(raw(address),key=None) for address in [0x4735c8,0x4735e8,0x473638]],
    'rows': bitmap(raw(0x4735b8),43,80),
    'buttons': [bitmap(raw(address),x,y) for address,x,y in [(0x473588,31,3),(0x473578,98,3),(0x473568,165,3),(0x473558,316,354),(0x473548,403,354)]],
    'equipmentButtons': [bitmap(raw(address),x,y) for address,x,y in [(0x473624,316,354),(0x473610,316,384),(0x4735fc,403,354)]],
    'hpLabel':bitmap(raw(0x4735a8),39,425),'hpFrame':bitmap(raw(0x473598),66,428),'hpFill':bitmap(raw(0x473538),69,430),
    'confirmation':bitmap(raw(0x4735d8),212,152),
    'confirmYes':bitmap(raw(0x4734b4),249,301),'confirmNo':bitmap(raw(0x4734a8),345,301),
    'back': [animation(raw(address),543,383) for address in [0x47293c,0x47294c]],
    'emptyEquipmentLabel':raw(0x4735f8),
    'asciiFont':resolve('font/ascfont.16'),'chineseFont':resolve('font/stdfont.16'),
}
path=ROOT/'assets/catalog/rules.json'
rules=json.loads(path.read_text(encoding='utf8'))
rules.setdefault('inventory',{})['rendering']=rendering
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps(rendering,ensure_ascii=True))
