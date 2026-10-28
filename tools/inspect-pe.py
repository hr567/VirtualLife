"""Read-only PE inspection for the original disc executable; never executes it."""
import argparse
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "research/tools/python-packages"))
import capstone
import pefile

parser = argparse.ArgumentParser()
parser.add_argument("--exe", type=Path, default=ROOT / "research/original/N2Game.exe")
parser.add_argument("--address", type=lambda x: int(x, 0))
parser.add_argument("--length", type=lambda x: int(x, 0), default=256)
parser.add_argument("--find-int", type=lambda x: int(x, 0))
parser.add_argument("--text")
parser.add_argument("--bytes", action="store_true")
parser.add_argument("--pointers", type=int, default=0)
args = parser.parse_args()
pe = pefile.PE(str(args.exe))
base = pe.OPTIONAL_HEADER.ImageBase
data = args.exe.read_bytes()
if args.address is not None:
    blob = pe.get_data(args.address - base, args.length)
    if args.pointers:
        for i in range(args.pointers):
            address = struct.unpack_from('<I', blob, i * 4)[0]
            try:
                value = pe.get_string_at_rva(address - base).decode('gbk', errors='replace')
            except Exception:
                value = ''
            print(f'{i}: 0x{address:08x} {value}')
    elif args.bytes:
        print(blob.hex())
    else:
        md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
        for ins in md.disasm(blob, args.address):
            print(f"{ins.address:08x}  {ins.bytes.hex():24s} {ins.mnemonic:8s} {ins.op_str}")
elif args.find_int is not None or args.text:
    needles = [struct.pack("<I", args.find_int)] if args.find_int is not None else [args.text.encode("ascii")]
    for needle in needles:
        pos = data.find(needle)
        while pos >= 0:
            try: print(f"file=0x{pos:x} va=0x{base + pe.get_rva_from_offset(pos):x}")
            except pefile.PEFormatError: print(f"file=0x{pos:x}")
            pos = data.find(needle, pos + 1)
else:
    print(json.dumps({"imageBase": hex(base), "entry": hex(base + pe.OPTIONAL_HEADER.AddressOfEntryPoint),
        "imageBytes": pe.OPTIONAL_HEADER.SizeOfImage,
        "imports": {entry.dll.decode(): [i.name.decode() if i.name else i.ordinal for i in entry.imports]
                    for entry in pe.DIRECTORY_ENTRY_IMPORT}}, indent=2))
