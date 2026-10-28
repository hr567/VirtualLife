"""Rebuild all executable-derived catalogs in dependency order (local research only)."""
import hashlib, json, subprocess, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
original = ROOT/'research/original/N2Game.exe'
catalog = ROOT/'assets/catalog/rules.json'
if not original.is_file(): raise SystemExit('Extract the reference executable locally before exporting tables.')
if catalog.is_file():
    expected = json.loads(catalog.read_text(encoding='utf8'))['sourceSha256']
    if hashlib.sha256(original.read_bytes()).hexdigest() != expected:
        raise SystemExit('Reference executable differs from the catalog revision; use a separate research checkout.')
scripts = [
    'export-rule-tables.py', 'export-gameplay-tables.py', 'export-native-shops.py',
    'export-equipment-tables.py', 'export-inventory-tables.py', 'export-inventory-rendering.py',
    'export-relationship-tables.py',
    'export-competition-tables.py', 'export-running-tables.py',
    'export-battle-tables.py', 'export-battle-rendering.py',
    'export-final-battle-tables.py', 'export-ending-summary.py', 'export-epilogue-tables.py',
    'export-status-rendering.py', 'export-status-submenus.py', 'export-status-property.py', 'export-status-gdi.py',
    'export-map-options.py',
]
# Preserve the complete prior catalog if an individual exporter fails halfway.
previous = catalog.read_bytes() if catalog.exists() else None
try:
    for script in scripts:
        print(f'Exporting {script}', flush=True)
        subprocess.run([sys.executable, str(ROOT/'tools'/script)], cwd=ROOT, check=True)
except BaseException:
    if previous is not None: catalog.write_bytes(previous)
    raise
print('All original tables exported. Run content integrity and native parity tests before committing.')
