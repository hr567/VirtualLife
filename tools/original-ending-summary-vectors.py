"""Execute the original life-history layout, scroll, FLC clocks and palette updates.

Only allocation, history access, input/clock, GDI metrics and device presentation
are stubbed. The zeroed-allocation and relative-clock-zero baseline is explicit:
native empty promotion history leaves one record's coordinates uninitialized.
"""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import *
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));RULES=json.loads((ROOT/'assets/catalog/rules.json').read_text(encoding='utf8'))['endingSummary']
OBJ=0x20000000;STACK=0x10010000;STOP=0x30000ff0

def w32(vm,a,v):vm.mem_write(a,struct.pack('<I',v&0xffffffff))
def i32(vm,a):return struct.unpack('<i',vm.mem_read(a,4))[0]
def string(vm,a):
    result=bytearray()
    while True:
        value=vm.mem_read(a+len(result),1)[0]
        if value==0:return bytes(result)
        result.append(value)
def fnv(data):
    value=2166136261
    for b in data:value=((value^b)*16777619)&0xffffffff
    return value
FIELDS={'scrollY':0x6b8,'actorY':0x6bc,'lastScrollMs':0x6c0,'speed':0x6c4,'lastFlashMs':0x56d6c,'fadeStartedMs':0x56d78}
FLAGS={'scrolling':0x56964,'showText':0x56965,'actorEntering':0x56966,'flashing':0x56d67,'flashDirty':0x56d68,'flashAlternate':0x56d69,
       'waiting':0x56d70,'graying':0x56d71,'grayPause':0x56d72,'finished':0x56d73,'fadingIn':0x56d74}

