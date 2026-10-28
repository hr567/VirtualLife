"""Execute original FLIC decoders with guard regions; record visible bytes and overruns."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UcError,UC_ARCH_X86,UC_MODE_32,UC_HOOK_MEM_READ,UC_HOOK_MEM_WRITE
from unicorn.x86_const import UC_X86_REG_ESP
pe=pefile.PE(str(ROOT/'research/original/N2Game.exe'));base=pe.OPTIONAL_HEADER.ImageBase
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects={row['sha256']:row for row in manifest['objects']}
selected=['cd/data/map/play/h.flc','cd/data/map/play/top.flc','cd/data/map/play/top_h.flc','cd/data/map/play/water_2.flc','cd/data/charani/m24/m24w1c.flc']
def fnv(data):
    result=2166136261
    for byte in data:result=((result^byte)*16777619)&0xffffffff
    return result
cases=[]
for resource in selected:
    digest=manifest['resources'][resource]['sha256'];data=(ROOT/objects[digest]['path']).read_bytes()
    frames,width,height=struct.unpack_from('<HHH',data,6)
    source=0x10000000;dest=0x11000000;stack=0x12000000
    vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(base,(pe.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(base,pe.get_memory_mapped_image())
    for address in [source,dest,stack]:vm.mem_map(address,0x200000)
    native=struct.pack('<5HIH',frames,width,height,71,0,16,0)+data[128:]
    vm.mem_write(source,native)
    over={'sourceReadBytes':0,'sourceReadMaximum':0,'imageWriteBytes':0,'imageWriteMaximum':0}
    def on_read(_vm,_access,address,size,_value,_data):
        over['sourceReadBytes']+=size;over['sourceReadMaximum']=max(over['sourceReadMaximum'],address+size-source-len(native))
    def on_write(_vm,_access,address,size,_value,_data):
        over['imageWriteBytes']+=size;over['imageWriteMaximum']=max(over['imageWriteMaximum'],address+size-dest-width*height)
    vm.hook_add(UC_HOOK_MEM_READ,on_read,begin=source+len(native),end=source+0x1fffff)
    vm.hook_add(UC_HOOK_MEM_WRITE,on_write,begin=dest+width*height,end=dest+0x1fffff)
    expected=[]
    fault=None
    for frame in range(frames):
        sp=stack+0x1fff00;vm.reg_write(UC_X86_REG_ESP,sp);vm.mem_write(sp,struct.pack('<5I',0x1000,source,dest,width,height))
        try: vm.emu_start(0x458030,0x1000,count=3000000)
        except UcError as error:
            fault={'frame':frame,'error':str(error)}
            break
        expected.append({'indicesHash':fnv(vm.mem_read(dest,width*height)),'paletteHash':fnv(vm.mem_read(0x4e78d0,768))})
    cases.append({'resource':resource,'sha256':digest,'width':width,'height':height,'frames':expected,'guard':dict(over),'fault':fault})
    print(resource,frames,over,fault,flush=True)
result={'method':'Original N2Game.exe 0x458030, with input/output guard instrumentation. Zero-initialized buffers; visible indices and six-bit palette only. No OS calls.',
        'sourceSha256':hashlib.sha256((ROOT/'research/original/N2Game.exe').read_bytes()).hexdigest(),'cases':cases}
(ROOT/'tests/fixtures/original-media.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
