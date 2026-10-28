"""Execute both complete original transitions, including native pixel copies.

Only allocation, DirectDraw Lock/Unlock and Sleep are replaced. The original
shuffle, CRT rand, column loop, completion checks and 45aa60/45a9e0 copy routines
execute unchanged. No game source or reference executable is modified.
"""
import hashlib, json, struct, sys, zlib
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_ECX,UC_X86_REG_EAX,UC_X86_REG_EIP

SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE))
OBJECT=0x20000000;FRONT=0x20001000;BACK=0x20002000;TEMP=0x20003000
FRONT_PIXELS=0x21000000;BACK_PIXELS=0x21080000;STOP=0x30000ff0;SLEEP=0x30000000
def u32(vm,address):return struct.unpack('<I',vm.mem_read(address,4))[0]
def write32(vm,address,value):vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def fnv_words(values):
    value=0xcbf29ce484222325
    for word in values:
        for byte in struct.pack('<I',word):value=((value^byte)*0x100000001b3)&0xffffffffffffffff
    return f'{value:016x}'
def pixels(seed,source):
    return bytes(((x*7+y*13+seed) if source else (x^y^seed^0xa5))&255 for y in range(480) for x in range(640))

def sequence(kind,seed,seconds,unit):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x100000);vm.mem_map(OBJECT,0x10000)
    vm.mem_map(FRONT_PIXELS,0x100000);vm.mem_map(0x30000000,0x1000)
    write32(vm,OBJECT+4,FRONT);write32(vm,OBJECT+8,BACK)
    for object_,address in [(FRONT,FRONT_PIXELS),(BACK,BACK_PIXELS)]:
        write32(vm,object_+0x4c,640);write32(vm,object_+0x60,address)
    original_front=pixels(seed,False);source=pixels(seed,True)
    vm.mem_write(FRONT_PIXELS,original_front);vm.mem_write(BACK_PIXELS,source)
    for module in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in module.imports:
            if imp.name==b'Sleep':write32(vm,imp.address,SLEEP)
    write32(vm,0x4a3f84,seed)
    clock=0;draws=0;frames=[];blits=[];all_blits=[];initial=None;order=[];sleep_values=[]
    normalized=(unit if unit in [1,2,4] else 2)if kind=='rain'else(unit if unit in [4,8,16,32]else 16)
    def ret(args=0,value=0):
        stack=vm.reg_read(UC_X86_REG_ESP);target=u32(vm,stack)
        vm.reg_write(UC_X86_REG_EAX,value&0xffffffff)
        vm.reg_write(UC_X86_REG_ESP,stack+4+args);vm.reg_write(UC_X86_REG_EIP,target)
    def capture(columns=False):
        stack=vm.reg_read(UC_X86_REG_ESP)
        result={'time':clock,'rngState':u32(vm,0x4a3f84),'rngDraws':draws,
                'blitsHash':fnv_words(v for row in blits for v in row),'blitCount':len(blits),
                'frontAdler32':zlib.adler32(vm.mem_read(FRONT_PIXELS,640*480))}
        if columns:
            values=list(struct.unpack('<'+'I'*(640//normalized*3),vm.mem_read(stack+0x28,640//normalized*12)))
            result['columnsHash']=fnv_words(values)
            result['finishedColumns']=sum(values[2::3])
        return result
    def hook(vm,address,size,_):
        nonlocal clock,draws,initial,order
        if address==SLEEP:
            delay=u32(vm,vm.reg_read(UC_X86_REG_ESP)+4);sleep_values.append(delay)
            clock=(clock+delay)&0xffffffff;ret(4)
        elif address==0x45df0e:ret(value=TEMP)
        elif address in [0x45dedf,0x458c00,0x458c30]:ret()
        elif address==0x45dec1:draws+=1
        elif address==0x45a9e0:
            stack=vm.reg_read(UC_X86_REG_ESP)
            x,y,right,bottom,surface,_=struct.unpack('<6I',vm.mem_read(stack+4,24))
            assert surface==FRONT
            assert 0<=x<right<=640 and 0<=y<bottom<=480
            rect=[x,y,right-x,bottom-y];blits.append(rect);all_blits.append(rect)
        elif address==0x457511 and kind=='rain':
            initial=capture(True)
        elif address==0x457646 and kind=='rain':
            frames.append(capture(True));blits.clear()
        elif address==0x457330 and kind=='mosaic':
            count=(640//normalized)*(480//normalized)
            stack=vm.reg_read(UC_X86_REG_ESP)
            order=list(struct.unpack('<'+'I'*count,vm.mem_read(stack+0x28,count*4)))
            initial=capture()
        elif address==0x457409 and kind=='mosaic':frames.append(capture())
    for address in [SLEEP,0x45df0e,0x45dedf,0x458c00,0x458c30,0x45dec1,0x45a9e0,0x457511,0x457646,0x457330,0x457409]:
        vm.hook_add(UC_HOOK_CODE,hook,begin=address,end=address)
    stack=0x100fff00;write32(vm,stack,STOP);write32(vm,stack+4,seconds);write32(vm,stack+8,unit)
    vm.reg_write(UC_X86_REG_ESP,stack);vm.reg_write(UC_X86_REG_ECX,OBJECT)
    vm.emu_start(0x457470 if kind=='rain'else 0x457260,STOP,count=50000000)
    assert vm.reg_read(UC_X86_REG_EIP)==STOP
    assert bytes(vm.mem_read(FRONT_PIXELS,640*480))==source
    assert initial is not None and frames
    assert (not sleep_values) if kind=='mosaic'else len(sleep_values)==len(frames)
    if kind=='rain':assert frames[-1]['finishedColumns']==640//normalized
    return {'kind':kind,'seed':seed,'seconds':seconds,'unit':unit,'normalizedUnit':normalized,
            'sleepMs':sleep_values[0]if sleep_values else 0,'initial':initial,'frames':frames,'order':order,
            'allBlitsHash':fnv_words(v for row in all_blits for v in row),'allBlitCount':len(all_blits),
            'finalRngState':u32(vm,0x4a3f84),'finalRngDraws':draws}

cases=[]
for kind,seed,seconds,unit in [
    ('rain',42,1,2),('rain',0,1,2),('rain',1,1,2),('rain',1999,1,2),
    ('rain',0x80000000,1,2),('rain',0xffffffff,1,2),
    ('rain',42,1,1),('rain',42,1,4),('rain',42,1,0),('rain',42,1,3),
    ('rain',42,0,2),('rain',17,2,4),('rain',42,0x7fffffff,2),
    ('mosaic',42,2,16),('mosaic',0,2,16),('mosaic',1,2,16),('mosaic',1999,2,16),
    ('mosaic',0xffffffff,2,16),('mosaic',42,2,4),('mosaic',42,2,8),('mosaic',42,2,32),
    ('mosaic',42,2,0),('mosaic',42,0,16),('mosaic',42,0xffffffff,16),
]:
    case=sequence(kind,seed,seconds,unit);cases.append(case)
    print(f'{kind} seed={seed} seconds={seconds} unit={unit}: {len(case["frames"])} passes / {case["allBlitCount"]} native pixel copies',flush=True)
out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
     'evidence':['0x00457260','0x00457470','0x0045aa60','0x0045a9e0','0x0045dec1'],
     'method':__doc__.strip(),'pixelPattern':{'source':'(x*7+y*13+seed)&255','front':'(x^y^seed^0xa5)&255'},
     'hashFormat':'FNV-1a 64 over little-endian u32 words; columns are [speed,y,finished], blits [x,y,width,height]',
     'cases':cases}
(ROOT/'tests/fixtures/original-transitions.json').write_text(json.dumps(out,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(cases)} original transition cases / {sum(len(c["frames"])for c in cases)} passes / {sum(c["allBlitCount"]for c in cases)} copies')
