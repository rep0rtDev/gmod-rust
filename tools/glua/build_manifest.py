#!/usr/bin/env python3
"""GLua coverage manifest.

    tools/glua/build_manifest.py [--pages DIR] [--present FILE] [--stubs FILE]

--pages: a directory of cached wiki pages (`https://wiki.facepunch.com/gmod/<addr>?format=json`,
one file per page, `/` in the address written as `__`); rebuilds the catalogue entries
(kind, parent, realms, args, rets) and keeps the recorded statuses. Without it the existing
manifest's entries are kept.
--present / --stubs: the output of `cargo run -p gm-script --example glua_list` (one address per
line); sets every entry's status to implemented / stub / missing accordingly.

Also writes docs/glua/coverage.md (counts per realm and per library / class).
"""
import json, os, re, sys, glob, collections

here = os.path.dirname(os.path.abspath(__file__))
repo = os.path.dirname(os.path.dirname(here))
manifest_path = os.path.join(here, 'manifest.json')
coverage_path = os.path.join(repo, 'docs', 'glua', 'coverage.md')

args = sys.argv[1:]
def opt(name):
    if name in args:
        i = args.index(name)
        return args[i + 1]
    return None
pages_dir, present_file, stubs_file = opt('--pages'), opt('--present'), opt('--stubs')

old = {}
if os.path.exists(manifest_path):
    for e in json.load(open(manifest_path))['functions']:
        old[e['address']] = e

tag_re = re.compile(r'<(function|hook|panel|enum|structure|type)\b([^>]*)>', re.S)
attr_re = re.compile(r'(\w+)="([^"]*)"')
arg_re = re.compile(r'<arg\s+([^>]*)>(.*?)</arg>|<arg\s+([^>]*)/>', re.S)
ret_re = re.compile(r'<ret\s+([^>]*)>(.*?)</ret>|<ret\s+([^>]*)/>', re.S)
realm_re = re.compile(r'<realm>(.*?)</realm>', re.S)

def params(markup, rx):
    res = []
    for mm in rx.finditer(markup):
        a = dict(attr_re.findall(mm.group(1) or mm.group(3) or ''))
        p = {'name': a.get('name', ''), 'type': a.get('type', '')}
        if 'default' in a:
            p['default'] = a['default']
        res.append(p)
    return res

entries = []
if pages_dir:
    for f in sorted(glob.glob(os.path.join(pages_dir, '*.json'))):
        try:
            page = json.load(open(f))
        except Exception:
            continue
        addr, markup = page.get('address', ''), page.get('markup', '')
        tags = page.get('tags', '').split()
        m = tag_re.search(markup)
        if not m:
            continue
        kind = m.group(1)
        attrs = dict(attr_re.findall(m.group(2)))
        realms = [t[len('realm-'):] for t in tags if t.startswith('realm-')]
        rm = realm_re.search(markup)
        if rm and not realms:
            realms = [r.strip().lower() for r in re.split(r'\s+and\s+|,', rm.group(1)) if r.strip()]
        e = {
            'address': addr,
            'kind': kind if kind != 'function' else attrs.get('type', 'function'),
            'parent': attrs.get('parent', ''),
            'name': attrs.get('name', addr.split(':')[-1].split('.')[-1]),
            'realms': realms,
            'deprecated': 'deprecated' in tags,
            'internal': 'internal' in tags,
            'args': params(markup, arg_re),
            'rets': params(markup, ret_re),
            'status': old.get(addr, {}).get('status', 'missing'),
        }
        if old.get(addr, {}).get('note'):
            e['note'] = old[addr]['note']
        entries.append(e)
else:
    entries = list(old.values())

def read_list(path):
    return set(l.strip() for l in open(path) if l.strip()) if path else None
present, stubs = read_list(present_file), read_list(stubs_file)
if present is not None:
    stubs = stubs or set()
    for e in entries:
        a = e['address']
        if '(' in a:
            # a documented alias page (`Global.error(lowercase)`): probe the real name
            sep = '.' if e['kind'] == 'libraryfunc' else ':'
            a = f"{e['parent']}{sep}{e['name']}"
        if e['kind'] in ('enum', 'structure', 'type'):
            e['status'] = 'implemented' if a in present else e['status']
            continue
        # A hook the server really raises counts even when its gamemode default is a stub.
        if a in present and (a not in stubs or e['kind'] == 'hook'):
            e['status'] = 'implemented'
        elif a in stubs:
            e['status'] = 'stub'
        else:
            e['status'] = 'missing'

entries.sort(key=lambda e: e['address'])
for e in entries:
    for flag in ('deprecated', 'internal'):
        if flag in e and not e[flag]:
            del e[flag]
with open(manifest_path, 'w') as f:
    # one entry per line: diffable, and under the public tree's 2 MB cap
    f.write('{"source": "https://wiki.facepunch.com/gmod (page markup)", "functions": [\n')
    f.write(',\n'.join(json.dumps(e, ensure_ascii=False, separators=(',', ':')) for e in entries))
    f.write('\n]}\n')

# coverage summary
funcs = [e for e in entries if e['kind'] not in ('enum', 'structure', 'type')]
def realm_of(e):
    r = set(e['realms'])
    if 'server' in r and 'client' in r: return 'shared'
    if 'server' in r: return 'server'
    if 'client' in r: return 'client'
    if 'menu' in r: return 'menu'
    return 'unknown'
lines = ['# GLua coverage', '',
         'Generated by `tools/glua/build_manifest.py` from `tools/glua/manifest.json`; the statuses come from',
         '`cargo run -p gm-script --example glua_list` (what an actor VM really provides). `implemented` = the',
         'function exists and does its documented job here, `stub` = exists but raises "not available in this',
         'realm yet", `missing` = absent. Realm: shared = client and server.', '']
tot = collections.Counter(e['status'] for e in funcs)
lines.append(f"Total documented functions / hooks: {len(funcs)}: implemented {tot['implemented']}, stub {tot['stub']}, missing {tot['missing']}.")
lines.append('')
lines.append('| realm | total | implemented | stub | missing |')
lines.append('|---|---|---|---|---|')
by_realm = collections.defaultdict(collections.Counter)
for e in funcs:
    by_realm[realm_of(e)][e['status']] += 1
    by_realm[realm_of(e)]['total'] += 1
for r in ['shared', 'server', 'client', 'menu', 'unknown']:
    c = by_realm.get(r)
    if c:
        lines.append(f"| {r} | {c['total']} | {c['implemented']} | {c['stub']} | {c['missing']} |")
lines += ['', '| parent | kind | total | implemented | stub | missing |', '|---|---|---|---|---|---|']
by_parent = collections.defaultdict(collections.Counter)
kind_of = {}
for e in funcs:
    key = e['parent'] or e['address'].split('/')[0]
    by_parent[key][e['status']] += 1
    by_parent[key]['total'] += 1
    kind_of[key] = e['kind']
for key in sorted(by_parent, key=lambda k: (-by_parent[k]['total'], k)):
    c = by_parent[key]
    lines.append(f"| {key} | {kind_of[key]} | {c['total']} | {c['implemented']} | {c['stub']} | {c['missing']} |")
os.makedirs(os.path.dirname(coverage_path), exist_ok=True)
open(coverage_path, 'w').write('\n'.join(lines) + '\n')
print(len(entries), 'entries;', dict(tot))
