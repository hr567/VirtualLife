"""Independent native 004576a0/00457720 palette-cycle evidence for the renderer."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import *
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));STOP=0x30000ff0;STACK=0x1001ff00
def w32(vm,a,v):vm.mem_write(a,struct.pack('<I',v&0xffffffff))
def u32(vm,a):return struct.unpack('<I',vm.mem_read(a,4))[0]
def i32(vm,a):return struct.unpack('<i',vm.mem_read(a,4))[0]
def field(row,name):return row[name].get('signed',row[name].get('bytes'))[0]
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'));objects={row['sha256']:row for row in manifest['objects']}
maps=json.loads((ROOT/'assets/catalog/maps.json').read_text(encoding='utf8'))['maps']

def run(map,start,steps):
    cycles=[{'first':field(row,'FirstColor'),'last':field(row,'LastColor'),'type':field(row,'Type'),'interval':1000//field(row,'Speed')}
            for name,row in map['legacySections'].items()if name.startswith('PalCycling_')]
    resource=map['backgroundTiles'][0]['resourceId'];data=(ROOT/objects[manifest['resources'][resource]['sha256']]['path']).read_bytes();offset=14+struct.unpack_from('<I',data,14)[0]
    palette=bytearray(v for i in range(256)for v in [data[offset+i*4+2],data[offset+i*4+1],data[offset+i*4],0]);initial=bytes(palette)
    vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(0x400000,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x10000);vm.mem_map(0x30000000,0x1000);w32(vm,0x46b110,0x30000020)
    clock=[0];writes=[]
    def ret(args=0,value=0):
        sp=vm.reg_read(UC_X86_REG_ESP);vm.reg_write(UC_X86_REG_EIP,u32(vm,sp));vm.reg_write(UC_X86_REG_ESP,sp+4+args);vm.reg_write(UC_X86_REG_EAX,value)
    def hook(vm,address,size,_):
        sp=vm.reg_read(UC_X86_REG_ESP)
        if address==0x30000020:ret(value=clock[0])
        elif address==0x456ed0:
            first=u32(vm,sp+4);count=u32(vm,sp+8);vm.mem_write(u32(vm,sp+12),bytes(palette[first*4:(first+count)*4]));ret(12)
        elif address==0x456e60:
            index=u32(vm,sp+4);rgb=[u32(vm,sp+n)for n in [8,12,16]];palette[index*4:index*4+3]=bytes(rgb);writes.append([index,*rgb]);ret(16)
    for address in [0x30000020,0x456ed0,0x456e60]:vm.hook_add(UC_HOOK_CODE,hook,begin=address,end=address)
    def call(address,obj,args=()):
        vm.reg_write(UC_X86_REG_ECX,obj);vm.reg_write(UC_X86_REG_ESP,STACK);w32(vm,STACK,STOP)
        for i,value in enumerate(args):w32(vm,STACK+4+i*4,value)
        vm.emu_start(address,STOP,count=1000000)
    expected=bytearray(palette)
    for i,cycle in enumerate(cycles):
        call(0x4576a0,0x20000000+i*0x1000,[0x20008000,cycle['first'],cycle['last']-cycle['first']+1,1,cycle['interval'],cycle['type']])
        cycle.update(lastTick=0,position=cycle['first'],direction=1)
    frames=[];clock[0]=start
    for elapsed in steps:
        clock[0]=(clock[0]+elapsed)&0xffffffff;writes.clear();expected_writes=[]
        for i,cycle in enumerate(cycles):
            call(0x457720,0x20000000+i*0x1000)
            if (clock[0]-cycle['lastTick'])&0xffffffff>cycle['interval']:
                cycle['lastTick']=clock[0]
                if cycle['type']==0:
                    last=bytes(expected[cycle['last']*4:cycle['last']*4+3])
                    for index in range(cycle['last'],cycle['first'],-1):
                        rgb=bytes(expected[(index-1)*4:(index-1)*4+3]);expected[index*4:index*4+3]=rgb;expected_writes.append([index,*rgb])
                    expected[cycle['first']*4:cycle['first']*4+3]=last;expected_writes.append([cycle['first'],*last])
                else:
                    cycle['position']+=cycle['direction'];position=cycle['position']
                    if position in [cycle['first'],cycle['last']]:cycle['direction']*=-1
                    else:
                        source=cycle['first']if cycle['direction']==1 else position;rgb=initial[source*4:source*4+3]
                        expected[position*4:position*4+3]=rgb;expected_writes.append([position,*rgb])
            obj=0x20000000+i*0x1000
            assert [u32(vm,obj+0x20),i32(vm,obj+0x18),i32(vm,obj+0x1c)]==[cycle['lastTick'],cycle['position'],cycle['direction']]
        assert palette==expected and writes==expected_writes,(clock[0],writes,expected_writes)
        frames.append({'time':clock[0],'writes':writes[:],'cycles':[{k:cycle[k]for k in ['lastTick','position','direction']}for cycle in cycles],
                       'colors':[[index,*palette[index*4:index*4+3]]for cycle in cycles for index in range(cycle['first'],cycle['last']+1)]})
    return {'map':map['id'],'start':start,'cycles':[{k:cycle[k]for k in ['first','last','type','interval']}for cycle in cycles],'frames':frames}

if __name__=='__main__':
    cases=[]
    for map in maps:
        if not any(name.startswith('PalCycling_')for name in map['legacySections']):continue
        for start,steps in [(0,[0,1,82,1,416,1,0,83,1]*4),(100000,[16]*240),(0xfffffff0,[16]*240)]:cases.append(run(map,start,steps))
    output={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),'method':'Native cycle constructor and full 00457720 with only palette device and clock hooks; compares all color writes and cycle state against renderer transition model.','cases':cases}
    (ROOT/'tests/fixtures/original-palette-cycles.json').write_text(json.dumps(output,separators=(',',':'))+'\n',encoding='utf8')
    print(f'{len(cases)} original palette scenarios, {sum(len(c["frames"])for c in cases)} frame states verified')
