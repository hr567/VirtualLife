"""Original candidate selection and two-answer relationship settlement vectors."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_EAX,UC_X86_REG_EIP
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));STOP=0x2000fff0
def write32(vm,address,value):vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def i32(vm,address):return struct.unpack('<i',vm.mem_read(address,4))[0]
def create(seed,contacts):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(0x400000,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x10000);vm.mem_map(0x30000000,0x1000)
    write32(vm,0x4a3f84,seed);vm.mem_write(0x47ad8c,bytes([*contacts,*([255]*(8-len(contacts)))]));vm.mem_write(0x4e5c14,bytes([len(contacts)]))
    for module in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in module.imports:
            if imp.name==b'GetTickCount':write32(vm,imp.address,0x30000000)
    draws=[0]
    def ret(args=0,value=0):
        stack=vm.reg_read(UC_X86_REG_ESP);vm.reg_write(UC_X86_REG_EAX,value)
        vm.reg_write(UC_X86_REG_ESP,stack+4+args);vm.reg_write(UC_X86_REG_EIP,i32(vm,stack))
    def hook(vm,address,size,_):
        if address in [0x45bd90,0x30000000]:ret()
        elif address in [0x45bae0,0x45a370]:ret(4)
        elif address==0x45dec1:draws[0]+=1
    vm.hook_add(UC_HOOK_CODE,hook)
    return vm,draws
def bytes8(vm,address):return [v if v<128 else v-256 for v in vm.mem_read(address,8)]
def candidates(seed,contacts):
    vm,draws=create(seed,contacts);stack=0x1001ff00;write32(vm,stack,STOP);vm.reg_write(UC_X86_REG_ESP,stack)
    vm.emu_start(0x4379f0,STOP,count=100000)
    assert vm.reg_read(UC_X86_REG_EIP)==STOP
    return {'seed':seed,'contacts':contacts,'offers':list(vm.mem_read(0x4e5bf8,4)),'rngState':i32(vm,0x4a3f84)&0xffffffff,'rngDraws':draws[0]}
def settle(sex,candidate,correct,first_correct,attempt,gift,contacts):
    vm,draws=create(42,contacts);stack=0x1001f000;vm.reg_write(UC_X86_REG_ESP,stack)
    vm.mem_write(0x4b6498,struct.pack('<h',sex));write32(vm,0x4b648a,30000);vm.mem_write(0x4b9f4a,bytes([gift]))
    vm.mem_write(0x47ad9c,bytes([7,6,255,255,255,255,255,255]));vm.mem_write(0x4e5bf8,bytes([0,1,2,3]))
    vm.mem_write(stack+0x13,bytes([1 if correct else 0,0,int(first_correct),candidate,attempt]))
    write32(vm,stack+0x18,1);vm.mem_write(stack+0x1e,bytes([2,1]));write32(vm,stack+0x24,1);write32(vm,stack+0x3c,candidate)
    vm.emu_start(0x438efc,0x4390cb,count=100000)
    assert vm.reg_read(UC_X86_REG_EIP)==0x4390cb
    return {'sex':sex,'candidate':candidate,'secondCorrect':correct,'firstCorrect':first_correct,'attempt':attempt,'giftLevel':gift,'contacts':contacts,
            'money':i32(vm,0x4b648a),'afterContacts':bytes8(vm,0x47ad8c),'newTargets':bytes8(vm,0x47ad9c),
            'attempts':vm.mem_read(stack+0x17,1)[0],'noticeKind':vm.mem_read(stack+0x12,1)[0],
            'returnKind':vm.mem_read(0x4e5b88,1)[0],'offers':list(vm.mem_read(0x4e5bf8,4)),
            'rngState':i32(vm,0x4a3f84)&0xffffffff,'rngDraws':draws[0]}
out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
     'method':'Original 0x4379f0 candidate loop and 0x438efc answer/settlement instructions; sound/text/clock are hooked',
     'candidates':[candidates(seed,contacts) for seed in [0,1,42,1999,0xffffffff] for contacts in [[],[0],[3,1,7],[0,2,4,6]]],
     'settlements':[settle(sex,candidate,correct,first,attempt,gift,contacts) for sex in range(2)
                    for candidate,contacts in [(0,[]),(5,[0,2,7])]
                    for correct,first in [(False,False),(True,False),(False,True),(True,True)] for attempt,gift in [(0,1),(0,2),(1,2)]]}
(ROOT/'tests/fixtures/original-relationships.json').write_text(json.dumps(out,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(out["candidates"])} offer sets and {len(out["settlements"])} answer settlements')
