"""Original read-only home and investment scenes (0043b040 / 0042a120)."""
import json, struct, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT/'research/tools/python-packages'))
import pefile
pe = pefile.PE(str(ROOT/'research/original/N2Game.exe'))
base = pe.OPTIONAL_HEADER.ImageBase
manifest = json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects = {row['sha256']:row for row in manifest['objects']}

def number(address,fmt='<i'): return struct.unpack(fmt,pe.get_data(address-base,struct.calcsize(fmt)))[0]
def text(address): return pe.get_string_at_rva(address-base).decode('gbk')
def pointer(address): return text(number(address,'<I'))
def resolve(path):
    path = path.replace('\\','/').lower()
    return next(p for p in [path,'install/'+path,'cd/data/'+path,'cd/'+path] if p in manifest['resources'])
def media(path,x=0,y=0,transparent=None):
    resource = resolve(path)
    obj = objects[manifest['resources'][resource]['sha256']]
    result = {'resource':resource,'x':x,'y':y,'transparent':transparent,**obj['media']}
    if transparent == 'firstPixel':
        raw = (ROOT/obj['path']).read_bytes()
        offset = struct.unpack_from('<I',raw,10)[0]
        width,height = struct.unpack_from('<ii',raw,18)
        bits,compression = struct.unpack_from('<HI',raw,28)
        assert bits == 8 and compression == 0, (resource,bits,compression)
        result['transparent'] = raw[offset+((width+3)//4*4)*(height-1 if height>0 else 0)]
    return result

close = {'autoCloseMs':8000,'autoCloseComparison':'now > enteredAt + 8000',
         'closeInputs':['leftMouseUp','enterKeyDown','escapeKeyUp'],
         'showCursor':False,'backButton':False,'autoCloseClockStartsBeforeTransitions':True,
         'transitionInputs':'No input is polled during either transition.'}
data = {
    'house':{
        'evidence':'0x0043b040','availableWhen':'housing.style >= 0',**close,
        'sound':resolve(pointer(0x491010)),
        'layerRule':'Draw layers 0 through housing.furniture inclusive, in table order.',
        'drawMode':'opaque: every layer calls 0x00458f90, despite setting an unused color key',
        'paletteSource':'styles[housing.style].layers[0]',
        'transitions':[
            {'kind':'rain','evidence':'0x0043b4ad -> 0x00457470','durationSeconds':1,'unitSize':2,
             'initialFront':'black','sourceLayers':'layers.slice(0, housing.furniture)',
             'sleepMs':20,'visibleColumnHeight':'column.finished ? 480 : column.y'},
            {'kind':'mosaic','evidence':'0x0043b528 -> 0x00457260','durationSeconds':2,'unitSize':16,
             'initialFront':'completed rain','sourceLayers':'layers.slice(0, housing.furniture + 1)',
             'durationIgnored':True,'presentation':'All ordered tile copies execute synchronously before return.'},
        ],
        'styles':[
            {'style':style,'layers':[
                media(text(0x490620+style*640+i*40),
                      number(0x490618+style*640+i*40),number(0x49061c+style*640+i*40))
                for i in range(15 if style==3 else 16)]}
            for style in range(4)],
    },
    'investment':{
        'evidence':'0x0042a120','availableWhen':'housing.investmentStage >= 2',**close,
        'sound':resolve(pointer(0x47b3a0)),
        'background':media(pointer(0x47b35c)),
        'paletteSource':'background',
        'transitions':[
            {'kind':'mosaic','evidence':'0x0042a540 -> 0x00457260','durationSeconds':2,'unitSize':16,
             'initialFront':'background','sourceLayers':'background plus current investment stage building',
             'durationIgnored':True,'presentation':'All ordered tile copies execute synchronously before return.'},
        ],
        'stages':[
            {'investmentStage':stage,**media(pointer(0x47b360+(stage-2)*4),
                number(0x47b38c+((stage-2)//3+1)*4,'<h'),
                number(0x47b38e+((stage-2)//3+1)*4,'<h'),'firstPixel')}
            for stage in range(2,14)],
    },
}
path=ROOT/'assets/catalog/rules.json'
rules=json.loads(path.read_text(encoding='utf8'))
rules.setdefault('statusRendering',{}).update(data)
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'homeLayers':[len(s['layers']) for s in data['house']['styles']],
                  'investmentStages':len(data['investment']['stages']),
                  'investmentColorKeys':[s['transparent'] for s in data['investment']['stages']]}))
