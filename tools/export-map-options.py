"""Current-map overview and game-options rendering contracts from the executable."""
import json, struct, sys
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
pe=pefile.PE(str(ROOT/'research/original/N2Game.exe')); base=pe.OPTIONAL_HEADER.ImageBase
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects={row['sha256']:row for row in manifest['objects']}
def number(address,fmt='<i'): return struct.unpack(fmt,pe.get_data(address-base,struct.calcsize(fmt)))[0]
def text(address): return pe.get_string_at_rva(address-base).decode('gbk')
def pointer(address): return text(number(address,'<I'))
def resolve(path):
    path=path.replace('\\','/').lower()
    return next(p for p in [path,'install/'+path,'cd/data/'+path,'cd/'+path] if p in manifest['resources'])
def media(path,x=0,y=0,transparent=None):
    resource=resolve(path); raw=objects[manifest['resources'][resource]['sha256']]['media']
    result={'resource':resource,'x':x,'y':y,'transparent':transparent,**raw}
    if raw['kind'] in ['flc','fli']:
        result.update(frameCount=raw['frames'],interval=(raw['speedRaw']*1000//7)&0xffff if raw['kind']=='fli' else raw['speedRaw'])
    return result
def at(address,x=0,y=0,transparent=None): return media(text(address),x,y,transparent)
def button(address,x,y,transparent=None):
    result=at(address,x,y,transparent);result.update(frameCount=3,frameHeight=result['height']//3)
    return result
def back(x,y): return [at(a,x,y,1) for a in [0x47293c,0x47294c]]

map_format=text(0x473384)
assert map_format == '%s.m%s%s',map_format
maps=[]
for special,count,path_table,columns_table,rows_table in [(False,7,0x47718c,0x477204,0x477220),(True,4,0x4771c4,0x4771e4,0x4771f4)]:
    for index in range(count):
        prefix=pointer(path_table+index*4)
        columns,rows=number(columns_table+index*4),number(rows_table+index*4)
        maps.append({'mapId':prefix.replace('\\','/').split('/')[-1].lower(),'special':special,'index':index,
                     'columns':columns,'rows':rows,'tileWidth':640//columns,'tileHeight':480//rows,
                     'tiles':[{'column':x,'row':y,**media(map_format%(prefix,str(y),str(x)),x*(640//columns),y*(480//rows))}
                              for y in range(rows) for x in range(columns)]})
overview={
    'evidence':'0x00402800','dispatch':'0x004157d0 case 3','maps':maps,
    'closeInputs':['escapeKeyDown','rightMouseUp'],'changesMap':False,'movesCamera':False,
    'showCursor':True,'backgroundSampling':'Each 640x480 source tile scales independently to tileWidth x tileHeight; black remainder is retained.',
    'cameraRect':{'x':'trunc(cameraX / columns)','y':'trunc(cameraY / rows)',
                  'width':'tileWidth','height':'tileHeight','paletteIndices':[242,243,244,245,246],
                  'advanceMs':100,'comparison':'>','initialLastTick':0,'drawBeforeAdvance':True},
    'markers':[at(a,transparent='firstFramePixel') for a in [0x4733ec,0x4733d8,0x4733c4,0x4733b0,0x4733a0,0x473390]],
    'markerIntervalMs':100,'markerLoop':True,'markerDrawOrder':[1,2,3,4,5,0],
    'playerMarker':{'x':'cameraRect.x + trunc(tileWidth / 2) - 24','y':'cameraRect.y + trunc(tileHeight / 2) - 48'},
    'spiritMarkers':{'hideWhen':'special map or life stage 2',
                     'freeX':'trunc(originalSpiritWorldX / columns)','freeY':'trunc(originalSpiritWorldY / rows)',
                     'attachedX':'playerMarker.x','attachedY':'cameraRect.y + trunc(tileHeight / 2) - 72'},
}
options={
    'evidence':'0x00402e70','dispatch':'0x004157d0 case 4','background':at(0x473450),
    'cursor':at(0x47343c,transparent=250),'back':back(543,383),
    'buttons':[{'action':action,**button(address,x,121+49*i)}
               for i,(action,address,x) in enumerate([('volume',0x47342c,270),('save',0x47341c,270),('load',0x47340c,270),('exit',0x4733fc,252)])],
    'volume':{
        'evidence':'0x004032a0','background':at(0x473488),'back':back(505,350),
        'track':at(0x473460,233,224),'thumb':at(0x473474,233,224,1),
        'minimum':0,'maximum':100,'default':50,'channels':'single master volume for all wave audio',
        'nativeSettingFile':text(0x47a660),'nativeSettingAddress':'0x00476f18',
        'settingRead':'0x00425390','settingWrite':'0x00425410',
        'thumbY':'224 - trunc((thumb.height - track.height) / 2)',
        'thumbX':'233 + trunc((track.width - thumb.width) * value / 100)',
        'value':'trunc((thumb.x - 233) * 100 / (track.width - thumb.width))',
        'nativeChannelValue':'trunc(value * 65535 / 100)',
    },
    'save':{'evidence':'0x00403510','background':at(0x4734d0),'back':back(539,371),
            'button':button(0x473498,254,367),'slots':8,'slotTextX':148,'slotTextY':74,'slotSpacing':36,
            'slotHitRect':[144,71,337,96],'slotDoubleClickRect':[144,71,446,92],
            'selectedColor':1,'normalColor':255,
            'confirm':{'background':at(0x4734c0,212,152,1),'yes':button(0x4734b4,249,301,1),'no':button(0x4734a8,345,301,1)}},
    'load':{'evidence':'0x00403c60','background':at(0x473500),'back':back(539,371),
            'button':button(0x4734e0,254,367),'slots':8,'slotTextX':148,'slotTextY':74,'slotSpacing':36,
            'slotHitRect':[144,71,337,96],'slotDoubleClickRect':[144,71,446,92],
            'selectedColor':1,'normalColor':255,
            'confirm':{'background':at(0x4734f0,212,152,1),'yes':button(0x4734b4,249,301,1),'no':button(0x4734a8,345,301,1)}},
    'exit':{'evidence':'0x004043b0','background':at(0x473528),'back':back(505,350),
            'yes':button(0x47351c,239,237,1),'no':button(0x473510,357,237,1),
            'yesKey':'KeyY','noKey':'KeyN'},
}
path=ROOT/'assets/catalog/rules.json'; rules=json.loads(path.read_text(encoding='utf8'))
rules['mapOverview']=overview;rules['optionsRendering']=options
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'overviewMaps':len(maps),'mapIds':[row['mapId'] for row in maps],
                  'optionsActions':[b['action'] for b in options['buttons']],
                  'volumeTrack':[options['volume']['track']['width'],options['volume']['track']['height']],
                  'volumeThumb':[options['volume']['thumb']['width'],options['volume']['thumb']['height']]}))
