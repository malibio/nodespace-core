"""Build the MINIMAL-SUBTREE arms and verify them at production fidelity.

Established so far, all 3 reps byte-identical within each arm, on
dev-schema-creation (1 candidate, 2 tools):

  whole subtree (786 chars)                supersedes PRESENT
  ONE TYPE, 2 sentences                    PRESENT
  ONE TYPE, 1 terse clause (~42 chars)     PRESENT   <- content, not shape
  ENUM rule, 2 sentences                   PRESENT
  FIELDS, 1 sentence                       ABSENT
  FIELDS, expanded to 2 sentences          ABSENT    <- shape refuted
  irrelevant rule (title casing)           MALFORMED <- off-topic prose is harmful
  nothing                                  ABSENT

So one specific rule does the work, and it works because it states something no
parameter schema can express: that a detail pointing at another record is a FIELD
rather than a second type or out of scope.

These arms answer the two questions that remain:

  minimal-subtree     the whole Schema Creation subtree cut to that one clause.
                      Does 786 -> ~42 chars hold on its own case?
  minimal-at-fidelity the same minimal subtree, but on the production-shaped
                      prompt: 9 tools and 3 candidates competing. Everything
                      measured above ran on a single-candidate case, which is
                      exactly the condition the earlier "subtrees are inert"
                      finding failed to generalise from.
"""
import json
import re

MINIMAL = ('ONE TYPE PER REQUEST: create exactly one. A detail that points at another '
           'record is a FIELD on this type, not a second type to define.')

MARKERS = ['CALL create_schema NOW', 'ONE TYPE PER REQUEST', 'FIELDS: define only',
           'A FIELD WITH A FIXED SET OF STATES']


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, (m.group(1) if m else None)


def replace_rules(system, replacement, marker_start='--- Candidate 1:'):
    ref = system.find(marker_start)
    if ref < 0:
        return system
    headp, block = system[:ref], system[ref:]
    out, inserted = [], False
    for p in block.split('\n\n'):
        flat = p.strip().replace('\\\n', '').replace('\n', ' ')
        if any(flat.startswith(m) for m in MARKERS):
            if not inserted:
                out.append(replacement)
                inserted = True
            continue
        out.append(p)
    return headp + '\n\n'.join(out)


def rebuild_file(base_path, out_path, name, notes, transform):
    src = open(base_path).read()
    starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
    head = src[: starts[0]]
    turns = [src[starts[i]: (starts[i + 1] if i + 1 < len(starts) else len(src))]
             for i in range(len(starts))]
    rebuilt = []
    for t in turns:
        m, system = system_of(t)
        if system is None:
            rebuilt.append(t)
            continue
        rebuilt.append(t[: m.start(1)] + transform(system) + t[m.end(1):])
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""', 'notes = """\n' + notes.strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = re.sub(r'(?m)^# derived-from:.*$', f'# derived-from: {base_path.split("/")[-1]}', out, count=1)
    if '# derived-from:' not in out:
        out = f'# derived-from: {base_path.split("/")[-1]}\n' + out
    out += ''.join(rebuilt)
    open(out_path, 'w').write(out)
    print(f'wrote {out_path} ({len(out):,} chars)')


EVIDENCE = '''
EVIDENCE THIS ARM RESTS ON (dev-schema-creation, 3 reps each, byte-identical):

  whole subtree, 786 chars               supersedes PRESENT
  ONE TYPE rule alone, 2 sentences       PRESENT
  ONE TYPE rule alone, 1 terse clause    PRESENT
  ENUM rule alone, 2 sentences           PRESENT
  FIELDS rule alone, 1 sentence          ABSENT
  FIELDS rule expanded to 2 sentences    ABSENT
  an irrelevant 2-sentence rule          MALFORMED OUTPUT
  no subtree at all                      ABSENT

Terse-but-on-topic works; long-but-off-topic does not, and an off-topic rule is
worse than an empty slot -- it produced a call echoing create_schema's own tool
description back as an argument value, 3/3. That is direct support for ADR-064's
deletion-not-substitution rule.
'''

# Arm 1: minimal subtree on its own case.
rebuild_file(
    'packages/agent/goldens/dev-schema-creation.toml',
    'packages/agent/goldens/ablation/schema-creation-minimal.toml',
    'schema-creation-minimal',
    '''
MINIMAL SUBTREE -- Schema Creation's guidance cut from 786 chars to one rule.

Keeps only: "ONE TYPE PER REQUEST: create exactly one. A detail that points at
another record is a FIELD on this type, not a second type to define."

Drops CALL NOW, FIELDS, and the ENUM rule. The ENUM rule independently restores
supersedes, so this arm also tests whether losing it costs the enum's own
correctness -- status must still come back as type "enum" with allowed_values
[proposed, accepted], which is what create_schema's parameter schema already
specifies.
''' + EVIDENCE,
    lambda s: replace_rules(s, MINIMAL),
)

# Arm 2: the same minimal subtree at production fidelity.
# Built from production-baseline, whose Candidate 1 is Node Creation -- so the
# minimal rule replaces THAT subtree, and candidates 2/3 keep theirs, matching
# what a per-candidate minimisation would actually ship.
NODE_CREATION_MINIMAL = ('VALUES WITH NO MATCHING FIELD: put a particular the listed fields do not '
                         'cover into field_values under a key of your own rather than dropping it.')

rebuild_file(
    'packages/agent/goldens/ablation/production-baseline.toml',
    'packages/agent/goldens/ablation/minimal-at-fidelity.toml',
    'minimal-at-fidelity',
    '''
MINIMAL SUBTREE AT PRODUCTION FIDELITY -- 9 tools, 3 candidates.

Every result in the minimisation series above was measured on
dev-schema-creation: one candidate, two tools. That is the same condition the
earlier "subtrees are inert" finding was measured under, and that finding did
NOT generalise -- it held on dev-status-change-enum and dev-instance-creation
and broke on dev-schema-creation.

So the minimal-subtree result has to be re-tested where several candidates and
nine tools compete before it can be relied on.

This arm keeps candidate 1 (Node Creation) cut to its single structurally-
inexpressible rule -- put an uncovered value under a key of your own rather than
dropping it, which no parameter schema can state -- and leaves candidates 2 and
3 untouched.

Baseline to beat: production-baseline.toml, 5/5, turn 1 search_nodes with the
exact enum member in_dev, turn 2 update_node with the real uuid and
field_values={"status": "ready_for_review"}.
''' + EVIDENCE,
    lambda s: replace_rules(s, NODE_CREATION_MINIMAL),
)
