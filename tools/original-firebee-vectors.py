"""Execute the complete Firebee x86 frame loop with hardware drawing/input hooks."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import *
SOURCE=ROOT/'research/original/N2Game.exe'
PE=pefile.PE(str(SOURCE))
STOP=0x200ffff0
def w32(vm,a,v):vm.mem_write(a,struct.pack('<I',v&0xffffffff))
def i32(vm,a):return struct.unpack('<i',vm.mem_read(a,4))[0]
def i16(vm,a):return struct.unpack('<h',vm.mem_read(a,2))[0]
def byte(vm,a):return vm.mem_read(a,1)[0]
TABLES={'x':list(struct.unpack('<7h',PE.get_data(0x97ff8,14))), 'intervals':list(struct.unpack('<4h',PE.get_data(0x98008,8)))}
FIELDS={'wave':(0x13,1),'direction':(0x1b,1),'playerState':(0x23,1),'playerX':(0x28,4),
        'failed':(0x44,4),'shotAt':(0x48,4),'finished':(0x54,4),'descentAt':(0x58,4)}

def sequence(seed,frames,initial=None):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x100000);vm.mem_map(0x30000000,0x1000)
    objects={}
    for index,addr in enumerate(range(0x4e6bf0,0x4e6e10,4)):
        obj=0x20001000+index*0x1000;objects[addr]=obj;w32(vm,addr,obj);w32(vm,obj,0x20000000)
    w32(vm,0x20000008,0x30000000);w32(vm,0x20000004,0x30000000)
    w32(vm,0x4b6454,0x200fe000);w32(vm,0x200fe004,0x200fe100);w32(vm,0x200fe008,0x200fe200)
    w32(vm,0x46b110,0x30000020);w32(vm,0x4a3f84,seed)
    for addr in list(range(0x4e6bf8,0x4e6c38,4))+[0x4e6c78]+list(range(0x4e6c80,0x4e6e10,4)):
        obj=objects[addr];count,period=(10,114) if addr>=0x4e6c80 else ((20,71) if addr==0x4e6c78 else (24,71))
        for off,value in [(0x1c,0),(0x24,count),(0x28,0),(0x30,1),(0x34,period)]:w32(vm,obj+off,value)
    # Score is a global short; do not retain the dummy object pointer there.
    w32(vm,0x4e6c64,0)
    index=[-1];base=[0];draws=[0];result=[];drawing={'drawnEnemies':[],'drawnEnemyShots':[],'drawnPlayerShots':[],'drawnExplosions':[],'sounds':[]}
    def ret(args=0,value=0):
        sp=vm.reg_read(UC_X86_REG_ESP);vm.reg_write(UC_X86_REG_EIP,i32(vm,sp));vm.reg_write(UC_X86_REG_ESP,sp+4+args);vm.reg_write(UC_X86_REG_EAX,value&0xffffffff)
    def track(obj):return [i32(vm,obj+off) for off in [0xc,0x10,0x1c,0x28,0x30]]
    def snapshot():
        b=base[0];row={name:({1:byte,2:i16,4:i32}[width])(vm,b+off) for name,(off,width) in FIELDS.items()}
        row.update(score=i16(vm,0x4e6c64),rowY=list(struct.unpack('<16h',vm.mem_read(b+0x70,32))),
                   health=list(vm.mem_read(b+0x90,112)),
                   enemyShots=[list(struct.unpack('<2h',vm.mem_read(b+0x290+4*i,4))) for i in range(i16(vm,b+0x30))],
                   playerShots=[list(struct.unpack('<2h',vm.mem_read(b+0x100+4*i,4))) for i in range(vm.reg_read(UC_X86_REG_EBP)&65535)],
                   enemies=[track(objects[0x4e6bf8+4*i]) for i in range(16)],
                   explosions=[track(objects[0x4e6c80+4*i]) for i in range(i16(vm,b+0x24))],
                   death=track(objects[0x4e6c78]),rngState=i32(vm,0x4a3f84)&0xffffffff,rngDraws=draws[0])
        row.update({key:value[:] for key,value in drawing.items()})
        return row
    def hook(vm,address,size,_):
        if address==0x445f9f:
            index[0]+=1;base[0]=vm.reg_read(UC_X86_REG_ESP)
            for value in drawing.values():value.clear()
            if index[0]==0 and initial:
                for name,value in initial.items():
                    if name in FIELDS:
                        off,width=FIELDS[name];vm.mem_write(base[0]+off,(value&((1<<(8*width))-1)).to_bytes(width,'little'))
                    elif name=='rowY':vm.mem_write(base[0]+0x70,struct.pack('<16h',*value))
                    elif name=='health':vm.mem_write(base[0]+0x90,bytes(value))
                    elif name=='score':vm.mem_write(0x4e6c64,struct.pack('<h',value))
                    elif name in ['playerShots','enemyShots']:
                        offset=0x100 if name=='playerShots' else 0x290
                        for i,point in enumerate(value):vm.mem_write(base[0]+offset+4*i,struct.pack('<2h',*point))
                        if name=='playerShots':vm.reg_write(UC_X86_REG_EBP,len(value));w32(vm,base[0]+0x34,len(value))
                        else:w32(vm,base[0]+0x30,len(value))
        elif address==0x446c4f:
            result.append(snapshot())
            if index[0]+1==len(frames) or result[-1]['finished']:vm.emu_stop()
        elif address==0x30000000:
            obj=vm.reg_read(UC_X86_REG_ECX);sp=vm.reg_read(UC_X86_REG_ESP)
            for field,addr in [('drawnEnemyShots',0x4e6c7c),('drawnPlayerShots',0x4e6bf0)]:
                if obj==objects[addr]:drawing[field].append([i32(vm,sp+4),i32(vm,sp+8)])
            ret(12)
        elif address==0x30000020:ret(value=frames[index[0]]['time'] if index[0]>=0 else 0)
        elif address in [0x458ef0,0x456cd0]:ret(4)
        elif address==0x45a370:
            if index[0]>=0:
                obj=vm.reg_read(UC_X86_REG_ECX)
                for i,number in enumerate([1,2,3,4,5,6,7,9,10,11]):
                    if obj==objects[0x4e6c3c+i*4]:drawing['sounds'].append(number)
            ret(4)
        elif address==0x458550:
            obj=vm.reg_read(UC_X86_REG_ECX)
            if obj in [objects[addr] for addr in range(0x4e6c80,0x4e6e10,4)]:drawing['drawnExplosions'].append(track(obj))
            ret(4)
        elif address==0x458570:
            obj=vm.reg_read(UC_X86_REG_ECX)
            for i in range(16):
                if obj==objects[0x4e6bf8+i*4]:drawing['drawnEnemies'].append([i,i32(vm,obj+0xc),i32(vm,obj+0x10),i32(vm,obj+0x1c)])
            ret(8)
        elif address==0x456ed0:ret(12)
        elif address in [0x4458c0,0x445700,0x456c20,0x4597a0,0x458610,0x45ab10]:ret()
        elif address in [0x4599a0,0x459b60]:
            key=i32(vm,vm.reg_read(UC_X86_REG_ESP)+4);field={0xcb:'left',0xcd:'right',0x39:'fire'}.get(key,'none')
            ret(4,int(frames[index[0]].get(field,False)))
        elif address==0x45dec1:draws[0]+=1
    vm.hook_add(UC_HOOK_CODE,hook);w32(vm,0x1001ff00,STOP);vm.reg_write(UC_X86_REG_ESP,0x1001ff00)
    vm.emu_start(0x445df0,STOP,count=20000000)
    assert result and (len(result)==len(frames) or result[-1]['finished']), (len(result),hex(vm.reg_read(UC_X86_REG_EIP)))
    return {'seed':seed,'initial':initial or {},'frames':[dict(frame,after=after) for frame,after in zip(frames,result)]}

def reward(score,money):
    vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000)
    vm.reg_write(UC_X86_REG_ESP,0x1001ff00);vm.mem_write(0x4e6c64,struct.pack('<h',score));w32(vm,0x4b648a,money)
    vm.emu_start(0x446c84,0x446caf,count=100)
    return {'score':score,'before':money,'after':i32(vm,0x4b648a),'animationDelta':((score*11+32768)%65536)-32768}

if __name__=='__main__':
    cases=[]
    for seed in [1,42,1999,0xffffffff]:
        cases.append(sequence(seed,[{'time':t,'left':i%360<90,'right':180<=i%360<270,'fire':i%2==0} for i,t in enumerate(range(16,30001,16))]))
    health=[(row+1 if row<14 else (8 if row==14 else 100)) if col<max(7-row//2,1) else 0 for row in range(16) for col in range(7)]
    # Isolated collision and removal boundaries, including multiple projectiles.
    for x in [112,113,114,153,154,155,174,175,305,344,345]:
        for y in [238,239,267,268]:
            cases.append(sequence(42,[{'time':16},{'time':32}],{'playerShots':[[x,y]]}))
    for x in [304,305,306,344,345,346]:
        cases.append(sequence(42,[{'time':16},{'time':32}],{'enemyShots':[[x,348],[400,354],[100,350]],'playerShots':[[x+9,354],[400,354]]}))
    for wave in [7,10,14,15,16]:
        ys=[400-i*56 for i in range(16)];ys[wave-1]=30;ys[wave-2]=60
        cases.append(sequence(1999,[{'time':t,'fire':True} for t in range(16,5001,16)],{'wave':wave,'rowY':ys,'health':health}))
    for x in [103,104,105,453,454,455,456]:
        for left,right in [(1,0),(0,1),(1,1)]:cases.append(sequence(1,[{'time':16,'left':left,'right':right}],{'playerX':x}))
    cases.append(sequence(42,[{'time':t} for t in range(16,2001,16)],{'enemyShots':[[315,348]]}))
    # Each enemy row: strict collision edges, final HP, and the boss's first-hit
    # end flag at 0x446a1d (independent of its remaining 100 HP).
    for row in range(16):
        wave=max(6,min(16,row+1));ys=[-1000-i*56 for i in range(16)];ys[row]=120
        x=TABLES['x'][min(row//2,6)] if row<15 else 252
        for dx in ([10,11,51,52] if row<15 else [30,31,89,90]):
            for hp in [1,health[row*7]]:
                states=health[:];states[row*7]=hp
                cases.append(sequence(42,[{'time':16},{'time':32}],{'wave':wave,'rowY':ys,'health':states,'playerShots':[[x+dx,121],[x+dx,149]]}))
    # Empty leading rows advance only at their floor crossing; clocks use >=.
    for wave in [6,7,9,12,15,16]:
        for y in [318,319,320]:
            ys=[-1000-i*56 for i in range(16)];ys[wave-6]=y
            cases.append(sequence(1,[{'time':t} for t in [79,80,99,100,149,150,299,300,301]],{'wave':wave,'rowY':ys,'health':[0]*112}))
    out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),'method':'Original Firebee 0x445df0 complete frame loop and FLC timer code. Only clock/input/drawing/audio/loading hooks. Fresh-stack unused bytes are zero.','tables':TABLES,'cases':cases}
    out['rewards']=[reward(score,money) for score in [0,1,2978,2979,3230,3390,-1] for money in [0,1000,99999998,99999999]]
    out['fields']=list(cases[0]['frames'][0]['after'])
    for case in cases:
        for frame in case['frames']:frame['after']=[frame['after'][name] for name in out['fields']]
    (ROOT/'tests/fixtures/original-firebee.json').write_text(json.dumps(out,separators=(',',':'))+'\n',encoding='utf8')
    print(f'Generated {len(cases)} cases, {sum(len(c["frames"]) for c in cases)} frames. Tables: {TABLES}')
