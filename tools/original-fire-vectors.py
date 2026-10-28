"""Original Fire mini-game state and CRT RNG differential vectors."""
import hashlib
import json
import struct
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_EAX,UC_X86_REG_ECX,UC_X86_REG_EIP,UC_X86_REG_FPCW
SOURCE=ROOT/'research/original/N2Game.exe'
PE=pefile.PE(str(SOURCE))
OBJECT=0x20000000
STOP=0x2000fff0
def static_words(address,count): return list(struct.unpack('<'+'i'*count,PE.get_data(address-0x400000,count*4)))
TABLES={
 'keys':static_words(0x46b318,21),
 'bodyIdle':static_words(0x46b36c,7),
 'bodyRandom':static_words(0x46b388,32),
 'bodyLost':static_words(0x46b408,4),
 'bodyWon':static_words(0x46b418,8),
 'sparksStart':static_words(0x46b438,4),
 'sparksLoop':static_words(0x46b448,4),
 'sparksEnd':static_words(0x46b458,2),
 'lightning':static_words(0x46b460,8),
 'positions':static_words(0x46b480,38),
 'cellSizes':static_words(0x46b628,8),
 'lossPositions':static_words(0x46b518,68),
 'actorMotion':static_words(0x46b848,15),
 'shakeY':static_words(0x46b884,5),
}
FIELDS={
 'failed':(4,1),'finished':(5,1),'duration':(8,4),'gauge':(12,4),
 'startedAt':(0x10,4),'elapsed':(0x14,4),'wonAtElapsed':(0x18,4),'fadeAt':(0x1c,4),
 'hand':(0x420,1),'mouseArmed':(0x421,1),'bodyState':(0x424,4),'bodyRest':(0x428,4),
 'bodyFrame':(0x42c,4),'bodyAt':(0x430,4),'sparksState':(0x434,4),
 'sparksFrame':(0x438,4),'sparksAt':(0x43c,4),'sparksPending':(0x440,1),
 'actorType':(0x444,4),'actorX':(0x448,4),'actorY':(0x44c,4),'wonX':(0x450,4),'wonY':(0x454,4),
 'actorStep':(0x458,4),'actorAt':(0x45c,4),'actorHold':(0x460,1),'winStage':(0x464,4),
 'lightningActive':(0x468,1),'lightningFrame':(0x46c,4),'lightningAt':(0x470,4),
 'fireFlash':(0x484,1),'flashAt':(0x488,4),'flashDelay':(0x48c,4),
 'shakeActive':(0x790,1),'shakeAt':(0x794,4),'shakeFrame':(0x798,4),'shakeOffset':(0x79c,4),
 'helpState':(0x7a0,4),'helpAt':(0x7a4,4),'helpX':(0x7a8,4),'helpY':(0x7ac,4),
 'winBanner':(0x7b0,1),'bannerAt':(0x7b4,4),'lossBanner':(0x7b8,1),'lossFrame':(0x7bc,4),'lossAt':(0x7c0,4),
 'lossAnimation':(0x7c4,1),'intro':(0x7c5,1),'introAt':(0x7c8,4),
 'randomOverlay':(0x7cc,1),'randomAt':(0x7d0,4),'rewardStarted':(0x7d4,1),'rewardPercent':(0x7d8,4),
}
TRACKS=[(0x15f0,8,80),(0x1a2c,8,80),(0x1e68,12,40),(0x22a4,5,100),(0x26e0,2,100),(0x2b1c,2,50)]
def write32(vm,address,value): vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def i32(vm,address): return struct.unpack('<i',vm.mem_read(address,4))[0]

def create_vm(seed,start=0):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    # Original CRT 0x46097d calls _controlfp(0x10000,0x30000), selecting
    # 53-bit precision. Unicorn's otherwise-zero control word selects 24-bit.
    vm.reg_write(UC_X86_REG_FPCW,0x023f)
    vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000)
    vm.mem_map(OBJECT,0x10000)
    vm.mem_map(0x30000000,0x1000)
    write32(vm,0x46b110,0x30000000)
    write32(vm,0x4a3f84,seed)
    for name,value in {'duration':20500,'startedAt':start,'actorAt':start,'introAt':start,
                       'mouseArmed':1,'intro':1,'actorX':static_words(0x46b498,1)[0]+TABLES['actorMotion'][0],
                       'actorY':static_words(0x46b49c,1)[0]+TABLES['actorMotion'][1]}.items():
        offset,width=FIELDS[name]
        vm.mem_write(OBJECT+offset,value.to_bytes(width,'little'))
    write32(vm,OBJECT+0x13e8,348)
    write32(vm,0x4b6454,OBJECT+0x4000)
    write32(vm,OBJECT+0x4008,OBJECT+0x5000)
    write32(vm,OBJECT+0x50a8,OBJECT+0x6000)
    write32(vm,OBJECT+0x6000,OBJECT+0x7000)
    write32(vm,OBJECT+0x7014,0x30000010)
    write32(vm,OBJECT+0x7dc+0x194,OBJECT+0x8000)
    write32(vm,OBJECT+0x7dc+0x1c,10)
    write32(vm,0x4b640c,OBJECT+0x9000)
    for offset,count,period in TRACKS:
        for field,value in [(0x24,count),(0x30,1),(0x34,period)]:write32(vm,OBJECT+offset+field,value)
    vm.mem_write(0x4b6494,struct.pack('<h',1000))
    clock=[start]
    draws=[0]
    def ret(args=0,value=0):
        sp=vm.reg_read(UC_X86_REG_ESP)
        vm.reg_write(UC_X86_REG_EIP,i32(vm,sp))
        vm.reg_write(UC_X86_REG_ESP,sp+4+args)
        vm.reg_write(UC_X86_REG_EAX,value&0xffffffff)
    def hook(vm,address,size,_):
        if address==0x30000000:ret(value=clock[0])
        elif address==0x30000010:ret(24)
        elif address==0x45a370:ret(4)
        elif address==0x458610:ret()
        elif address in [0x4597a0,0x45a420]:ret()
        elif address==0x459ba0:ret(4)
        elif address==0x4585c0 and vm.reg_read(UC_X86_REG_ECX) not in [OBJECT+offset for offset,_,_ in TRACKS]:ret()
        elif address==0x45dec1:draws[0]+=1
    vm.hook_add(UC_HOOK_CODE,hook)
    return vm,clock,draws

