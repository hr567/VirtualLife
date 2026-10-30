"""Observe the profile's original mode-2 BMP construction conversions.

No window is created and no display mode is changed. Shared definitions are
loaded without executing the existing context/fixture writer.
"""
import hashlib,json,platform,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
observer=ROOT/'tools/export-native-ui-gdi.py'
prefix=observer.read_text(encoding='utf8').split('contexts={}',1)[0]
namespace={'__file__':str(observer),'__name__':'profile_gdi_observer'}
exec(compile(prefix,str(observer),'exec'),namespace)
meta=json.loads((ROOT/'assets/catalog/native-profile.json').read_text(encoding='utf8'))
target=namespace['palette'](meta['background']['resource'])
images=sorted(set(namespace['resources'](meta)))
data={'formatVersion':1,'sourceSha256':meta['sourceSha256'],'platform':platform.platform(),
      'method':'Original LoadImageA(0x2010) -> StretchBlt(SRCCOPY), 8-bit construction palette from pm0301.bmp; 00421490 uses bitmap mode 2. Modern Windows GDI baseline, not a Win9x screenshot.',
      'evidence':['0x00421490','0x004589c0','0x0045cf40','0x0045d150'],
      'targetPalette':list(target),'targetPaletteSha256':hashlib.sha256(target).hexdigest(),
      'images':{resource:namespace['translate'](resource,target)for resource in images}}
destination=ROOT/'assets/catalog/native-profile-gdi.json'
if '--check'in sys.argv:
    if json.loads(destination.read_text(encoding='utf8'))!=data:raise SystemExit('Profile GDI observation differs')
else:destination.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'images':len(images),'nonIdentity':sum(any(a!=b for a,b in v['indexMap'])for v in data['images'].values()),'paletteSha256':data['targetPaletteSha256']}))
for group in ['actors','confirm','cancel']:
    for row in meta[group][:1]:
        palette=namespace['palette'](row['resource'])
        nonblack=[i for i in range(256)if any(palette[i*4:i*4+3])]
        print(json.dumps({'resource':row['resource'],'firstNonblack':min(nonblack),'lastNonblack':max(nonblack),'nonblackColors':len(nonblack),'differentFromBackground':sum(palette[i*4:i*4+3]!=target[i*4:i*4+3]for i in nonblack),'colorsDifferingMoreThanQuantization':sum(any(abs(palette[i*4+c]-target[i*4+c])>3 for c in range(3))for i in nonblack)}))