def machine(sex,career,victory,history):
    vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0,0x1000);vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x400000);vm.mem_map(0x30000000,0x1000)
    vm.reg_write(UC_X86_REG_FPCW,0x023f)
    heap=[0x20100000];clock=[0];keys=[False,False,False];displayed=[bytes(1024)]
    base=bytes(v for row in RULES['palettes'][sex][career]for v in row);vm.mem_write(OBJ+0x56564,base)
    w32(vm,OBJ+8,career);vm.mem_write(OBJ+4,bytes([int(victory)]));vm.mem_write(0x4b6498,struct.pack('<h',sex));w32(vm,OBJ+0x6ac,1)
    for off,value in [(0x1c,0),(0x24,30),(0x28,0),(0x30,1),(0x34,50)]:w32(vm,OBJ+0x16c+off,value)
    # Device object -> backbuffer wrapper -> fake surface, and fake palette.
    w32(vm,0x4b6454,0x20080000);w32(vm,0x20080008,0x20080100);w32(vm,0x200801a8,0x20080200)
    w32(vm,0x20080200,0x20080300);w32(vm,0x20080368,0x30000050)
    w32(vm,0x20080010,0x20080400);w32(vm,0x20080400,0x20080500);w32(vm,0x20080518,0x30000060)
    w32(vm,0x4b640c,0x20080600);w32(vm,0x46b110,0x30000020);w32(vm,0x46b054,0x30000030)
    for dll in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in dll.imports:
            if imp.name==b'DrawTextA':w32(vm,imp.address,0x30000040)
    def ret(args=0,value=0):
        sp=vm.reg_read(UC_X86_REG_ESP);vm.reg_write(UC_X86_REG_EIP,i32(vm,sp));vm.reg_write(UC_X86_REG_ESP,sp+4+args);vm.reg_write(UC_X86_REG_EAX,value&0xffffffff)
    def hook(vm,address,size,_):
        sp=vm.reg_read(UC_X86_REG_ESP)
        if address==0x45d746:
            count=i32(vm,sp+4);pointer=heap[0];heap[0]+=(count+15)&~15;ret(value=pointer)
        elif address==0x45d754:ret()
        elif address==0x4193a0:ret(value=len(history[(vm.reg_read(UC_X86_REG_ECX)-0x4b6410)//4]))
        elif address==0x419370:ret(4,history[(vm.reg_read(UC_X86_REG_ECX)-0x4b6410)//4][i32(vm,sp+4)])
        elif address==0x45db32:
            destination=i32(vm,sp+4);template=string(vm,i32(vm,sp+8));value=template%i32(vm,sp+12);vm.mem_write(destination,value+b'\0');ret(value=len(value))
        elif address==0x30000020:ret(value=clock[0])
        elif address==0x30000030:ret(8,1)
        elif address==0x30000040:ret(20,RULES['lineHeight'])
        elif address==0x30000050:ret(8)
        elif address==0x30000060:displayed[0]=bytes(vm.mem_read(i32(vm,sp+20),1024));ret(20)
        elif address in [0x4597a0,0x458610]:ret()
        elif address==0x459b60:ret(4,int(keys[0]))
        elif address==0x459ba0:ret(4,int(keys[2]if i32(vm,sp+4)==1 else keys[1]))
    for address in [0x45d746,0x45d754,0x4193a0,0x419370,0x45db32,0x30000020,0x30000030,0x30000040,0x30000050,0x30000060,0x4597a0,0x458610,0x459b60,0x459ba0]:
        vm.hook_add(UC_HOOK_CODE,hook,begin=address,end=address)
    # Enter after resource loading. Run native allocation/count/formatting/gender
    # replacement/0044cd70 line wrapping and the entire state initialization.
    vm.reg_write(UC_X86_REG_EBP,OBJ);vm.reg_write(UC_X86_REG_ESP,STACK);vm.emu_start(0x44d875,0x44dbfd,count=10000000)
    records=[];pointer=i32(vm,OBJ+0x6b0);allocated=i32(vm,OBJ+0x6b4)
    for i in range(allocated):
        p=pointer+i*32;text_pointer=i32(vm,p+4)
        records.append({'bytes':list(string(vm,text_pointer))if text_pointer else[],'x':i32(vm,p+12),'y':i32(vm,p+16),'height':i32(vm,p+28)})
    return vm,clock,keys,displayed,records

def sequence(sex,career,victory,history,step,mode,max_frames=15000):
    vm,clock,keys,displayed,records=machine(sex,career,victory,history);rows=[]
    for frame in range(max_frames):
        clock[0]=(frame+1)*step
        s_down=mode=='fast'and frame==0 or mode=='holdRelease'and frame in [10,40]
        s_up=mode=='holdRelease'and frame in [20,50]
        skip=(mode=='earlySkip'and frame==0)or(mode=='lateSkip'and clock[0]>=2500 and clock[0]-step<2500)or(mode=='holdSkip'and clock[0]>=2600)
        keys[:]=[s_down,s_up,skip]
        vm.reg_write(UC_X86_REG_ECX,OBJ);vm.reg_write(UC_X86_REG_ESP,STACK);w32(vm,STACK,STOP)
        vm.emu_start(0x44dd70,STOP,count=10000000)
        keep=vm.reg_read(UC_X86_REG_EAX)&255
        if keep:
            # Execute the original renderer's actual palette selection. Surface
            # painting is outside this state oracle; there is no inferred LUT.
            vm.reg_write(UC_X86_REG_ESI,OBJ);vm.reg_write(UC_X86_REG_EBX,0);vm.reg_write(UC_X86_REG_ESP,STACK)
            vm.emu_start(0x44e6c4,0x44e780,count=10000000)
        row={name:i32(vm,OBJ+off)for name,off in FIELDS.items()};row.update({name:bool(vm.mem_read(OBJ+off,1)[0])for name,off in FLAGS.items()})
        row['actor']=[i32(vm,OBJ+0x16c+off)for off in [0x1c,0x28,0x30]]
        row['paletteHash']=fnv(displayed[0]);row['basePaletteHash']=fnv(vm.mem_read(OBJ+0x56564,1024));row['alternatePaletteHash']=fnv(vm.mem_read(OBJ+0x56967,1024))
        rows.append({'sDown':s_down,'sUp':s_up,'skip':skip,'after':row})
        if not keep:break
    assert not keep,(sex,career,mode,len(rows))
    return {'sex':sex,'career':career,'victory':victory,'history':history,'step':step,'mode':mode,'paragraphs':records,'frames':rows}

if __name__=='__main__':
    basic=[[]for _ in range(16)];basic[1]=[2016];basic[2]=[2019];basic[7]=[2030,2035];basic[14]=[15];basic[15]=[2060]
    full=[[2010+i*2,2050+i,2100+i,2150+i,2200+i]for i in range(16)]
    empty=[[]for _ in range(16)];empty[1]=[2016];empty[2]=[2019];empty[15]=[2025]
    cases=[]
    for sex in range(2):
        for victory in [False,True]:
            for mode,step in [('fast',41),('earlySkip',16),('lateSkip',101),('holdRelease',150),('holdSkip',71)]:
                cases.append(sequence(sex,sex*10,victory,basic,step,mode));print(f'Checked summary sex={sex} victory={victory} mode={mode}',flush=True)
    cases.append(sequence(0,25,True,empty,41,'normal'));cases.append(sequence(1,25,False,full,71,'fast'))
    layouts=[]
    for sex in range(2):
        for victory in [False,True]:
            for career in range(26):
                _,_,_,_,rows=machine(sex,career,victory,full if career%2 else basic)
                layouts.append({'sex':sex,'career':career,'victory':victory,'history':full if career%2 else basic,'paragraphs':rows})
    output={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),'method':'Original 0044d875 text construction + 0044cd70 byte wrapping; full 0044dd70 updates/FLC timers and 0044e6c4 displayed palette selection. Zeroed heap and initial clock 0; GDI height measured on host.','cases':cases,'layouts':layouts}
    output['fields']=list(cases[0]['frames'][0]['after'])
    for case in cases:
        for frame in case['frames']:frame['after']=[frame['after'][field]for field in output['fields']]
    (ROOT/'tests/fixtures/original-ending-summary.json').write_text(json.dumps(output,separators=(',',':'))+'\n',encoding='utf8')
    print(f'{len(layouts)} original summary layouts and {sum(len(c["frames"])for c in cases)} frames across {len(cases)} cases')