def call(vm,address,obj=OBJECT):
    write32(vm,0x1001ff00,STOP)
    vm.reg_write(UC_X86_REG_ESP,0x1001ff00)
    vm.reg_write(UC_X86_REG_ECX,obj)
    vm.emu_start(address,STOP,count=20000)
    assert vm.reg_read(UC_X86_REG_EIP)==STOP,hex(vm.reg_read(UC_X86_REG_EIP))
    return vm.reg_read(UC_X86_REG_EAX)&255

def snapshot(vm,draws):
    result={name:(int.from_bytes(vm.mem_read(OBJECT+offset,1),'little') if width==1 else i32(vm,OBJECT+offset)) for name,(offset,width) in FIELDS.items()}
    result['tracks']=[{'frame':i32(vm,OBJECT+offset+0x1c),'lastTick':i32(vm,OBJECT+offset+0x28),'first':i32(vm,OBJECT+offset+0x30)} for offset,_,_ in TRACKS]
    result['rngState']=i32(vm,0x4a3f84)&0xffffffff
    result['rngDraws']=draws[0]
    result['charm']=struct.unpack('<h',vm.mem_read(0x4b6494,2))[0]
    result['bonus']=[i32(vm,OBJECT+0x7dc+i*4) for i in range(9)] if result['rewardStarted'] else None
    return result

def sequence(seed,frames,start=0,initial=None):
    vm,clock,draws=create_vm(seed,start)
    if initial:
        for name,value in initial.items():
            offset,width=FIELDS[name]
            vm.mem_write(OBJECT+offset,(value&((1<<(width*8))-1)).to_bytes(width,'little'))
    rows=[]
    for frame in frames:
        clock[0]=frame['time']
        action=frame.get('action')
        if action:call(vm,{'left':0x4513f0,'right':0x451430,'releaseLeft':0x451470,'releaseRight':0x451490}[action])
        call(vm,0x44fdc0)
        if vm.mem_read(OBJECT+0x7d4,1)[0] and not call(vm,0x453b00,OBJECT+0x7dc):
            vm.mem_write(OBJECT+5,b'\x01')
        # Drawing itself changes two gameplay flags on loss. Execute only those
        # exact instructions rather than replacing the DDraw renderer.
        if i32(vm,OBJECT+0x444)==1 and not vm.mem_read(OBJECT+0x7c4,1)[0] and not vm.mem_read(OBJECT+0x7b8,1)[0]:
            if (clock[0]-i32(vm,OBJECT+0x1c))&0xffffffff>=500:
                vm.mem_write(OBJECT+0x7c4,b'\x01')
        rows.append(dict(frame,after=snapshot(vm,draws)))
        if rows[-1]['after']['finished']:break
    return {'seed':seed,'start':start,'initial':initial or {},'frames':rows}

if '--tables' in sys.argv:
    print(json.dumps(TABLES,indent=2));sys.exit()

cases=[]
for seed in [1,42,1999,0xffffffff]:
    cases.append(sequence(seed,[{'time':t} for t in range(16,24001,16)]))
    cases.append(sequence(seed,[{'time':t,'action':'left' if i%2==0 else 'right'} for i,t in enumerate(range(16,15001,16))]))
    cases.append(sequence(seed,[{'time':t,'action':'left' if i%2==0 else 'right'} for i,t in enumerate(range(1,15001,149))]))
for time in [999,1000,1001,1490,1491,1499,1500,1998,1999,2000,12500,12501,15500,15501,20500,20501]:
    cases.append(sequence(42,[{'time':time,'action':'left'}],initial={'gauge':995}))
cases.append(sequence(1999,[{'time':t} for t in [1491,1492,1493,1494,1495,1496,1497,1498,1499,1500,1991,1999,2991,2999,3999,4491,4499,5999,7999,9999,11999,13999,15999,17999,19999]]))
out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
     'method':'Original 0x44fdc0, input handlers and FLC timers execute x86; only drawing decoder, sound and bonus constructor are hooked. Heap-backed pre-activation fields begin zero.',
     'tables':TABLES,'cases':cases}
out['fields']=list(cases[0]['frames'][0]['after'])
for case in cases:
    for frame in case['frames']:
        frame['after']=[frame['after'][name] for name in out['fields']]
(ROOT/'tests/fixtures/original-fire.json').write_text(json.dumps(out,separators=(',',':'))+'\n',encoding='utf8')
print(f'Generated {sum(len(case["frames"]) for case in cases)} original Fire x86 frame states')
