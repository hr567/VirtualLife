"""Original N1 graphics, map notices, payroll and prologue presentation data.

The wave table is produced by executing 00425020, including its x87/CRT
conversion, rather than reproducing its trigonometry in the client.
"""
import hashlib, json, struct, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_EAX, UC_X86_REG_ECX
source = ROOT/'research/original/N2Game.exe'
pe = pefile.PE(str(source)); base = pe.OPTIONAL_HEADER.ImageBase
def number(va, fmt='<i'): return struct.unpack(fmt, pe.get_data(va-base, struct.calcsize(fmt)))[0]
def text(va): return pe.get_string_at_rva(va-base).decode('gbk')
def pointer(va): return text(number(va,'<I'))
vm = Uc(UC_ARCH_X86, UC_MODE_32)
vm.mem_map(base, (pe.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
vm.mem_write(base, pe.get_memory_mapped_image())
vm.mem_map(0x10000000, 0x10000)
vm.reg_write(UC_X86_REG_ESP, 0x1000fff0)
vm.mem_write(0x1000fff0, struct.pack('<II',0x10000000,10)) # 0040c2fb pushes amplitude 10
vm.emu_start(0x425020,0x10000000,count=100000)
wave = list(struct.unpack('<67i',vm.mem_read(0x4b667c,67*4)))
manifest = json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects = {o['sha256']:o for o in manifest['objects']}
def media(resource): return objects[manifest['resources'][resource]['sha256']]['media']
def cstring(address):
    value=bytearray()
    while len(value)<1024:
        byte=vm.mem_read(address+len(value),1)[0]
        if not byte: return bytes(value)
        value.append(byte)
    raise ValueError('Unterminated original text')
drawn=[]
def hook(uc,address,size,user):
    if address not in [0x45e47b,0x45db32,0x40e580]:return
    stack=uc.reg_read(UC_X86_REG_ESP)
    args=struct.unpack('<7I',uc.mem_read(stack,28))
    if address==0x45e47b:
        value=struct.unpack('<i',struct.pack('<I',args[1]))[0]
        uc.mem_write(args[2],str(value).encode()+b'\0')
    elif address==0x45db32:
        value=cstring(args[2]);assert b'%' not in value,value
        uc.mem_write(args[1],value+b'\0')
    else:
        drawn.append({'x':args[1],'y':args[2],'text':cstring(args[5]).decode('gbk')})
    uc.reg_write(UC_X86_REG_ESP,stack+4);uc.reg_write(UC_X86_REG_EIP,args[0])
handle=vm.hook_add(UC_HOOK_CODE,hook)
rank_panels=[]
vm.mem_write(0x4b6478,b'__NAME__\0')
for career in range(24):
    ranks=[]
    for rank in range(1,6):
        modes=[]
        for mode in [1,2]:
            drawn.clear()
            for va,value in [(0x4b64a2,career),(0x4b64a6,rank)]:vm.mem_write(va,struct.pack('<h',value))
            vm.reg_write(UC_X86_REG_ESP,0x1000fff0)
            vm.mem_write(0x1000fff0,struct.pack('<4I',0x10000000,mode,0,0))
            vm.emu_start(0x424210,0x10000000,count=100000)
            assert vm.reg_read(UC_X86_REG_EIP)==0x10000000
            modes.append(drawn[:])
        ranks.append(modes)
    rank_panels.append(ranks)
vm.hook_del(handle)
# Observe the complete original demotion bounce (00410350). Only window,
# drawing/allocation and Sleep calls are replaced; the four bounce trajectories
# and animation frame choices execute as original x86.
vm.mem_map(0,0x1000);vm.mem_map(0x20000000,0x20000)
def w32(address,value):vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def i32(address):return struct.unpack('<i',vm.mem_read(address,4))[0]
def ret(pop=0,value=0):
    stack=vm.reg_read(UC_X86_REG_ESP);destination=i32(stack)&0xffffffff
    vm.reg_write(UC_X86_REG_ESP,stack+4+pop);vm.reg_write(UC_X86_REG_EAX,value);vm.reg_write(UC_X86_REG_EIP,destination)
for obj in [0x20001000,0x20011000,0x20012000]:
    w32(obj,0x20000000);w32(obj+4,640);w32(obj+8,480)
w32(0x20000004,0x20000100);w32(0x20000018,0x20000110)
w32(0x4b6454,0x20010000);w32(0x20010004,0x20011000);w32(0x20010008,0x20012000)
actor=0x20013000
for offset,value in [(0xc,161),(0x10,214),(0x24,media('install/ani/pm0kc03.flc')['frames'])]:w32(actor+offset,value)
bounce=[];drawing={'offsetY':0,'sprite':None,'delayMs':0}
for entry in pe.DIRECTORY_ENTRY_IMPORT:
    for imp in entry.imports:
        if imp.name==b'Sleep':w32(imp.address,0x20000120)
def bounce_hook(uc,address,size,user):
    stack=uc.reg_read(UC_X86_REG_ESP)
    if address==0x45d746:ret(value=0x20001000)
    elif address==0x458800:ret(16,0x20001000)
    elif address==0x20000100:ret(12)
    elif address==0x20000110:
        drawing['offsetY']=i32(stack+8);ret(16)
    elif address==0x20000120:
        if bounce:bounce[-1]['delayMs']=i32(stack+4)
        ret(4)
    elif address==0x458ef0:drawing['sprite']=None;ret(4)
    elif address==0x410030:ret()
    elif address==0x458860 or address==0x45d754:ret()
    elif address==0x4585a0:w32(uc.reg_read(UC_X86_REG_ECX)+0x1c,i32(stack+4));ret(4)
    elif address==0x4585c0:ret()
    elif address==0x458570:
        obj=uc.reg_read(UC_X86_REG_ECX)
        drawing['sprite']={'frame':i32(obj+0x1c),'x':i32(obj+0xc),'y':i32(obj+0x10)};ret(8)
    elif address in [0x456c20,0x456c50]:bounce.append(dict(drawing));ret()
handle=vm.hook_add(UC_HOOK_CODE,bounce_hook)
vm.reg_write(UC_X86_REG_ESP,0x1000ff00)
vm.mem_write(0x1000ff00,struct.pack('<5I',0x10000000,4,10,25,actor))
vm.emu_start(0x410350,0x10000000,count=1000000)
assert vm.reg_read(UC_X86_REG_EIP)==0x10000000
assert bounce and any(frame['offsetY']<0 for frame in bounce)
vm.hook_del(handle)
result = {
 'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),
 'evidence':['0x0040a190','0x0040a610','0x0040abe0','0x0040b1a0','0x0040b360','0x0041ba90','0x0041bdb0','0x00423780','0x00423b70','0x00424210','0x00451e40','0x00452260'],
 'background':{'wave':wave,'waveFactor':number(0x46b240,'<d')*number(0x46b23c,'<f'),'waveOrigin':number(0x46b238,'<f'),'patternIntervalMs':number(0x473918)},
 'birthdaySound':'install/'+text(0x479a20).replace('\\','/').lower(),
 'lifeNotices':['install/bmp/end10.bmp','install/bmp/end03.bmp'],
 'payroll':{
   'panels':['install/bmp/pm0ka01.bmp','install/bmp/pm0kb01.bmp','install/bmp/pm0kc01.bmp'],
   'animations':['install/ani/pm0ka02.flc','install/ani/pm0kb02.flc','install/ani/pm0kc03.flc'],
   'animationPositions':[[135,0],[288,129],[161,214]],
   'counter':'install/'+text(0x47a61c).replace('\\','/').lower(),
   'counterPositions':[[144,222],[124,289],[123,292]],
   'demotionImage':'install/'+text(0x47a60c).replace('\\','/').lower(),
   'masteredAnimation':'install/'+text(0x47a654).replace('\\','/').lower(),
   'rankNames':[[pointer(0x476f98+(career*5+rank)*4) for rank in range(5)] for career in range(24)]+[['勇士']*5],
   'rankPanels':rank_panels,
   'demotionBounce':bounce,
 },
 'prologue':{
   'opening':'derived/video/n2open.webm','openingAudio':'derived/video/n2open.wav',
   'movie':'derived/proepi/aopen01.webm','narration':'cd/data/proepi/enter.wav',
   'subtitles':[{'text':pointer(0x4a2978+i*12) if number(0x4a2978+i*12,'<I') else '', 'segment':number(0x4a297c+i*12), 'durationMs':number(0x4a2980+i*12,'<I')} for i in range(17)],
 }
}
for key in ['opening','movie']:
    result['prologue'][key+'Metadata']=media(result['prologue'][key])
destination=ROOT/'assets/catalog/native-events.json'
destination.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(f'Exported {destination}: {len(wave)} original wave values and 17 prologue captions')
