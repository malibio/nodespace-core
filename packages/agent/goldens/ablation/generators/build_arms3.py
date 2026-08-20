"""Generate the footprint-reduction arm set, all derived from production-baseline.toml.

The baseline (9 tools, 3 candidates, 22,064 chars, 5/5) is the honest reference.
Every arm below subtracts or restructures exactly one thing from it, so a
difference is attributable.

Arms:
  A  no-subtrees-at-fidelity   candidate names+purposes, no instruction subtrees
                               (re-tests the "block is inert" finding, which was
                               measured at 1 tool / 1 candidate)
  B  rank1-only               candidate 1's subtree in full; 2 and 3 name-only
  C  rank1-plus-fetch         same as B, plus a load_procedure tool the model can
                               call for the withheld subtrees
  D  rank1-wrong-plus-fetch   like C, but candidates reordered so rank-1 is the
                               WRONG procedure -- does it fetch, and then act?
  E  k1                        only candidate 1, and the tool surface scoped to
                               that one skill's whitelist (4 tools, not 9)
"""
import json
import re

BASE = 'packages/agent/goldens/ablation/production-baseline.toml'
src = open(BASE).read()

turn_starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
head = src[: turn_starts[0]]
turns = [src[turn_starts[0]:turn_starts[1]], src[turn_starts[1]:]]

defs = {t['name']: t for t in json.load(open('/tmp/tool_defs.json'))}


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


def render_tool(name, indent='  '):
    d = defs[name]
    return '\n'.join([
        f'{indent}[[turn.tool]]',
        f'{indent}name = "{name}"',
        f'{indent}description = "{esc(d["description"])}"',
        f'{indent}parameters_schema = {toml_inline(d["parameters_schema"])}',
    ])


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


def system_of(turn_text):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', turn_text)
    return m, m.group(1)


def set_system(turn_text, new_system):
    m, _ = system_of(turn_text)
    return turn_text[: m.start(1)] + new_system + turn_text[m.end(1):]


def split_candidates(system):
    """-> (head_before_REFERENCE, ref_header, [(name, purpose, body), ...])"""
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
        first_nl = c.find('\n')
        title = c[:first_nl]
        name = title.split(': ', 1)[1].strip()
        rest = c[first_nl + 1:]
        pm = re.match(r'(?s)(Purpose:.*?)(\n\n|\n#)', rest)
        purpose = pm.group(1).strip() if pm else rest.split('\n\n')[0].strip()
        body = rest[len(purpose):].strip() if pm else ''
        cands.append((name, purpose, body))
    return headp, hdr, cands


def rebuild(system, keep_bodies, order=None, extra_note=''):
    headp, hdr, cands = split_candidates(system)
    if order:
        by = {n: (n, p, b) for n, p, b in cands}
        cands = [by[n] for n in order if n in by]
    out = [headp, hdr.rstrip() + extra_note + '\n\n']
    for i, (name, purpose, body) in enumerate(cands, 1):
        out.append(f'--- Candidate {i}: {name}\n{purpose}\n')
        if i <= keep_bodies and body:
            out.append(body + '\n')
        out.append('\n')
    return ''.join(out).rstrip()


def write_arm(path, name, notes, keep_bodies, tools=None, order=None,
              add_load_procedure=False, extra_note=''):
    rebuilt = []
    for t in turns:
        _, system = system_of(t)
        new_system = rebuild(system, keep_bodies, order, extra_note)
        t2 = set_system(t, new_system)
        if tools is not None or add_load_procedure:
            cut = t2.find('  [[turn.tool]]')
            kept = t2[:cut].rstrip() + '\n\n'
            names = tools if tools is not None else [
                m.group(1) for m in re.finditer(r'(?m)^  name = "(\w+)"', t2[cut:])
            ]
            blocks = [render_tool(n) for n in names]
            if add_load_procedure:
                d = LOAD_PROCEDURE
                blocks.append('\n'.join([
                    '  [[turn.tool]]',
                    f'  name = "{d["name"]}"',
                    f'  description = "{esc(d["description"])}"',
                    f'  parameters_schema = {toml_inline(d["parameters_schema"])}',
                ]))
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


BASELINE_NOTE = '''
Baseline to compare against: production-baseline.toml, 5/5 byte-identical --
turn 1 search_nodes with node_type "ticket" and a status filter of the exact
enum member "in_dev"; turn 2 update_node with the uuid from turn 1's tool
result plus field_values={"status": "ready_for_review"}.

A difference inside one rep is noise. Identical code has scored 6/7/7 on the
agent matrix.
'''

write_arm(
    'packages/agent/goldens/ablation/fidelity-no-subtrees.toml',
    'fidelity-no-subtrees',
    '''
ARM A -- re-tests the "candidate block is inert" finding at production fidelity.

ablation/no-instruction-subtree.toml concluded the instruction subtrees are
inert, but measured that with ONE tool and ONE candidate on offer. Disambiguating
among nine tools and three competing procedures is the block's actual job, so
inertness has never been tested where it would matter.

This arm keeps all three candidates' names and Purpose lines and drops only
their instruction subtrees. Tool surface unchanged at 9.

A TIE with the baseline means ~4KB of subtree is buying nothing even at real
fidelity, and can simply be deleted -- no new mechanism needed.
''' + BASELINE_NOTE,
    keep_bodies=0,
)

write_arm(
    'packages/agent/goldens/ablation/fidelity-rank1-only.toml',
    'fidelity-rank1-only',
    '''
ARM B -- rank-1 gets its full procedure; ranks 2 and 3 are name+purpose only.

Retrieval's rank-1 is correct roughly 75% of the time (ADR-064's description
rewrite measurement), so the full subtrees for candidates 2 and 3 are paid on
every turn to cover the minority case. This arm tests whether the majority case
survives on rank-1's procedure alone.

No fetch mechanism here -- that is arm C. If this ties the baseline, the
withheld subtrees were not contributing on a turn where rank-1 was right.
''' + BASELINE_NOTE,
    keep_bodies=1,
)

write_arm(
    'packages/agent/goldens/ablation/fidelity-rank1-fetch.toml',
    'fidelity-rank1-fetch',
    '''
ARM C -- arm B plus a `load_procedure` tool for the withheld subtrees.

Tests the hazard directly: does adding a fetch tool cause the model to fetch when
it does not need to, or to narrate instead of act?

ADR-064 rule 4 measured instructions delivered as a TOOL RESULT dropping
continuation from 100% to 44% and yielding prose. Here rank-1's procedure stays
in the prompt, so the risky channel is only used for a candidate the model
actively decides it wants -- the hedge that distinguishes this from full lazy
loading.

Expected on this case: rank-1 (Node Creation... then Graph Editing on turn 2) is
adequate, so a PASS means no needless fetch. Any load_procedure call here is
itself the finding.
''' + BASELINE_NOTE,
    keep_bodies=1,
    add_load_procedure=True,
    extra_note=' Only the first candidate below shows its full procedure; call load_procedure to fetch another.',
)

print('done')
