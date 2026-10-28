"""Original complete 0x441a70 introduction and 0x442930 card-shop loops.

The separately tested valuation function receives explicit input rows. All intro
and shop arithmetic, branches and FLC clocks execute unchanged x86 instructions.
Only input, graphics, text presentation and wall-clock reads are replaced.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import *

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))
RULES = json.loads((ROOT / 'assets/catalog/rules.json').read_text(encoding='utf8'))['finalBattle']
STACK = 0x1001e000
STOP = 0x30000ff0

def write(vm, address, value, size=4): vm.mem_write(address, (value & ((1 << (size * 8)) - 1)).to_bytes(size, 'little'))
def read(vm, address, size=4, signed=True): return int.from_bytes(vm.mem_read(address, size), 'little', signed=signed)

class Machine:
    def __init__(self, kind, config):
        self.kind = kind
        self.config = config
        self.clock = config.get('initialClock', 0)
        self.frames = []
        self.current = {}
        self.message = ''
        self.layers = []
        self.draws = []
        self.texts = []
        self.visible_items = []
        self.display_step = 0
        self.objects = {}
        self.reverse = {}
        self.metas = {}
        self.done = False
        self.vm = vm = Uc(UC_ARCH_X86, UC_MODE_32)
        vm.mem_map(0, 4096)
        vm.mem_map(0x400000, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
        vm.mem_write(0x400000, PE.get_memory_mapped_image())
        vm.mem_map(0x10000000, 0x20000)
        vm.mem_map(0x20000000, 0x100000)
        vm.mem_map(0x30000000, 4096)
        write(vm, 0x20000004, 0x30000010)
        write(vm, 0x20000008, 0x30000018)
        runtime_objects = dict(RULES['runtimeObjects'])
        runtime_objects['0x004e67dc'] = {**RULES['resourcesByAddress']['0x004960a8'], 'objectType': 'bitmap'}
        for index, (key, meta) in enumerate(runtime_objects.items()):
            address = int(key, 16)
            obj = 0x20001000 + index * 0x500
            self.objects[address] = obj
            self.reverse[obj] = address
            self.metas[address] = meta
            write(vm, address, obj)
            write(vm, obj, 0x20000000)
            if meta['objectType'] == 'animation':
                for offset, value in [(0xc, meta['x']), (0x10, meta['y']), (0x1c, 0),
                                      (0x24, meta['frameCount']), (0x28, 0), (0x30, 1), (0x34, meta['frameDurationMs'])]:
                    write(vm, obj + offset, value)
            elif meta['objectType'] == 'button':
                write(vm, obj, meta['x'])
                write(vm, obj + 4, meta['y'])
        # The text wrapper is constructed separately and is intentionally not
        # in the resource-object map.
        write(vm, 0x4e6660, 0x200e0000)
        write(vm, 0x4b6454, 0x200fe000)
        write(vm, 0x200fe004, 0x200fe100)
        write(vm, 0x200fe008, 0x200fe200)
        write(vm, 0x46b110, 0x30000020)
        write(vm, STACK, STOP)
        vm.reg_write(UC_X86_REG_ESP, STACK)
        inp = config.get('input', {'career': -1, 'lifeLevel': 1, 'equippedCount': 0, 'valuation': [0] * 14})
        self.input = inp
        write(vm, 0x4b64a2, inp['career'], 2)
        write(vm, 0x4b64a4, inp['lifeLevel'], 2)
        vm.mem_write(0x47ad94, bytes([0] * inp['equippedCount'] + [255] * (6 - inp['equippedCount'])))
        write(vm, 0x4b648a, config.get('money', 0))
        items = config.get('items', [])
        write(vm, 0x4e6508, len(items), 2)
        vm.mem_write(0x4e6760, bytes([255]) * 32)
        vm.mem_write(0x4e6760, bytes(items))
        write(vm, 0x4e6502, -1, 1)
        write(vm, 0x4e665c, -1, 1)
        write(vm, 0x4e649c, config.get('itemScroll', 0), 1)
        if 'introTrack' in config:
            for offset, key in [(0x1c, 'frame'), (0x28, 'lastTick'), (0x30, 'first')]:
                write(vm, self.objects[0x4e6480] + offset, config['introTrack'][key])
        addresses = [0x441860, 0x30000010, 0x30000018, 0x30000020, 0x458ef0,
                     0x458c50, 0x458bf0, 0x458c70, 0x458ba0, 0x458610,
                     0x4597a0, 0x45bd90, 0x45bae0, 0x45ba70, 0x44ca10, 0x45ab10,
                     0x458550, 0x45a8d0, 0x45a940, 0x45a090, 0x459a20, 0x4599e0,
                     0x459b60, 0x4596e0, 0x456c20, 0x456eb0]
        for address in addresses: vm.hook_add(UC_HOOK_CODE, self.hook, begin=address, end=address)

    def ret(self, count=0, value=0):
        vm = self.vm
        sp = vm.reg_read(UC_X86_REG_ESP)
        vm.reg_write(UC_X86_REG_EAX, value & 0xffffffff)
        vm.reg_write(UC_X86_REG_EIP, read(vm, sp, signed=False))
        vm.reg_write(UC_X86_REG_ESP, sp + 4 + count)

    def string(self, pointer):
        return bytes(self.vm.mem_read(pointer, 2048)).split(b'\0', 1)[0].decode('gbk')

    def next_frame(self):
        self.clock = (self.clock + self.config['step']) & 0xffffffff
        index = len(self.frames)
        self.current = dict(next((action for action in self.config.get('actions', []) if action['frame'] == index), {}))
        if self.config.get('continueEvery') and index % self.config['continueEvery'] == 0:
            self.current['action'] = 'continue'
        self.layers = []
        self.draws = []
        self.texts = []
        self.visible_items = []
        if self.kind == 'intro': self.display_step = read(self.vm, STACK - 0x20 + 0x13, 1, False)

    def track(self, address):
        vm = self.vm
        obj = self.objects[address]
        return {'frame': read(vm, obj + 0x1c), 'lastTick': read(vm, obj + 0x28, signed=False), 'first': read(vm, obj + 0x30) == 1}

    def hook(self, vm, address, _size, _):
        sp = vm.reg_read(UC_X86_REG_ESP)
        obj = vm.reg_read(UC_X86_REG_ECX)
        pointer = self.reverse.get(obj)
        if address == 0x441860:
            vm.mem_write(0x4e6514, struct.pack('<14i', *self.input['valuation']))
            self.ret()
        elif address == 0x30000020: self.ret(value=self.clock)
        elif address in [0x30000010, 0x30000018]:
            if pointer in self.metas:
                self.draws.append({'assetPath': self.metas[pointer]['assetPath'], 'x': read(vm, sp + 4), 'y': read(vm, sp + 8), 'opaque': address == 0x30000010})
                if 0x4e65b8 <= pointer < 0x4e65e0: self.visible_items.append((pointer - 0x4e65b8) // 4)
            self.ret(12)
        elif address == 0x458ef0:
            self.next_frame()
            self.ret(4)
        elif address in [0x458c50, 0x458bf0, 0x458c70, 0x458610, 0x4597a0, 0x45a8d0]: self.ret()
        elif address == 0x458ba0: self.ret(16)
        elif address == 0x45bd90: self.message = ''; self.ret()
        elif address == 0x45bae0: self.message += self.string(read(vm, sp + 4, signed=False)); self.ret(4)
        elif address == 0x45ba70: self.texts.append({'message': self.message}); self.ret(4)
        elif address == 0x44ca10:
            self.texts.append({'x': read(vm, sp + 4), 'y': read(vm, sp + 8), 'text': self.string(read(vm, sp + 16, signed=False))})
            self.ret()
        elif address == 0x45ab10:
            self.texts.append({'x': read(vm, sp + 4), 'y': read(vm, sp + 8), 'number': read(vm, sp + 16)})
            self.ret()
        elif address in [0x458550, 0x45a940]:
            meta = self.metas[pointer]
            animation = address == 0x458550
            self.layers.append({'assetPath': meta['assetPath'], 'frame': read(vm, obj + (0x1c if animation else 0x28)),
                                'x': read(vm, obj + (0xc if animation else 0x14)), 'y': read(vm, obj + (0x10 if animation else 0x18)),
                                'transparentIndex': meta.get('transparentIndex'), 'firstPixelTransparent': meta.get('firstPixelTransparent', False)})
            self.ret(4)
        elif address == 0x45a090:
            action = self.current.get('action')
            clicked = ((pointer == 0x4e6488 and action == 'continue') or
                       (pointer == 0x4e678c and action in ['buy', 'sell']) or
                       (pointer == 0x4e6790 and action == 'cancelShop') or
                       (pointer == 0x4e6794 and action == 'scrollLeft') or
                       (pointer == 0x4e6414 and action == 'scrollRight'))
            write(vm, obj + 0x1c, int(clicked))
            self.ret(4)
        elif address == 0x459a20:
            bounds = [read(vm, sp + 4 + n * 4) for n in range(4)]
            action = self.current.get('action')
            position = None
            if self.kind == 'intro' and action == 'continue': position = [1, 1]
            elif action == 'inspectBuy': position = RULES['shopCardPositions'][self.current['index']]
            elif action == 'inspectSell': position = RULES['heldShopPositions'][self.current['index'] - read(vm, 0x4e649c, 1)]
            self.ret(16, int(position is not None and bounds[0] <= position[0] < bounds[2] and bounds[1] <= position[1] < bounds[3]))
        elif address == 0x4599e0: self.ret(16)
        elif address == 0x459b60: self.ret(4)
        elif address == 0x4596e0: self.ret(16)
        elif address == 0x456eb0: self.ret(12)
        elif address == 0x456c20:
            self.snapshot()
            self.ret()
            if len(self.frames) >= self.config.get('limit', 2000): vm.emu_stop()
        else: raise AssertionError(hex(address))

    def snapshot(self):
        vm = self.vm
        items = list(vm.mem_read(0x4e6760, read(vm, 0x4e6508, 2)))
        if self.kind == 'intro':
            base = STACK - 0x20
            done = bool(read(vm, base + 0x1c))
            state = {'introStep': read(vm, base + 0x13, 1, False), 'introStartedMs': vm.reg_read(UC_X86_REG_EBX),
                     'introDurationMs': vm.reg_read(UC_X86_REG_EBP) & 65535,
                     'introTracks': [self.track(0x4e6484), self.track(0x4e6480)],
                     'money': read(vm, 0x4b648a), 'hp': read(vm, 0x4e655c), 'power': read(vm, 0x4e640c, 2),
                     'introDisplayStep': max(self.display_step, 1) if any(layer['assetPath'] == RULES['introAnimations'][1]['assetPath'] for layer in self.layers) else 0,
                     'introMessage': self.message, 'introDisplayedMessage': next((text['message'] for text in self.texts if 'message' in text), ''),
                     'phase': 'shop' if done else 'intro', 'layers': self.layers}
        else:
            base = STACK - 0x28
            done = bool(read(vm, base + 0x1c))
            phase = read(vm, base + 0x13, 1, False)
            buy = read(vm, 0x4e6502, 1)
            sell = read(vm, 0x4e665c, 1)
            scroll = read(vm, 0x4e649c, 1)
            movement = None if phase == 0 else {'item': buy, 'phase': phase, 'x': read(vm, base + 0x14, 2),
                                              'y': (vm.reg_read(UC_X86_REG_EBP) + 32768) % 65536 - 32768,
                                              'originalX': read(vm, base + 0x20, 2), 'originalY': read(vm, base + 0x24, 2)}
            state = {'shopAnimation': movement, 'shopPanelY': read(vm, base + 0x18), 'shopSelectedBuy': None if buy == -1 else buy,
                     'shopSelectedSell': None if sell == -1 else sell + scroll, 'itemScroll': scroll,
                     'shopMessage': self.message, 'money': read(vm, 0x4b648a), 'introTracks': [self.track(0x4e6484), self.track(0x4e6480)],
                     'shopDisplayedPanelY': next(draw['y'] for draw in self.draws if draw['assetPath'] == RULES['resourcesByAddress']['0x004960a8']['assetPath']),
                     'shopDisplayedItemCount': next(text['number'] for text in self.texts if text.get('x') == 269 and text.get('y') == 326),
                     'shopVisibleItems': self.visible_items,
                     'phase': 'opening' if done else 'shop', 'layers': self.layers}
        state['items'] = items
        self.frames.append({'elapsedMs': self.config['step'], 'action': self.current.get('action'),
                            'index': self.current.get('index'), 'after': state, 'draws': self.draws, 'texts': self.texts})
        self.done = done

    def run(self):
        entry = 0x441a70 if self.kind == 'intro' else 0x442930
        try: self.vm.emu_start(entry, STOP, count=30000000)
        except Exception as error:
            raise RuntimeError(f'{error} at {self.vm.reg_read(UC_X86_REG_EIP):08x}, {self.kind} frame {len(self.frames)}') from error
        returned = self.vm.reg_read(UC_X86_REG_EIP) == STOP
        if returned and self.kind == 'intro':
            # Resume the actual caller through its bonus-card allocation and
            # stop immediately before entering the shop.
            self.vm.reg_write(UC_X86_REG_EBP, 0)
            self.vm.emu_start(0x443e2d, 0x443e61, count=10000)
            assert self.vm.reg_read(UC_X86_REG_EIP) == 0x443e61
            self.frames[-1]['after']['items'] = list(self.vm.mem_read(0x4e6760, read(self.vm, 0x4e6508, 2)))
        return {**self.config, 'returned': returned, 'frames': self.frames}

def generate():
    basic = {'sex': 0, 'career': -1, 'lifeLevel': 1, 'equippedCount': 0,
             'valuation': [500000, 12340, 9980, 3355, 1110, 4000, 3000, 1230, 2340, 3450, 4560, 2000, 3, 5]}
    intros = []
    for step, every in [(100, 0), (71, 1), (17, 2), (1000, 1), (10000, 0)]:
        config = {'input': basic, 'step': step, 'continueEvery': every, 'limit': 7000}
        row = Machine('intro', config).run()
        intros.append(row)
        print(f'Intro {len(intros)}: {len(row["frames"])} frames, return {row["returned"]}', flush=True)
    for clock in [100000, 0xfffff000]:
        config = {'input': basic, 'initialClock': clock, 'step': 71, 'continueEvery': 3, 'limit': 7000}
        intros.append(Machine('intro', config).run())
    shops = []
    cases = [
        {'money': 0, 'items': [9] * 8, 'actions': []},
        {'money': 2000, 'items': [9] * 8, 'actions': [{'frame': 3, 'action': 'continue'}]},
    ]
    for index in range(9):
        for money in [RULES['itemPrices'][index], RULES['itemPrices'][index] + 1, 999999]:
            cases.append({'money': money, 'items': [9] * 8, 'actions': [{'frame': 0, 'action': 'inspectBuy', 'index': index},
                         {'frame': 1, 'action': 'buy', 'index': index}, {'frame': 30, 'action': 'continue'}]})
    for count in [0, 9, 10, 11, 30, 31, 32, 33, 36]:
        cases.append({'money': 999999, 'items': [index % 10 for index in range(count)], 'actions': [
            {'frame': 0, 'action': 'inspectBuy', 'index': 3}, {'frame': 1, 'action': 'buy', 'index': 3},
            {'frame': 40, 'action': 'continue'}]})
    for count in [1, 10, 11, 30]:
        cases.append({'money': 999999, 'items': [index % 10 for index in range(count)], 'itemScroll': max(0, count - 10), 'actions': [
            {'frame': 0, 'action': 'inspectSell', 'index': count - 1}, {'frame': 1, 'action': 'sell', 'index': count - 1},
            {'frame': 3, 'action': 'scrollLeft'}, {'frame': 5, 'action': 'scrollRight'}, {'frame': 7, 'action': 'continue'}]})
    cases.append({'money': 999999, 'items': [9] * 15, 'actions': [
        {'frame': 0, 'action': 'inspectBuy', 'index': 3}, {'frame': 1, 'action': 'cancelShop'},
        {'frame': 2, 'action': 'inspectSell', 'index': 3}, {'frame': 3, 'action': 'cancelShop'},
        {'frame': 4, 'action': 'inspectSell', 'index': 4}, {'frame': 5, 'action': 'scrollRight'},
        {'frame': 6, 'action': 'inspectSell', 'index': 7}, {'frame': 7, 'action': 'scrollLeft'}, {'frame': 9, 'action': 'continue'}]})
    cases.append({'money': 999999, 'items': [9] * 8, 'actions': [
        {'frame': 0, 'action': 'inspectBuy', 'index': 8}, {'frame': 1, 'action': 'buy', 'index': 8},
        {'frame': 2, 'action': 'continue'}]})
    for config in cases:
        config.update(step=71, limit=100)
        row = Machine('shop', config).run()
        shops.append(row)
    out = {'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
           'method': __doc__, 'intro': intros, 'shop': shops}
    (ROOT / 'tests/fixtures/original-final-intro-shop.json').write_text(json.dumps(out, separators=(',', ':')) + '\n', encoding='utf8')
    print(f'Generated {len(intros)} introductions / {sum(len(row["frames"]) for row in intros)} frames and {len(shops)} shops / {sum(len(row["frames"]) for row in shops)} frames')

if __name__ == '__main__': generate()
