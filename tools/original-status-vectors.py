"""Execute the original parameter overview's investment-return arithmetic."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_EIP
source=ROOT/'research/original/N2Game.exe';pe=pefile.PE(str(source))
def investment(stage):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(0x400000,(pe.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,pe.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000)
    stack=0x10010000;vm.reg_write(UC_X86_REG_ESP,stack-24)
    vm.mem_write(0x4b9ecd,bytes([stage]))
    vm.emu_start(0x401e66,0x401f48,count=10000)
    assert vm.reg_read(UC_X86_REG_EIP)==0x401f48
    assert vm.reg_read(UC_X86_REG_ESP)==stack
    return {'stage':stage,'value':struct.unpack('<i',vm.mem_read(stack+24,4))[0]}
result={'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),
    'method':'Unchanged x86 span 0x401e66..0x401f48, no hooks or instruction substitutions. All 14 reachable housing investment stages.',
    'investments':[investment(stage) for stage in range(14)]}
(ROOT/'tests/fixtures/original-status.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
print(json.dumps(result['investments']))
