"""Execute map decorative-delay draws followed by original god placement."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_EBP,UC_X86_REG_EBX,UC_X86_REG_EDI,UC_X86_REG_EIP,UC_X86_REG_EAX
source=ROOT/'research/original/N2Game.exe';pe=pefile.PE(str(source));base=pe.OPTIONAL_HEADER.ImageBase
def w32(vm,address,value):vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def i32(vm,address):return struct.unpack('<i',vm.mem_read(address,4))[0]
rows=[]
for map_data in json.loads((ROOT/'assets/catalog/maps.json').read_text(encoding='utf8'))['maps']:
 for seed in [1,2,42,777,2026,32767]:
  vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0,0x1000);vm.mem_map(base,(pe.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(base,pe.get_memory_mapped_image())
  vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x10000);w32(vm,0x4a3f84,seed);draws=[0]
  def hook(uc,address,size,_):
   if address==0x45dec1:draws[0]+=1
   elif address==0x41e880:
    stack=uc.reg_read(UC_X86_REG_ESP);target=i32(uc,stack);uc.reg_write(UC_X86_REG_EAX,0);uc.reg_write(UC_X86_REG_ESP,stack+12);uc.reg_write(UC_X86_REG_EIP,target)
  vm.hook_add(UC_HOOK_CODE,hook);delays=[];stack=0x1001f000
  for index in range(map_data['legacySections']['General']['SpiritCount']['signed'][0]):
   vm.reg_write(UC_X86_REG_ESP,stack);vm.reg_write(UC_X86_REG_EBP,0x20001000);vm.reg_write(UC_X86_REG_EBX,0x20002000);vm.reg_write(UC_X86_REG_EDI,0)
   w32(vm,stack+0x20,index);w32(vm,stack+0x128,index+1)
   vm.emu_start(0x41781d,0x417862,count=10000);assert vm.reg_read(UC_X86_REG_EIP)==0x417862
   delays.append(i32(vm,0x2000102f))
  w32(vm,0x4b60e0,0x20000000);w32(vm,0x20000018,map_data['start']['row']);w32(vm,0x2000001c,map_data['start']['column'])
  pool=map_data['coordinateIndex'];w32(vm,0x4e7878,len(pool))
  for index,point in enumerate(pool):w32(vm,0x4e70a0+index*8,point['row']);w32(vm,0x4e70a4+index*8,point['column'])
  vm.reg_write(UC_X86_REG_ESP,stack);w32(vm,stack,0x2000fff0);vm.emu_start(0x41e130,0x41e1e9,count=200000)
  assert vm.reg_read(UC_X86_REG_EIP)==0x41e1e9
  rows.append({'map':map_data['id'],'seed':seed,'delaysMs':delays,'positions':[{'row':i32(vm,0x4b653c+i*4),'column':i32(vm,0x4b6554+i*4)}for i in range(5)],'rngDraws':draws[0],'rngState':i32(vm,0x4a3f84)&0xffffffff})
result={'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'method':'Original 0x41781d-0x417862 random-delay instructions execute once per Spirit_n, then original 0x41e130 god placement runs with only sprite positioning stubbed. Ordering is from 0x414870/0x417d60.','cases':rows}
(ROOT/'tests/fixtures/original-map-load.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(rows)} map load sequences')
