"""Make browser-playable copies of the original local Indeo movies.

The source resources remain byte-identical; derived records identify their input
hash, exact encoder command, and decoded frame dimensions/duration.
"""
import argparse,hashlib,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--ffmpeg',type=Path,default=ROOT/'research/tools/ffmpeg/ffmpeg-master-latest-win64-gpl/bin/ffmpeg.exe')
parser.add_argument('--only',choices=['all','endings','opening'],default='all')
args=parser.parse_args();ffmpeg=args.ffmpeg.resolve();ffprobe=ffmpeg.with_name('ffprobe.exe'if ffmpeg.suffix=='.exe'else'ffprobe')
manifest_path=ROOT/'assets/manifest.json';manifest=json.loads(manifest_path.read_text(encoding='utf8'))
objects={entry['sha256']:entry for entry in manifest['objects']};temporary=ROOT/'output/ending-videos';temporary.mkdir(parents=True,exist_ok=True)
version=subprocess.check_output([str(ffmpeg),'-version'],text=True).splitlines()[0]
results=[]
movies = [('proepi', 'aopen02'), ('proepi', 'aopen04'), ('video', 'ffopening')]
for folder,name in movies:
    if args.only == 'opening' and name != 'ffopening' or args.only == 'endings' and name == 'ffopening': continue
    source_id=f'cd/data/{folder}/{name}.avi';source_hash=manifest['resources'][source_id]['sha256'];source=ROOT/objects[source_hash]['path']
    streams=json.loads(subprocess.check_output([str(ffprobe),'-v','error','-show_streams','-of','json',str(source)],text=True))['streams']
    probe=next(stream for stream in streams if stream['codec_type']=='video')
    output=temporary/f'{name}.webm'
    options=['-map','0:v:0','-an','-c:v','libvpx-vp9','-lossless','1','-pix_fmt','yuv444p','-g','1','-threads','1','-fflags','+bitexact','-flags:v','+bitexact','-map_metadata','-1']
    subprocess.run([str(ffmpeg),'-hide_banner','-loglevel','error','-y','-i',str(source),*options,str(output)],check=True)
    def frame_hashes(path):
        result=subprocess.check_output([str(ffmpeg),'-v','error','-i',str(path),'-map','0:v:0','-an','-pix_fmt','yuv444p','-f','framemd5','-'],text=True)
        return [line.rsplit(',',1)[1].strip()for line in result.splitlines()if line and not line.startswith('#')]
    assert frame_hashes(source)==frame_hashes(output),f'Lossless video validation failed: {name}'
    data=output.read_bytes();digest=hashlib.sha256(data).hexdigest();relative=f'assets/game/objects/{digest[:2]}/{digest}.webm';destination=ROOT/relative;destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(data)
    metadata={'kind':'webm','videoCodec':'vp9','losslessEncoding':True,'decodedFramesVerified':True,'pixelFormat':'yuv444p','width':probe['width'],'height':probe['height'],'frameRate':probe['r_frame_rate'],'frames':int(probe['nb_frames']),'durationMs':round(float(probe['duration'])*1000)}
    object_record={'sha256':digest,'path':relative,'bytes':len(data),'media':metadata,'derivedFrom':source_hash}
    objects[digest]=object_record;resource_id=f'derived/{folder}/{name}.webm';manifest['resources'][resource_id]={'sha256':digest,'derivedFrom':source_id,'conversion':{'tool':version,'arguments':options}}
    results.append({'resource':resource_id,**metadata,'bytes':len(data)})
    audio = next((stream for stream in streams if stream['codec_type']=='audio'), None)
    if audio:
        audio_output=temporary/f'{name}.wav'
        audio_options=['-map','0:a:0','-vn','-c:a','pcm_s16le','-fflags','+bitexact','-flags:a','+bitexact','-map_metadata','-1']
        subprocess.run([str(ffmpeg),'-v','error','-y','-i',str(source),*audio_options,str(audio_output)],check=True)
        def sample_hash(path):
            return subprocess.check_output([str(ffmpeg),'-v','error','-i',str(path),'-map','0:a:0','-c:a','pcm_s16le','-f','hash','-hash','sha256','-'],text=True).strip()
        assert sample_hash(source)==sample_hash(audio_output),f'Lossless audio validation failed: {name}'
        data=audio_output.read_bytes();digest=hashlib.sha256(data).hexdigest();relative=f'assets/game/objects/{digest[:2]}/{digest}.wav'
        destination=ROOT/relative;destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(data)
        metadata={'kind':'wav','formatTag':1,'channels':audio['channels'],'sampleRate':int(audio['sample_rate']),'bitsPerSample':16,'decodedSamplesVerified':True}
        objects[digest]={'sha256':digest,'path':relative,'bytes':len(data),'media':metadata,'derivedFrom':source_hash}
        resource_id=f'derived/{folder}/{name}.wav';manifest['resources'][resource_id]={'sha256':digest,'derivedFrom':source_id,'conversion':{'tool':version,'arguments':audio_options}}
        results.append({'resource':resource_id,**metadata,'bytes':len(data)})
manifest['objects']=sorted(objects.values(),key=lambda entry:entry['sha256'])
statistics=manifest['statistics'];statistics['resourceCount']=len(manifest['resources']);statistics['uniqueObjectCount']=len(objects);statistics['storedBytes']=sum(entry['bytes']for entry in objects.values())
statistics['sourceBytes']=sum(objects[entry['sha256']]['bytes']for entry in manifest['resources'].values());statistics['deduplicatedBytes']=statistics['sourceBytes']-statistics['storedBytes']
statistics['derivedResourceCount']=sum('derivedFrom'in entry for entry in manifest['resources'].values())
manifest_path.write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps(results,indent=2))
