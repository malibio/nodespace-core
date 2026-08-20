"""Apply the minimal-subtree treatment across every corpus case and measure.

Established: a subtree can be cut to the rule(s) stating something a parameter
schema CANNOT express, and the cut holds -- both on its own case (786 -> 137
chars, 3/3) and at production fidelity with 9 tools and 3 candidates (5/5).

This sweep applies the same editorial rule to every remaining case and measures
each against its own base, so the claim is either general or bounded by evidence
rather than by one example.

The editorial rule being applied, stated so it is reproducible:

  KEEP a sentence if it states something no parameter schema and no EXISTING
  SCHEMAS block can express -- an ontological distinction (kind vs instance,
  reference-as-field), or an instruction to act on something ABSENT from the
  declared surface (a value with no matching field).

  DROP a sentence if a schema already carries it: argument shape, required-ness,
  enum membership, key spelling, "call X now", or a restatement of what the tool
  description says.

Per-case minimal rules below were chosen by that test, not by length.
"""
import re

# case -> (minimal replacement, base result to compare against)
CASES = {
    'dev-instance-creation': (
        'VALUES WITH NO MATCHING FIELD: put a particular the listed fields do not cover into '
        'field_values under a key of your own rather than dropping it.',
        'create_node with all five values incl. blocked_by (unlisted)',
    ),
    'dev-status-change-enum': (
        None,  # no rule survives the test -- everything its subtree says is in a schema
        'turn1 search_nodes exact enum in_dev; turn2 update_node real uuid + ready_for_review',
    ),
    'dev-relationship-creation': (
        'DIRECTION: from_id is the record that ACTS, to_id is the record acted upon.',
        'create_relationship, correct direction',
    ),
    'dev-ambiguous-clarify': (
        None,
        'route_clarify with both real ids',
    ),
    'dev-empty-result-query': (
        None,
        'prose reply, no tool call',
    ),
    'dev-indirect-reference': (
        None,
        '3 turns, resolves indirect reference then updates',
    ),
    'dev-unseen-schema': (
        None,
        'turn1 search_nodes on release; turn2 update_node stage=soak',
    ),
}

RULE_START = re.compile(r'^[A-Z][A-Z ,\'-]{3,}:|^CALL [a-z_]+ NOW')

NOTE = '''
MINIMAL-SUBTREE SWEEP -- {case}

The editorial rule applied here, chosen by what a schema CAN and CANNOT state:

  KEEP  a sentence stating something no parameter schema and no EXISTING SCHEMAS
        block can express -- an ontological distinction (kind vs instance,
        reference-as-field) or an instruction to act on something ABSENT from
        the declared surface (a value with no matching field).
  DROP  a sentence a schema already carries -- argument shape, required-ness,
        enum membership, key spelling, "call X now", or a restatement of the
        tool description.

{applied}

EVIDENCE THIS RULE RESTS ON (dev-schema-creation, 3 reps each, byte-identical):

  whole subtree, 786 chars              supersedes PRESENT
  ONE TYPE rule alone, 2 sentences      PRESENT
  ONE TYPE rule alone, 1 terse clause   PRESENT
  ENUM rule alone, 2 sentences          PRESENT
  FIELDS rule alone                     ABSENT
  FIELDS expanded to 2 sentences        ABSENT
  an irrelevant 2-sentence rule         MALFORMED OUTPUT
  no subtree at all                     ABSENT

Terse-but-on-topic works; long-but-off-topic does not, and off-topic prose in the
slot is worse than an empty slot. Confirmed at production fidelity (9 tools, 3
candidates) by ablation/minimal-at-fidelity.toml, 5/5.

BASE RESULT: {base}
'''


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, (m.group(1) if m else None)


def transform(system, replacement):
    ref = system.find('--- Candidate 1:')
    if ref < 0:
        return system
    headp, block = system[:ref], system[ref:]
    out, inserted = [], False
    for p in block.split('\n\n'):
        flat = p.strip().replace('\\\n', '').replace('\n', ' ')
        if RULE_START.match(flat):
            if replacement and not inserted:
                out.append(replacement)
                inserted = True
            continue
        out.append(p)
    return headp + '\n\n'.join(out)


for case, (replacement, base_result) in CASES.items():
    path = f'packages/agent/goldens/{case}.toml'
    try:
        src = open(path).read()
    except FileNotFoundError:
        print(f'skip {case}: no such file')
        continue
    starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
    head = src[: starts[0]]
    turns = [src[starts[i]: (starts[i + 1] if i + 1 < len(starts) else len(src))]
             for i in range(len(starts))]

    before = after = 0
    rebuilt = []
    for t in turns:
        m, system = system_of(t)
        if system is None:
            rebuilt.append(t)
            continue
        before += len(system)
        new = transform(system, replacement)
        after += len(new)
        rebuilt.append(t[: m.start(1)] + new + t[m.end(1):])

    applied = (f'Kept for this case: "{replacement}"' if replacement
               else 'Nothing survives the KEEP test for this case: every rule its subtree '
                    'states is already carried by a parameter schema or by EXISTING SCHEMAS, '
                    'so the subtree is dropped entirely.')

    name = f'{case}-minimal'
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""',
                 'notes = """\n' + NOTE.format(case=case, applied=applied, base=base_result).strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = f'# derived-from: {case}.toml -- subtree minimised per the KEEP/DROP rule.\n' + out
    out += ''.join(rebuilt)
    op = f'packages/agent/goldens/ablation/{name}.toml'
    open(op, 'w').write(out)
    print(f'{case:28s} system {before:>6,} -> {after:>6,}  ({after - before:+,})')
