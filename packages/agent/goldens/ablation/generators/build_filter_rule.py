"""Close the two remaining sweep failures: search_nodes filter-array construction.

Sweep result (KEEP/DROP rule applied to all 8 corpus cases, 3 reps each):

  6 of 8 clean. Two turn-1 failures, and they share a signature -- both are
  search_nodes FILTER ARRAY construction, and both are cases whose minimal rule
  was None (subtree removed entirely):

    dev-status-change-enum  query "\\"\\"\\"" -- a corrupted empty-string literal
    dev-unseen-schema       {"operator":"in","value":"cut,soak"} -- a comma-joined
                            string where `in` needs an array

Both produce a filter that silently matches nothing, which is indistinguishable
from a genuinely empty result. That is the same class #2159 recorded when
declaring enums on filters yielded correct VALUES in a broken wire FORMAT.

So the KEEP test needs one more clause. Filter-array construction is nominally
"argument shape" -- schema territory -- but search_nodes' schema is depth 7 with
arrays of objects, and the model does not reliably build it from the schema
alone. Whether that is a legitimate KEEP (the schema cannot in practice state it)
or a schema defect (the schema should be flatter) is exactly what these arms
separate.

  filter-rule       minimal case + one sentence on filter construction. If this
                    closes both, deep-array construction is a real KEEP category.
  query-empty-rule  minimal case + one sentence on the empty-query convention
                    only. Narrower: tests whether dev-status-change-enum's
                    failure is specifically the '' vs '*' convention rather than
                    array building in general.
"""
import re

FILTER_RULE = ('FILTERS: each filter is one object with property, operator, and value. '
               'An operator taking several values takes them as a list, never as one '
               'comma-joined string.')

QUERY_RULE = ('LISTING BY TYPE: to list every node of a type, pass query as an empty '
              'string and set node_type.')

RULE_START = re.compile(r'^[A-Z][A-Z ,\'-]{3,}:|^CALL [a-z_]+ NOW')

TARGETS = {
    'dev-status-change-enum': {
        'filter-rule': FILTER_RULE,
        'query-empty-rule': QUERY_RULE,
    },
    'dev-unseen-schema': {
        'filter-rule': FILTER_RULE,
    },
}

NOTE = '''
FILTER-CONSTRUCTION ARM -- {case}, rule: {slug}

Closes (or fails to close) the two remaining failures from the minimal-subtree
sweep. That sweep applied one editorial rule to all 8 corpus cases:

  KEEP  a sentence stating something no parameter schema and no EXISTING SCHEMAS
        block can express -- an ontological distinction, or an instruction to act
        on something ABSENT from the declared surface.
  DROP  a sentence a schema already carries -- argument shape, required-ness,
        enum membership, key spelling, "call X now".

6 of 8 cases passed 3/3. The two failures are both search_nodes turn-1 FILTER
ARRAY construction:

  dev-status-change-enum  query "\\"\\"\\"" -- corrupted empty-string literal
  dev-unseen-schema       {{"operator":"in","value":"cut,soak"}} -- comma-joined
                          string where `in` needs a list

Both yield a filter matching nothing, indistinguishable from a real empty result.
Same class as #2159, where enums-on-filters gave right values in a broken format.

THE RULE ADDED HERE

"{rule}"

This is nominally argument shape, which the KEEP test assigns to the schema. But
search_nodes' parameter schema is depth 7 with arrays of objects, and the model
does not build it reliably from the schema alone. If this arm closes the failure,
deep-array construction is a genuine KEEP category and the editorial rule needs
that clause. If it does not, the problem is the schema's shape rather than
missing prose, and the fix belongs in tools.rs.

BASE (unmodified case): passes 3/3.
MINIMAL (subtree dropped): turn 1 fails as above, turn 2 correct.
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
            if not inserted:
                out.append(replacement)
                inserted = True
            continue
        out.append(p)
    if not inserted:
        out.append(replacement)
    return headp + '\n\n'.join(out)


for case, variants in TARGETS.items():
    src = open(f'packages/agent/goldens/{case}.toml').read()
    starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
    head = src[: starts[0]]
    turns = [src[starts[i]: (starts[i + 1] if i + 1 < len(starts) else len(src))]
             for i in range(len(starts))]

    for slug, rule in variants.items():
        rebuilt = []
        for t in turns:
            m, system = system_of(t)
            if system is None:
                rebuilt.append(t)
                continue
            rebuilt.append(t[: m.start(1)] + transform(system, rule) + t[m.end(1):])

        name = f'{case}-{slug}'
        out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
        out = re.sub(r'(?s)^notes = """.*?"""',
                     'notes = """\n' + NOTE.format(case=case, slug=slug, rule=rule).strip() + '\n"""',
                     out, count=1, flags=re.M)
        out = f'# derived-from: {case}.toml -- minimal subtree plus one filter rule.\n' + out
        out += ''.join(rebuilt)
        path = f'packages/agent/goldens/ablation/{name}.toml'
        open(path, 'w').write(out)
        print(f'wrote {path}')
