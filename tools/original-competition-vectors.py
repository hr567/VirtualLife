"""Execute original student-contest scoring branches without renderer or OS calls."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_ESI,UC_X86_REG_EIP
source=ROOT/'research/original/N2Game.exe';pe=pefile.PE(str(source));base=pe.OPTIONAL_HEADER.ImageBase
def create():
 vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(base,(pe.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(base,pe.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000);vm.reg_write(UC_X86_REG_ESP,0x1001e000);vm.reg_write(UC_X86_REG_ESI,0);return vm
rows=[]
for stage in range(7):
 thresholds=struct.unpack('<3h',pe.get_data(0x4987cc+(stage-2 if 2<=stage<=4 else 0)*6-base,6))
 for seed in [1,42,1999]:
  for charm in sorted({0,9999,*[v+d for v in thresholds for d in [-1,0,1]]}):
   vm=create();vm.mem_write(0x4b649e,struct.pack('<h',stage));vm.mem_write(0x4b6494,struct.pack('<h',charm));vm.mem_write(0x4a3f84,struct.pack('<I',seed))
   vm.emu_start(0x44a646,0x44a8dc,count=10000);assert vm.reg_read(UC_X86_REG_EIP)==0x44a8dc
   scores=list(struct.unpack('<12h',vm.mem_read(0x4e6f78,24)))
   rows.append({'stage':stage,'seed':seed,'charm':charm,'scores':[scores[n:n+3]for n in range(0,12,3)],'totals':list(struct.unpack('<4h',vm.mem_read(0x4e6f90,8))),
    'place':vm.mem_read(0x4e6ff8,1)[0],'prize':struct.unpack('<h',vm.mem_read(0x4e7048,2))[0],'winners':list(vm.mem_read(0x1001e014,3)),'rngState':struct.unpack('<I',vm.mem_read(0x4a3f84,4))[0]})
result={'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'entry':'0x0044a646..0x0044a8dc','beauty':rows}
(ROOT/'tests/fixtures/original-competition.json').write_text(json.dumps(result,separators=(',',':')),encoding='utf8')
print(json.dumps({'beautyCases':len(rows)}))
