import sys
from pathlib import Path
sys.path.insert(0,str(Path('research/tools/python-packages').resolve()))
import pefile,capstone
pe=pefile.PE('research/original/N2Game.exe');md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32);md.skipdata=True
for section in pe.sections:
 if section.Characteristics&0x20000000:
  for ins in md.disasm(section.get_data(),pe.OPTIONAL_HEADER.ImageBase+section.VirtualAddress):
   if ins.mnemonic in ['fldcw','fninit','finit','fnstcw']:print(hex(ins.address),ins.mnemonic,ins.op_str)
