"""Original x86 spirit placement, digit clocks and slot arithmetic fixtures."""
import hashlib
import json
import struct
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_ECX,UC_X86_REG_EAX,UC_X86_REG_EIP
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));BASE=PE.OPTIONAL_HEADER.ImageBase
OBJECT=0x20000000;STOP=0x2000fff0

def w32(vm,address,value):vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def i32(vm,address):return struct.unpack('<i',vm.mem_read(address,4))[0]
def new():
    vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0,0x1000)
    vm.mem_map(BASE,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(BASE,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000);vm.mem_map(OBJECT,0x10000);vm.mem_map(0x30000000,0x1000)
    return vm
def ret(vm,args=0,value=0):
    stack=vm.reg_read(UC_X86_REG_ESP);target=i32(vm,stack)
    vm.reg_write(UC_X86_REG_EAX,value&0xffffffff);vm.reg_write(UC_X86_REG_ESP,stack+4+args);vm.reg_write(UC_X86_REG_EIP,target)
def call(vm,entry,args=(),stop=STOP):
    stack=0x1001f000;w32(vm,stack,STOP)
    for index,value in enumerate(args):w32(vm,stack+4+index*4,value)
    vm.reg_write(UC_X86_REG_ESP,stack);vm.emu_start(entry,stop,count=200000)
    assert vm.reg_read(UC_X86_REG_EIP)==stop
def positions(vm):return [{'row':i32(vm,0x4b653c+i*4),'column':i32(vm,0x4b6554+i*4)} for i in range(5)]

placement=[]
maps=json.loads((ROOT/'assets/catalog/maps.json').read_text(encoding='utf8'))['maps']
for map_data in maps:
    for seed in [1,2,42,777,2026,32767]:
        vm=new();draws=[0];w32(vm,0x4a3f84,seed);w32(vm,0x4b60e0,OBJECT)
        for offset,value in [(0x18,map_data['start']['row']),(0x1c,map_data['start']['column'])]:w32(vm,OBJECT+offset,value)
        pool=map_data['coordinateIndex'];w32(vm,0x4e7878,len(pool))
        for i,point in enumerate(pool):w32(vm,0x4e70a0+i*8,point['row']);w32(vm,0x4e70a4+i*8,point['column'])
        def hook(uc,address,size,_):
            if address==0x45dec1:draws[0]+=1
            elif address==0x41e880:ret(uc,8)
        vm.hook_add(UC_HOOK_CODE,hook)
        call(vm,0x41e130,stop=0x41e1e9)
        case={'map':map_data['id'],'seed':seed,'initial':positions(vm),'draws':draws[0],'rngState':i32(vm,0x4a3f84)&0xffffffff,'respawns':[]}
        for index in range(5):
            call(vm,0x41e280,[index]);case['respawns'].append({'index':index,'positions':positions(vm),'draws':draws[0],'rngState':i32(vm,0x4a3f84)&0xffffffff})
        placement.append(case)

digits=[]
for count,interval in [(10,40),(4,100)]:
    vm=new();clock=[0]
    for module in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in module.imports:
            if imp.name==b'GetTickCount':w32(vm,imp.address,0x30000000)
    vm.hook_add(UC_HOOK_CODE,lambda uc,address,size,_:ret(uc,value=clock[0]) if address==0x30000000 else ret(uc) if address==0x45a8d0 else None)
    for offset,value in [(0x10,count),(0x28,3),(0x2c,interval),(0x40,0),(0x48,1)]:w32(vm,OBJECT+offset,value)
    frames=[]
    for time in [0,17,40,41,58,81,82,99,100,101,102,999,1000,1001,1300,*range(1320,2500,17)]:
        clock[0]=time;vm.reg_write(UC_X86_REG_ECX,OBJECT);call(vm,0x45a990)
        frames.append({'time':time,'value':i32(vm,OBJECT+0x28),'lastTick':i32(vm,OBJECT+0x40),'initialFrame':bool(i32(vm,OBJECT+0x48))})
    digits.append({'count':count,'interval':interval,'frames':frames})

rewards=[]
for spirit in [2,3,4]:
    for attribute in ([0] if spirit==2 else range(4)):
        for before in ([0,500,99_999_900,2_147_483_600,-2_147_483_600] if spirit==2 else [0,500,9900,32760,-32768]):
            for positive in ([False,True] if spirit==2 else [True]):
                vm=new();stack=0x1001f000;vm.reg_write(UC_X86_REG_ESP,stack)
                digits_value=[9,8,7,6]
                for index,value in enumerate(digits_value):w32(vm,stack+0x28+index*4,value)
                w32(vm,stack+0x3c,attribute);w32(vm,stack+0x40,OBJECT);w32(vm,OBJECT+8,spirit)
                vm.mem_write(stack+0x390,bytes([int(positive)]))
                address=0x4b648a if spirit==2 else [0x4b6492,0x4b6490,0x4b6494,0x4b648e][attribute]
                vm.mem_write(address,struct.pack('<i' if spirit==2 else '<h',before))
                vm.emu_start(0x41fd8e,0x41ff75,count=10000);assert vm.reg_read(UC_X86_REG_EIP)==0x41ff75
                after=struct.unpack('<i' if spirit==2 else '<h',vm.mem_read(address,4 if spirit==2 else 2))[0]
                rewards.append({'spirit':spirit,'attribute':attribute,'before':before,'positive':positive,'digits':digits_value,'after':after})

result={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        'method':'Original placement and respawn execute with only sprite-position drawing hooked; original sprite clock runs with controlled GetTickCount and drawing hook; original slot arithmetic block executes unchanged',
        'placement':placement,'digits':digits,'rewards':rewards}
(ROOT/'tests/fixtures/original-spirits.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(placement)} spirit placement sequences, {sum(len(case["frames"]) for case in digits)} digit frames and {len(rewards)} reward cases')
