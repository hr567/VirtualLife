"""Capture original x86 shop stock selection before UI resource construction."""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_EAX,UC_X86_REG_EIP

SOURCE=ROOT/'research/original/N2Game.exe'
PE=pefile.PE(str(SOURCE));BASE=PE.OPTIONAL_HEADER.ImageBase

def case(seed,stage,previous_visits,has_candidate):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(0,0x1000)
    vm.mem_map(BASE,(PE.OPTIONAL_HEADER.SizeOfImage+0xfff)&~0xfff)
    vm.mem_write(BASE,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000)
    stack=0x1001ff00
    vm.mem_write(stack,struct.pack('<I',0x1000));vm.reg_write(UC_X86_REG_ESP,stack)
    vm.mem_write(0x4a3f84,struct.pack('<I',seed))
    vm.mem_write(0x4b649e,struct.pack('<h',stage))
    vm.mem_write(0x4b9ece,bytes([previous_visits]))
    vm.mem_write(0x47ad8c,bytes([0 if has_candidate else 255]+[255]*7))
    draws=[]
    def hook(uc,address,size,user):
        if address==0x45dec1:draws.append(address)
        if address in [0x45d746,0x45a330]:
            sp=uc.reg_read(UC_X86_REG_ESP);ret=struct.unpack('<I',uc.mem_read(sp,4))[0]
            uc.reg_write(UC_X86_REG_EAX,0)
            uc.reg_write(UC_X86_REG_ESP,sp+4+(8 if address==0x45a330 else 0))
            uc.reg_write(UC_X86_REG_EIP,ret)
    vm.hook_add(UC_HOOK_CODE,hook)
    vm.emu_start(0x42b930,0x42bbdf,count=100000)
    assert vm.reg_read(UC_X86_REG_EIP)==0x42bbdf
    groups=struct.unpack('<8i',vm.mem_read(0x4ba0d0,32))
    indices=struct.unpack('<8i',vm.mem_read(0x4ba0f0,32))
    return {'seed':seed,'stage':stage,'previousVisits':previous_visits,'hasCandidate':has_candidate,
            'offers':[{'group':group,'index':index} for group,index in zip(groups,indices)],
            'draws':len(draws),'rngState':struct.unpack('<I',vm.mem_read(0x4a3f84,4))[0],
            'visits':vm.mem_read(0x4b9ece,1)[0]}

cases=[case(seed,stage,visits,candidate) for seed in [0,1,2,42,777,32767]
       for stage in [2,6] for visits in [0,2] for candidate in [False,True]]
result={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        'method':'Unicorn runs original 0x0042b930 through 0x0042bbdf; only allocation and audio loading are hooked',
        'cases':cases}
out=ROOT/'tests/fixtures/original-shops.json'
out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(cases)} original shop-stock cases')
