"""Apply arm A's treatment (drop instruction subtrees, keep name + Purpose) to
the corpus cases whose guidance encodes rules a parameter schema CANNOT express.

Arm A passed 5/5 on dev-status-change-enum, where every rule the subtrees carry
is also carried structurally: `filters`' shape is in search_nodes' schema, and
the legal enum members are in EXISTING SCHEMAS. That is the easy case for
deletion.

The cases below are the hard ones:

  dev-instance-creation  its guidance carries VALUES WITH NO MATCHING FIELD --
                         ADR-063's `custom:`-prefix rule and the instruction to
                         invent a key rather than drop a value. No parameter
                         schema can say "invent a key for a value the type does
                         not define". This case dropped a user value 3/3 across
                         EIGHT prose arms before the declared-field pattern
                         fixed it (see PATTERN.toml), so it is the one with a
                         measured argument-loss failure mode.

  dev-schema-creation    its guidance carries the create_schema-vs-create_node
                         ontological distinction. A schema cannot express "do
                         not create a type that already exists".

  dev-unseen-schema      generalisation to a type absent from every example.

If arm A's treatment holds here, the subtrees are redundant with the structural
channels generally. If it breaks here, the finding narrows usefully: subtrees
are inert for argument SHAPE and load-bearing for rules only prose can state.
"""
import re
import sys

CASES = [
    'dev-instance-creation',
    'dev-schema-creation',
    'dev-unseen-schema',
]

NOTE_TMPL = '''
ARM A SWEEP -- instruction subtrees dropped, name + Purpose kept.

Derived from {case}.toml, differing in exactly one thing: each candidate's
guidance subtree is removed. The tool surface, EXISTING SCHEMAS, resident
prose, user messages, tool_results, and expect are byte-identical to the base.

WHY THIS CASE

ablation/fidelity-no-subtrees.toml showed the subtrees inert on
dev-status-change-enum at production fidelity (9 tools, 3 candidates), 5/5.
But every rule that case's subtrees carry is ALSO carried structurally --
`filters`' shape by search_nodes' parameter schema, the legal enum members by
EXISTING SCHEMAS. Deleting prose that duplicates a schema is the easy case.

This case is the hard one: its guidance states rules no parameter schema can
express. If the treatment holds here too, the subtrees are redundant in
general. If it breaks, the finding narrows to "inert for argument shape,
load-bearing for what only prose can state" -- which is more precise and more
useful than "inert".

BASE RESULT TO COMPARE AGAINST

Recorded from a 3-rep run of the base case immediately before this arm was
generated. A regression against it is the finding; a tie means the subtree was
not contributing on this case either.
'''


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, (m.group(1) if m else None)


def strip_subtrees(system):
    ref = system.find('REFERENCE')
    if ref < 0:
        return system, 0
    headp = system[:ref]
    block = system[ref:]
    hdr_end = block.find('--- Candidate')
    if hdr_end < 0:
        return system, 0
    hdr = block[:hdr_end]
    chunks = re.split(r'(?=--- Candidate \d+: )', block[hdr_end:])
    out = [headp, hdr.rstrip() + '\n\n']
    dropped = 0
    for c in chunks:
        if not c.strip():
            continue
        nl = c.find('\n')
        title = c[:nl]
        rest = c[nl + 1:]
        pm = re.match(r'(?s)(Purpose:.*?)(\n\n|\n#)', rest)
        purpose = pm.group(1).strip() if pm else rest.split('\n\n')[0].strip()
        body = rest[len(purpose):].strip() if pm else ''
        if body:
            dropped += len(body)
        out.append(f'{title}\n{purpose}\n\n')
    return ''.join(out).rstrip(), dropped


for case in CASES:
    src = open(f'packages/agent/goldens/{case}.toml').read()
    turn_starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
    head = src[: turn_starts[0]]
    turns = [src[turn_starts[i]: (turn_starts[i + 1] if i + 1 < len(turn_starts) else len(src))]
             for i in range(len(turn_starts))]

    rebuilt = []
    total_dropped = 0
    for t in turns:
        m, system = system_of(t)
        if system is None:
            rebuilt.append(t)
            continue
        new_system, dropped = strip_subtrees(system)
        total_dropped += dropped
        rebuilt.append(t[: m.start(1)] + new_system + t[m.end(1):])

    name = f'{case}-no-subtrees'
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""',
                 'notes = """\n' + NOTE_TMPL.format(case=case).strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = f'# derived-from: {case}.toml -- subtrees dropped, everything else identical.\n' + out
    out += ''.join(rebuilt)

    path = f'packages/agent/goldens/ablation/{name}.toml'
    open(path, 'w').write(out)
    print(f'wrote {path}  (dropped {total_dropped:,} chars of subtree)')
