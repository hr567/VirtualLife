"""Execute unmodified 00452260 for the real defeat/victory outro branches.

Only input polling, graphics and the DirectShow device are deterministic mocks.
The original victory byte chooses AOPEN03/native phase 2 or AOPEN04/phase 3.
The device reports the corresponding verified manifest clip duration; this is
not a measurement of DirectShow scheduling or a Windows GUI recording.
"""
import hashlib
import json
import struct
import sys
from fractions import Fraction
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "research/tools/python-packages"))
import pefile
from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import (
    UC_X86_REG_ESP, UC_X86_REG_ECX, UC_X86_REG_EDX, UC_X86_REG_EIP,
    UC_X86_REG_EAX,
)

SOURCE = ROOT / "research/original/N2Game.exe"
SOURCE_SHA256 = "e725c9c7dd9de766cd1bf66dba18260858e447980c65efbd292a9cd3687ef4c0"
assert hashlib.sha256(SOURCE.read_bytes()).hexdigest() == SOURCE_SHA256
PE = pefile.PE(str(SOURCE))
BASE = PE.OPTIONAL_HEADER.ImageBase
MANIFEST = json.loads((ROOT / "assets/manifest.json").read_text(encoding="utf-8"))
OBJECTS = {entry["sha256"]: entry for entry in MANIFEST["objects"]}
OBJ, COM, VTABLE = 0x20000000, 0x20002000, 0x20003000
STACK, STOP = 0x1001F000, 0x30000FF0


def w32(vm, address, value):
    vm.mem_write(address, struct.pack("<I", value & 0xFFFFFFFF))


def r32(vm, address):
    return struct.unpack("<I", vm.mem_read(address, 4))[0]


def ret(vm, args=0, value=0):
    stack = vm.reg_read(UC_X86_REG_ESP)
    target = r32(vm, stack)
    vm.reg_write(UC_X86_REG_EAX, value & 0xFFFFFFFF)
    vm.reg_write(UC_X86_REG_ESP, stack + 4 + args)
    vm.reg_write(UC_X86_REG_EIP, target)


def call(vm):
    vm.reg_write(UC_X86_REG_ESP, STACK)
    vm.reg_write(UC_X86_REG_ECX, OBJ)
    w32(vm, STACK, STOP)
    vm.emu_start(0x00452260, STOP, count=100000)
    assert vm.reg_read(UC_X86_REG_EIP) == STOP
    assert vm.reg_read(UC_X86_REG_ESP) == STACK + 4
    return bool(vm.reg_read(UC_X86_REG_EAX) & 255)


def verified_clip(resource):
    record = MANIFEST["resources"][resource]
    obj = OBJECTS[record["sha256"]]
    assert hashlib.sha256((ROOT / obj["path"]).read_bytes()).hexdigest() == record["sha256"]
    return record, obj


cases = []
for victory in (False, True):
    suffix = "04" if victory else "03"
    native_phase = 3 if victory else 2
    source_id = f"cd/data/proepi/aopen{suffix}.avi"
    derived_id = f"derived/proepi/aopen{suffix}.webm"
    source_record, _ = verified_clip(source_id)
    derived_record, derived_object = verified_clip(derived_id)
    media = derived_object["media"]
    assert derived_record["derivedFrom"] == source_id
    assert derived_object["derivedFrom"] == source_record["sha256"]
    assert media["losslessEncoding"] and media["decodedFramesVerified"]
    duration = media["durationMs"]
    assert duration == round(Fraction(media["frames"]) * 1000 / Fraction(media["frameRate"]))
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0, 0x1000)
    vm.mem_map(BASE, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
    vm.mem_write(BASE, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    vm.mem_map(0x20000000, 0x10000)
    vm.mem_map(0x30000000, 0x1000)
    vm.mem_write(OBJ + 4, bytes([int(victory)]))
    w32(vm, OBJ + 0x110, 1)
    w32(vm, OBJ + 0x10C, 0x20005000)
    w32(vm, OBJ + 0x244, 0x4A2A48 if victory else 0x4A2AF0)
    w32(vm, OBJ + 0x24C, 14 if victory else 9)
    w32(vm, OBJ + 0x250, 14 if victory else 9)
    clock, video_started, opened = [0], [0], []
    imports = {}
    for dll in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in dll.imports:
            if imp.name == b"GetTickCount":
                imports[0x30000200] = imp.name
                w32(vm, imp.address, 0x30000200)

    def hook(uc, address, _size, _data):
        stack = uc.reg_read(UC_X86_REG_ESP)
        if address in imports:
            ret(uc, value=clock[0])
        elif address == 0x4597A0:
            ret(uc)
        elif address == 0x459BA0:
            ret(uc, 4, 0)
        elif address == 0x451B50:
            phase = r32(uc, OBJ + 0x110)
            ret(uc, value=int(phase == 1 or clock[0] - video_started[0] < duration))
        elif address == 0x451CB0:
            uc.mem_write(r32(uc, stack + 4), struct.pack("<q", (clock[0] - video_started[0]) * 10000))
            ret(uc, 4, 1)
        elif address in (0x451750, 0x45D754):
            ret(uc)
        elif address == 0x45D746:
            ret(uc, value=0x20005000)
        elif address == 0x451730:
            ret(uc, value=uc.reg_read(UC_X86_REG_ECX))
        elif address == 0x4517A0:
            candidates = [uc.reg_read(UC_X86_REG_EDX)] + [r32(uc, stack + index * 4) for index in range(1, 6)]
            for pointer in candidates:
                try:
                    value = bytes(uc.mem_read(pointer, 260)).split(b"\0", 1)[0].decode("ascii")
                except (UcError, UnicodeDecodeError):
                    continue
                if "aopen" in value.lower():
                    opened.append(value)
            video_started[0] = clock[0]
            ret(uc, 20, 1)
        else:
            raise AssertionError(hex(address))

    for address in [*imports, 0x4597A0, 0x459BA0, 0x451B50, 0x451CB0, 0x451750, 0x45D754, 0x45D746, 0x451730, 0x4517A0]:
        vm.hook_add(UC_HOOK_CODE, hook, begin=address, end=address)
    alive = call(vm)
    assert alive and r32(vm, OBJ + 0x110) == native_phase
    assert opened == [f"ProEpi\\aopen{suffix}.avi"], opened
    frames = []
    for elapsed in (duration - 1, 1):
        clock[0] += elapsed
        alive = call(vm)
        frames.append({"elapsedMs": elapsed, "after": {
            "alive": alive, "nativePhase": r32(vm, OBJ + 0x110),
            "videoTimeMs": clock[0] - video_started[0],
            "phase": "outro" if alive else "finished",
        }})
    assert [frame["after"]["alive"] for frame in frames] == [True, False]
    cases.append({"victory": victory, "victoryByte": int(victory), "nativePhase": native_phase,
                  "openedClip": opened[0], "sourceResource": source_id,
                  "sourceClipSha256": source_record["sha256"], "derivedResource": derived_id,
                  "derivedClipSha256": derived_record["sha256"], "durationMs": duration,
                  "decodedFrames": media["frames"], "frameRate": media["frameRate"], "frames": frames})

output = {"sourceSha256": SOURCE_SHA256, "entryPoint": "0x00452260", "method": __doc__, "cases": cases}
destination = ROOT / "tests/fixtures/original-epilogue-outros.json"
destination.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"fixture": str(destination.relative_to(ROOT)), "cases": len(cases),
                  "boundaryFrames": sum(len(case["frames"]) for case in cases),
                  "durationsMs": [case["durationMs"] for case in cases]}))
