"""Isolate why arm C recovered and arm B did not, then build the remaining levers.

B (rank-1 subtree only)            -> turn 1 WRONG 5/5: query "in dev", no filter
C (same + load_procedure + header) -> turn 1 RIGHT 5/5, load_procedure never called

C differs from B in TWO ways at once, so the cause is unattributed:
  1. a 10th tool declaration (load_procedure)
  2. an extra sentence on the REFERENCE header telling the model only the first
     candidate is shown in full

Arms C1/C2 separate them. Arms D/E are the footprint levers proper.
"""
import json
import re

BASE = 'packages/agent/goldens/ablation/production-baseline.toml'
src = open(BASE).read()
turn_starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
head = src[: turn_starts[0]]
turns = [src[turn_starts[0]:turn_starts[1]], src[turn_starts[1]:]]
defs = {t['name']: t for t in json.load(open('/tmp/tool_defs.json'))}

HEADER_NOTE = (' Only the first candidate below shows its full procedure; '
               'call load_procedure to fetch another.')

LOAD_PROCEDURE = {
    'name': 'load_procedure',
    'description': (
        "Fetch the full step-by-step procedure for one of the REFERENCE candidates listed "
        "above by name. Only the first candidate's procedure is shown in full; call this if "
        "a different one fits the request better. Returns the procedure text — follow it and "
        "then make the real tool call in the same turn."
    ),
    'parameters_schema': {
        'type': 'object',
        'required': ['name'],
        'properties': {
            'name': {
                'type': 'string',
                'description': "The candidate's name, copied exactly as it appears after '--- Candidate N:'.",
            }
        },
    },
}

# Node Creation's whitelist -- what a K=1 turn would actually scope to.
K1_TOOLS = ['create_node', 'search_semantic', 'search_nodes', 'get_node']


def esc(s):
    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n')


def toml_key(k):
    return k if re.fullmatch(r'[A-Za-z0-9_-]+', k) else '"' + esc(k) + '"'


def toml_inline(v):
    if isinstance(v, dict):
        return '{ ' + ', '.join(f'{toml_key(k)} = {toml_inline(x)}' for k, x in v.items()) + ' }'
    if isinstance(v, list):
        return '[' + ', '.join(toml_inline(x) for x in v) + ']'
    if isinstance(v, bool):
        return 'true' if v else 'false'
    if isinstance(v, (int, float)):
        return str(v)
    return '"' + esc(str(v)) + '"'


def render_tool_dict(d, indent='  '):
    return '\n'.join([
        f'{indent}[[turn.tool]]',
        f'{indent}name = "{d["name"]}"',
        f'{indent}description = "{esc(d["description"])}"',
        f'{indent}parameters_schema = {toml_inline(d["parameters_schema"])}',
    ])


def render_tool(name, indent='  '):
    return render_tool_dict(defs[name], indent)


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, m.group(1)


def set_system(t, new):
    m, _ = system_of(t)
    return t[: m.start(1)] + new + t[m.end(1):]


def split_candidates(system):
    ref = system.find('REFERENCE')
    headp = system[:ref]
    block = system[ref:]
    hdr_end = block.find('--- Candidate')
    hdr = block[:hdr_end]
    chunks = re.split(r'(?=--- Candidate \d+: )', block[hdr_end:])
    cands = []
    for c in chunks:
        if not c.strip():
            continue
        nl = c.find('\n')
        name = c[:nl].split(': ', 1)[1].strip()
        rest = c[nl + 1:]
        pm = re.match(r'(?s)(Purpose:.*?)(\n\n|\n#)', rest)
        purpose = pm.group(1).strip() if pm else rest.split('\n\n')[0].strip()
        body = rest[len(purpose):].strip() if pm else ''
        cands.append((name, purpose, body))
    return headp, hdr, cands


def rebuild(system, keep_bodies, order=None, header_note='', keep_only=None):
    headp, hdr, cands = split_candidates(system)
    if order:
        by = {n: (n, p, b) for n, p, b in cands}
        cands = [by[n] for n in order if n in by]
    if keep_only:
        cands = [c for c in cands if c[0] in keep_only]
    out = [headp, hdr.rstrip() + header_note + '\n\n']
    for i, (name, purpose, body) in enumerate(cands, 1):
        out.append(f'--- Candidate {i}: {name}\n{purpose}\n')
        if i <= keep_bodies and body:
            out.append(body + '\n')
        out.append('\n')
    return ''.join(out).rstrip()


