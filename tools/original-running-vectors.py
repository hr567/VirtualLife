"""Original student running contest, including its real FLC clocks and input."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import *
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));STOP=0x200ffff0
RULES=json.loads((ROOT/'assets/catalog/rules.json').read_text(encoding='utf8'))['competition']
def w32(vm,a,v):vm.mem_write(a,struct.pack('<I',v&0xffffffff))
def i32(vm,a):return struct.unpack('<i',vm.mem_read(a,4))[0]
def i16(vm,a):return struct.unpack('<h',vm.mem_read(a,2))[0]
FIELDS={'contestant':(0x13,1),'phase':(0x14,1),'rank':(0x15,1),'hand':(0x16,1),'podiumPlace':(0x17,1),'interval':(0x18,4),'counter':(0x1c,4),'phaseSince':(0x28,4),'finished':(0x34,4)}

def sequence(seed,sex,stage,stamina,step,mode,max_frames=20000,start=0):
    vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x100000);vm.mem_map(0x30000000,0x1000)
    objects={}
    for index,addr in enumerate(range(0x4e6e44,0x4e7060,4)):
        obj=0x20001000+index*0x1000;objects[addr]=obj;w32(vm,addr,obj);w32(vm,obj,0x20000000)
    w32(vm,0x20000004,0x30000000);w32(vm,0x20000008,0x30000000);w32(vm,0x2000001c,0x30000010)
    w32(vm,0x4b6454,0x200fe000);w32(vm,0x200fe004,0x200fe100);w32(vm,0x200fe008,0x200fe200)
    w32(vm,0x46b110,0x30000020);w32(vm,0x4a3f84,seed)
    for addr,value in [(0x4b6498,sex),(0x4b649e,stage),(0x4b648e,stamina)]:vm.mem_write(addr,struct.pack('<h',value))
    tracks={};running=RULES['running']
    for actor in range(4):
        for phase in range(3):tracks[f'actor{actor}_{phase}']=(objects[0x4e6ec0+(actor*3+phase)*4],running['actors'][sex][actor][phase])
    for name,addr in [('track',0x4e6ef8),('launch',0x4e6ef0),('flight',0x4e6ef4),('handAnimation',0x4e6e68),('host',0x4e700c),('podium',0x4e7050)]:tracks[name]=(objects[addr],running['hand' if name=='handAnimation' else name])
    for i in range(3):tracks[f'podium{i}']=(objects[0x4e7054+i*4],running['podiumPlaces'][i])
    for obj,animation in tracks.values():
        for off,value in [(0xc,animation['x']),(0x10,animation['y']),(0x1c,0),(0x24,animation['frameCount']),(0x28,0),(0x30,1),(0x34,animation['interval'])]:w32(vm,obj+off,value)
    index=[-1];clock=[start];base=[0];draws=[0];rows=[];layers=[];sounds=[]
    def ret(args=0,value=0):
        sp=vm.reg_read(UC_X86_REG_ESP);vm.reg_write(UC_X86_REG_EIP,i32(vm,sp));vm.reg_write(UC_X86_REG_ESP,sp+4+args);vm.reg_write(UC_X86_REG_EAX,value&0xffffffff)
    def snapshot():
        b=base[0];row={name:(vm.mem_read(b+off,1)[0] if size==1 else i32(vm,b+off))for name,(off,size)in FIELDS.items()}
        row.update(times=list(struct.unpack('<4h',vm.mem_read(b+0x2c,8))),opponentIntervals=list(struct.unpack('<3h',vm.mem_read(b+0x3c,6))),
                   tracks={name:[i32(vm,obj+off)for off in [0x1c,0x28,0x30,0x34]]for name,(obj,_)in tracks.items()},
                   layers=layers[:],sounds=sounds[:],rngState=i32(vm,0x4a3f84)&0xffffffff,rngDraws=draws[0])
        return row
    def hook(vm,address,size,_):
        if address==0x449817:
            index[0]+=1;base[0]=vm.reg_read(UC_X86_REG_ESP);clock[0]=start+(index[0]+1)*step;layers.clear();sounds.clear()
        elif address==0x44a0cc:
            rows.append({'time':clock[0],'left':mode=='both' or(mode=='alternate' and index[0]%2==0),'right':mode=='both' or(mode=='alternate' and index[0]%2!=0),'after':snapshot()})
            if len(rows)>=max_frames or rows[-1]['after']['finished']:vm.emu_stop()
        elif address in [0x30000000,0x30000010]:ret(12 if address==0x30000000 else 16)
        elif address==0x30000020:ret(value=clock[0])
        elif address in [0x458ef0,0x45ba70,0x45bae0]:ret(4)
        elif address in [0x456d30,0x456dd0,0x456ed0,0x456eb0]:ret(12)
        elif address in [0x448920,0x456c20,0x4597a0,0x458610,0x45bd90,0x45a3e0,0x44ca10]:ret()
        elif address==0x45aef0:
            sp=vm.reg_read(UC_X86_REG_ESP);vm.mem_write(i32(vm,sp+4),b'\0');ret()
        elif address==0x45a370:
            if index[0]>=0:
                obj=vm.reg_read(UC_X86_REG_ECX)
                for name,addr in [('launch',0x4e6f28),('result',0x4e6f30)]:
                    if obj==objects[addr]:sounds.append(name)
            ret(4)
        elif address in [0x458550,0x458570]:
            obj=vm.reg_read(UC_X86_REG_ECX)
            for name,(pointer,_)in tracks.items():
                if pointer==obj:layers.append([name,i32(vm,obj+0x1c)])
            ret(4 if address==0x458550 else 8)
        elif address==0x449af3:layers.clear() # the first podium frame clears the whole backbuffer
        elif address==0x459b60:
            key=i32(vm,vm.reg_read(UC_X86_REG_ESP)+4)
            ret(4,int(mode=='both' or(mode=='alternate' and index[0]%2==(0 if key==0xcb else 1))))
        elif address==0x45dec1:draws[0]+=1
    vm.hook_add(UC_HOOK_CODE,hook);w32(vm,0x1001ff00,STOP);vm.reg_write(UC_X86_REG_ESP,0x1001ff00)
    vm.emu_start(0x4495a0,STOP,count=100000000)
    assert rows and(len(rows)==max_frames or rows[-1]['after']['finished']),(len(rows),hex(vm.reg_read(UC_X86_REG_EIP)))
    return {'seed':seed,'sex':sex,'stage':stage,'stamina':stamina,'step':step,'mode':mode,'start':start,'frames':rows}

if __name__=='__main__':
    cases=[]
    for sex in range(2):
        for step in [16,71,150]:
            cases.append(sequence(42,sex,3,300,step,'alternate'))
    cases.append(sequence(1999,0,2,0,100,'none'))
    cases.append(sequence(1999,1,4,1000,16,'both'))
    cases.append(sequence(1,1,3,400,71,'alternate',start=30000))
    for stage in [1,2,3,4,5]:
        row=stage-2 if 2<=stage<=4 else 0
        for threshold in RULES['staminaThresholds'][row]:
            for stamina in [threshold-1,threshold,threshold+1]:cases.append(sequence(42,0,stage,stamina,16,'none',max_frames=2))
    out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),'method':'Complete original 0x4495a0 and FLC timer/reset code; only file/UI/input/clock/audio hooks.','cases':cases}
    out['fields']=list(cases[0]['frames'][0]['after'])
    for case in cases:
        for frame in case['frames']:frame['after']=[frame['after'][name]for name in out['fields']]
    (ROOT/'tests/fixtures/original-running.json').write_text(json.dumps(out,separators=(',',':'))+'\n',encoding='utf8')
    print(f'{len(cases)} original running cases, {sum(len(c["frames"])for c in cases)} frames')
