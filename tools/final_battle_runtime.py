"""Capture original 0x43fd10 constructor arguments with graphics/filesystem stubs."""
import struct
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP

def capture(pe, resource):
    base = pe.OPTIONAL_HEADER.ImageBase
    captures = []
    for sex in range(2):
        vm = Uc(UC_ARCH_X86, UC_MODE_32)
        vm.mem_map(0, 4096)
        vm.mem_map(base, (pe.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
        vm.mem_write(base, pe.get_memory_mapped_image())
        vm.mem_map(0x10000000, 0x20000)
        vm.mem_map(0x20000000, 0x400000)
        vm.mem_map(0x30000000, 4096)
        def u32(a): return struct.unpack('<I', vm.mem_read(a, 4))[0]
        def i32(a): return struct.unpack('<i', vm.mem_read(a, 4))[0]
        def put(a, n): vm.mem_write(a, struct.pack('<I', n & 0xffffffff))
        def text(a): return bytes(vm.mem_read(a, 260)).split(b'\0',1)[0].decode('gbk')
        def ret(pop=0, value=0):
            sp = vm.reg_read(UC_X86_REG_ESP)
            vm.reg_write(UC_X86_REG_EAX, value)
            vm.reg_write(UC_X86_REG_EIP, u32(sp))
            vm.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
        allocated = [0x20001000]
        records = {}
        def hook(_vm, address, _size, _data):
            sp = vm.reg_read(UC_X86_REG_ESP); this = vm.reg_read(UC_X86_REG_ECX)
            if address == 0x45d746:
                result = allocated[0]; allocated[0] += (u32(sp+4)+255)&~255; ret(value=result)
            elif address == 0x45aef0:
                # Mode 0 is the Data root; mode 1 is the installed runtime root.
                vm.mem_write(u32(sp+4), b'cd/data/\0' if u32(sp+8)==0 else b'install/\0'); ret(value=1)
            elif address == 0x445630: ret()
            elif address == 0x4582d0:
                records[this] = {**resource(text(u32(sp+16))), 'x':i32(sp+8), 'y':i32(sp+12), 'objectType':'animation', 'constructorMode':i32(sp+20)}
                put(this+8,this); put(this+0x34,records[this]['frameDurationMs']); ret(20,this)
            elif address in [0x458830,0x45a7d0]:
                records[this] = {**resource(text(u32(sp+8))), 'x':0, 'y':0, 'objectType':'bitmap' if address==0x458830 else 'imageStrip'}
                put(this+0x4c,this); ret(12,this)
            elif address == 0x459eb0:
                records[this] = {**resource(text(u32(sp+20))), 'x':i32(sp+8), 'y':i32(sp+12), 'objectType':'button', 'transparentIndex':i32(sp+16)}
                ret(20,this)
            elif address == 0x45a300: ret(value=this)
            elif address == 0x45a330:
                records[this] = {**resource(text(u32(sp+8))), 'x':0, 'y':0, 'objectType':'sound'}; ret(8,1)
            elif address == 0x4585a0: ret(4)
            elif address == 0x458d90: ret(8,254)  # sentinel consumed only by the following color-key setter
            elif address in [0x458760,0x458b20]:
                key = u32(sp+4)
                if key==254: records[this]['firstPixelTransparent']=True
                else: records[this]['transparentIndex']=key
                ret(4)
            else: raise AssertionError(hex(address))
        calls = [0x45d746,0x45aef0,0x445630,0x4582d0,0x458830,0x45a7d0,0x459eb0,0x45a300,0x45a330,0x4585a0,0x458d90,0x458760,0x458b20]
        for address in calls: vm.hook_add(UC_HOOK_CODE,hook,begin=address,end=address)
        stack = 0x1001e000; put(stack,0x30000000)
        vm.mem_write(0x4b6498,struct.pack('<h',sex)); vm.reg_write(UC_X86_REG_ESP,stack)
        try: vm.emu_start(0x43fd10,0x30000000,count=2000000)
        except Exception:
            print('Final constructor capture stopped at',hex(vm.reg_read(UC_X86_REG_EIP))); raise
        if vm.reg_read(UC_X86_REG_EIP)!=0x30000000: raise ValueError('Original constructor did not return')
        result = {}
        for address in range(0x4e63c8,0x4e67e4,4):
            pointer = u32(address)
            if pointer in records:
                metadata = dict(records[pointer])
                if metadata['objectType']=='animation': metadata['frameDurationMs']=u32(pointer+0x34)
                result[f'0x{address:08x}'] = metadata
        captures.append(result)
    result = captures[0]
    for address, female in captures[1].items():
        if address not in result: raise ValueError(f'Sex-dependent global allocation: {address}')
        male = result[address]
        if male['assetPath'] != female['assetPath']:
            if male['objectType'] != 'sound' or female['objectType'] != 'sound': raise ValueError(f'Unhandled sex-dependent image: {address}')
            male['sexResources'] = [male['assetPath'],female['assetPath']]
    return result
