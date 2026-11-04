#!/usr/bin/env python3
"""Removes implemented addresses from the stub lists of the glua .luau files.

    tools/glua/unstub.py Address [Address ...]   (e.g. debug.getinfo  Entity:AddFlags  GM:CalcView)

A stub list is `for _, n in ipairs({ "A", "B" }) do ... stub("prefix" .. n) end` or a direct
`X = stub("Address")`; the matching name is dropped (the whole line when the list empties).
"""
import re, sys, glob, os
here = os.path.dirname(os.path.abspath(__file__))
files = glob.glob(os.path.join(here, '..', '..', 'crates', 'gm-script', 'src', 'glua', '*.luau'))
want = set(sys.argv[1:])
removed = set()
for f in files:
    src = open(f).read().split('\n')
    out = []
    i = -1
    while i + 1 < len(src):
        i += 1
        line = src[i]
        # multi-line list: `for _, name in ipairs({ ... }) do` / `if T[name] == nil then T[name] = stub("P:" .. name) end` / `end`
        m = re.search(r'for _, (\w+) in ipairs\(\{([^}]*)\}\) do\s*$', line)
        if m and i + 2 < len(src):
            mb = re.search(r'stub\("([^"]*)" \.\. ' + m.group(1) + r'\)', src[i + 1])
            if mb and src[i + 2].strip() == 'end':
                prefix = mb.group(1)
                names = re.findall(r'"([^"]*)"', m.group(2))
                keep = [n for n in names if prefix + n not in want]
                for n in names:
                    if prefix + n in want: removed.add(prefix + n)
                if len(keep) == len(names):
                    out.append(line)
                elif keep:
                    out.append(line[:m.start(2)] + ' ' + ', '.join('"%s"' % n for n in keep) + ' ' + line[m.end(2):])
                else:
                    i += 2
                    continue
                out.append(src[i + 1]); out.append(src[i + 2]); i += 2
                continue
        m = re.search(r'ipairs\(\{([^}]*)\}\)(.*?stub\("([^"]*)" \.\. n\))', line)
        if m and 'stub(' in line:
            prefix = m.group(3)
            names = re.findall(r'"([^"]*)"', m.group(1))
            keep = [n for n in names if prefix + n not in want]
            for n in names:
                if prefix + n in want: removed.add(prefix + n)
            if len(keep) == len(names):
                out.append(line)
            elif keep:
                out.append(line[:m.start(1)] + ' ' + ', '.join('"%s"' % n for n in keep) + ' ' + line[m.end(1):])
            continue
        m = re.search(r'stub\("([^"]*)"\)', line)
        if m and m.group(1) in want and '__glua_stub' not in line.split('=')[0]:
            removed.add(m.group(1)); continue
        m = re.search(r'__glua_stub\("([^"]*)"\)', line)
        if m and m.group(1) in want:
            removed.add(m.group(1)); continue
        out.append(line)
    open(f, 'w').write('\n'.join(out))
missing = want - removed
print('removed', len(removed), 'not found:', ' '.join(sorted(missing)))