def write_arm(path, name, notes, keep_bodies, tools=None, order=None,
              add_load_procedure=False, header_note='', keep_only=None):
    rebuilt = []
    for t in turns:
        _, system = system_of(t)
        t2 = set_system(t, rebuild(system, keep_bodies, order, header_note, keep_only))
        if tools is not None or add_load_procedure:
            cut = t2.find('  [[turn.tool]]')
            kept = t2[:cut].rstrip() + '\n\n'
            names = tools if tools is not None else [
                m.group(1) for m in re.finditer(r'(?m)^  name = "(\w+)"', t2[cut:])
            ]
            blocks = [render_tool(n) for n in names]
            if add_load_procedure:
                blocks.append(render_tool_dict(LOAD_PROCEDURE))
            t2 = kept + '\n\n'.join(blocks) + '\n'
        rebuilt.append(t2)
    out = head.replace('name = "production-baseline"', f'name = "{name}"', 1)
    out = re.sub(r'(?s)^notes = """.*?"""', 'notes = """\n' + notes.strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = re.sub(r'(?m)^# derived-from:.*$',
                 '# derived-from: production-baseline.toml -- the honest 9-tool/3-candidate reference.',
                 out, count=1)
    out += ''.join(rebuilt)
    open(path, 'w').write(out)
    print(f'wrote {path} ({len(out):,} chars)')


REF = '''
Reference results, all 5 reps, byte-identical within each arm:
  production-baseline   turn1 RIGHT  turn2 RIGHT   9 tools, 3 full subtrees
  fidelity-no-subtrees  turn1 RIGHT  turn2 RIGHT   9 tools, 0 subtrees
  fidelity-rank1-only   turn1 WRONG  turn2 RIGHT   9 tools, 1 subtree
  fidelity-rank1-fetch  turn1 RIGHT  turn2 RIGHT   10 tools, 1 subtree, 0 fetches

turn 1 RIGHT = search_nodes with node_type "ticket" and filters carrying the
exact enum member "in_dev". The 5/5 failure shape is query "in dev" with no
filter -- right tool, wrong argument shape.
'''

write_arm(
    'packages/agent/goldens/ablation/fidelity-rank1-header-only.toml',
    'fidelity-rank1-header-only',
    '''
ARM C1 -- isolates the HEADER SENTENCE from the load_procedure tool.

Identical to fidelity-rank1-only.toml except the REFERENCE header carries the
extra sentence arm C added. No load_procedure tool.

If this passes, the recovery was the header sentence -- a prompt-only fix, and
the fetch tool is unnecessary. If it fails like B, the recovery came from the
tool declaration's presence instead.
''' + REF,
    keep_bodies=1,
    header_note=HEADER_NOTE,
)

write_arm(
    'packages/agent/goldens/ablation/fidelity-rank1-tool-only.toml',
    'fidelity-rank1-tool-only',
    '''
ARM C2 -- isolates the load_procedure TOOL from the header sentence.

Identical to fidelity-rank1-only.toml except a load_procedure declaration is
added. Header sentence NOT added.

If this passes, a tenth tool declaration alone flipped behavior -- which would
be a property of the tool surface rather than of the guidance, and worth
understanding before relying on either.
''' + REF,
    keep_bodies=1,
    add_load_procedure=True,
)

write_arm(
    'packages/agent/goldens/ablation/fidelity-rank1-wrong-fetch.toml',
    'fidelity-rank1-wrong-fetch',
    '''
ARM D -- rank-1 is deliberately the WRONG procedure, with fetch available.

Candidates reordered so Relationship Management is rank-1 and shows its full
procedure (it instructs create_relationship); Graph Editing and Node Creation
are name+purpose only. load_procedure is offered.

This is the case the rank-1-plus-fetch design exists for: retrieval mis-ranks
roughly 25% of the time, and the question is whether the model recognises the
mismatch and fetches, or simply follows the wrong procedure in front of it.

Three distinguishable outcomes:
  - calls load_procedure, then the right tool  -> the design works
  - calls the right tool without fetching      -> tool schemas carry it; subtrees
                                                  are not load-bearing at all
  - calls create_relationship                  -> the wrong rank-1 procedure won,
                                                  and lazy loading is unsafe
''' + REF,
    keep_bodies=1,
    order=['Relationship Management', 'Graph Editing', 'Node Creation'],
    add_load_procedure=True,
    header_note=HEADER_NOTE,
)

write_arm(
    'packages/agent/goldens/ablation/fidelity-k1.toml',
    'fidelity-k1',
    '''
ARM E -- RETRIEVAL_TOP_K = 1: one candidate, and the tool surface scoped to that
one skill's whitelist.

The largest single footprint lever. Production unions three candidates'
whitelists, and because the read tools appear in 5-6 of the 8 seeded skills, that
union is 9 of 15 registered tools. K=1 gives Node Creation's whitelist alone:
create_node, search_semantic, search_nodes, get_node.

Note this arm removes BOTH the extra candidates and the extra tools, so a failure
does not say which mattered -- fidelity-no-subtrees already isolates the
candidate half. What this measures is whether the turn survives on a 4-tool
surface at all.
''' + REF,
    keep_bodies=1,
    keep_only=['Node Creation'],
    tools=K1_TOOLS,
)

print('done')
