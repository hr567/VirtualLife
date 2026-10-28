"""Execute original FistGame frame logic with only input, clock and drawing hooked."""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_EBX, UC_X86_REG_EBP

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))
STOP = 0x2000fff0

def write32(vm, addr, value): vm.mem_write(addr, struct.pack('<I', value & 0xffffffff))
def i32(vm, addr): return struct.unpack('<i', vm.mem_read(addr, 4))[0]
def i16(vm, addr): return struct.unpack('<h', vm.mem_read(addr, 2))[0]

FIELDS = {'remainingSeconds':(0x10,4), 'score':(0x14,4), 'secondLastTick':(0x18,4),
          'targetX':(0x38,2), 'targetFrame':(0x42,2), 'targetLastTick':(0x48,4),
          'targetInterval':(0x4c,4), 'resumeState':(0x54,4), 'playerX':(0x58,2),
          'combo':(0x66,2), 'misses':(0x68,2), 'strikeFrame':(0x6a,2),
          'strikeLastTick':(0x70,4), 'playerState':(0x78,4)}

def sequence(frames, initial=None):
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0x400000, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
    vm.mem_write(0x400000, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    vm.mem_map(0x20000000, 0x10000)
    vm.mem_map(0x30000000, 0x1000)
    # Every sprite has a harmless vtable, preserving the original caller stack.
    for index, address in enumerate(range(0x4a536c,0x4a5404,4)):
        obj=0x20000100+index*0x100
        write32(vm,address,obj)
        write32(vm,obj,0x20000000)
    write32(vm,0x20000004,0x30000000)
    write32(vm,0x20000008,0x30000000)
    write32(vm,0x4b6454,0x20008000)
    write32(vm,0x20008004,0x20008100)
    write32(vm,0x20008008,0x20008200)
    write32(vm,0x20008100,0x20000000)
    write32(vm,0x20008200,0x20000000)
    write32(vm,0x46b110,0x30000020)
    index=[-1]
    result=[]
    base=[0]
    def ret(arg_bytes=0,value=0):
        sp=vm.reg_read(UC_X86_REG_ESP)
        vm.reg_write(UC_X86_REG_EAX,value & 0xffffffff)
        vm.reg_write(UC_X86_REG_EIP,i32(vm,sp))
        vm.reg_write(UC_X86_REG_ESP,sp+4+arg_bytes)
    def snapshot():
        row={name:(i16 if width==2 else i32)(vm,base[0]+offset) for name,(offset,width) in FIELDS.items()}
        row['targetState']=vm.reg_read(UC_X86_REG_EBP)
        row['targetSpeed']=struct.unpack('<h',struct.pack('<H',vm.reg_read(UC_X86_REG_EBX)&65535))[0]
        return row
    def hook(vm,address,size,_):
        if address==0x40d68e:
            index[0]+=1
            base[0]=vm.reg_read(UC_X86_REG_ESP)
            if index[0]==0 and initial:
                for name,value in initial.items():
                    if name=='targetState': vm.reg_write(UC_X86_REG_EBP,value)
                    elif name=='targetSpeed': vm.reg_write(UC_X86_REG_EBX,value & 65535)
                    else:
                        offset,width=FIELDS[name]
                        vm.mem_write(base[0]+offset,struct.pack('<H' if width==2 else '<I',value & ((1<<(width*8))-1)))
                # DI carries the current combo on this first frame.
                from unicorn.x86_const import UC_X86_REG_EDI
                if 'combo' in initial: vm.reg_write(UC_X86_REG_EDI,initial['combo'] & 65535)
        elif address==0x40dd43:
            result.append(snapshot())
            if index[0]+1==len(frames) or result[-1]['remainingSeconds']==0: vm.emu_stop()
        elif address==0x30000000: ret(12)
        elif address==0x30000020: ret(value=frames[index[0]]['time'])
        elif address in [0x458ef0,0x45a370,0x4585a0,0x458550]: ret(4)
        elif address==0x458570: ret(8)
        elif address in [0x456c20,0x40e1f0,0x45a3e0,0x4597a0,0x40e660]: ret()
        elif address in [0x4599a0,0x459b60]:
            key=i32(vm,vm.reg_read(UC_X86_REG_ESP)+4)
            field={0xcb:'left',0xcd:'right',0x39:'fire'}.get(key,'none')
            ret(4,int(frames[index[0]].get(field,False)))
        elif address==0x45e47b:
            sp=vm.reg_read(UC_X86_REG_ESP)
            value=i32(vm,sp+4)
            dest=i32(vm,sp+8)
            vm.mem_write(dest,str(value).encode()+b'\0')
            ret(value=dest)
    vm.hook_add(UC_HOOK_CODE,hook)
    write32(vm,0x1001ff00,STOP)
    vm.reg_write(UC_X86_REG_ESP,0x1001ff00)
    vm.emu_start(0x40d580,STOP,count=2000000)
    assert len(result)==len(frames), (len(result),len(frames),hex(vm.reg_read(UC_X86_REG_EIP)))
    return {'initial':initial or {},'frames':[dict(frame,after=after) for frame,after in zip(frames,result)]}

def reward(score,before):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000)
    vm.reg_write(UC_X86_REG_ESP,0x1001ff00)
    from unicorn.x86_const import UC_X86_REG_EDI
    vm.reg_write(UC_X86_REG_EDI,score)
    vm.mem_write(0x4b6490,struct.pack('<h',before))
    vm.emu_start(0x40ddfa,0x40de76,count=1000)
    assert vm.reg_read(UC_X86_REG_EIP)==0x40de76
    delta=i32(vm,vm.reg_read(UC_X86_REG_ESP)+4)
    vm.emu_start(0x40dee3,0x40df89,count=1000)
    return {'score':score,'before':before,'delta':delta,'after':i16(vm,0x4b6490)}

sequences=[sequence([{'time':t,'left':i%180<60,'right':i%180>=120,'fire':i%19==0} for i,t in enumerate(range(16,10001,16))]),
           sequence([{'time':t} for t in [10000,10016,10051,10052,10999,11000]])]
for state in range(7):
    for x in [21,24,468,471]:
        for combo,misses in [(0,0),(2,0),(0,2),(2,2)]:
            initial={'targetState':state,'targetX':x,'combo':combo,'misses':misses,'resumeState':1,'targetFrame':2}
            sequences.append(sequence([{'time':t} for t in [50,51,101,102,153,204,305,406,507]],initial))
for x in [190,191,220,250,251,300]:
    for misses in [0,2]:
        sequences.append(sequence([{'time':t,'fire':t==151} for t in range(151,151*10,151)],{'playerX':258,'targetX':x,'targetSpeed':2,'misses':misses,'targetLastTick':10000}))
for left,right,fire in [(1,0,0),(0,1,0),(1,1,0),(1,0,1),(0,1,1)]:
    for x in [141,142,143,409,410,411]:
        sequences.append(sequence([{'time':151,'left':left,'right':right,'fire':fire}],{'playerX':x}))
out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
     'method':'Original FistGame 0x40d580 frame loop; clock/input/graphics/audio hooks only. All arithmetic and branches execute original x86.',
     'sequences':sequences,
     'rewards':[reward(score,before) for score in [20,29,30,39,40,999,65556] for before in [0,1,99,9500,9999,-100]]}
(ROOT/'tests/fixtures/original-fist.json').write_text(json.dumps(out,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(sequences)} sequences, {sum(len(s["frames"]) for s in sequences)} original x86 frame vectors')
