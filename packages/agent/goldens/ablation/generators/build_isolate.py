"""Isolate WHICH rule in Schema Creation's subtree carries the `supersedes` field.

dev-schema-creation-no-subtrees dropped the whole subtree and lost `supersedes`
3/3. The subtree contains four distinct rules; only one of them can be the cause,
and knowing which decides whether the finding is "completeness rules need prose"
or something narrower.

The four, as they appear in the base case:

  CALL NOW        "your next action is the tool call, not planning text"
  ONE TYPE        "create exactly one. A detail that points at another record is
                   a FIELD on this type, not a second type to define."
  FIELDS          "define only the fields the request implies. Every record
                   already has a title -- do not add a title or name field."
  ENUM            "a field with a fixed set of states gets type enum and every
                   legal value in allowed_values."

ONE TYPE is the suspect: `supersedes` is exactly "a detail that points at another
record", and without that sentence the model may be treating it as out of scope
rather than as a field. If so the finding is not "completeness needs prose" but
the narrower and more interesting "the model needs telling that a cross-record
reference is a field, not a second type".

Each arm keeps exactly one rule and drops the other three, so a pass identifies
the load-bearing sentence directly.
"""
import re

BASE = 'packages/agent/goldens/dev-schema-creation.toml'
src = open(BASE).read()

RULES = {
    'call-now': 'CALL create_schema NOW',
    'one-type': 'ONE TYPE PER REQUEST',
    'fields': 'FIELDS: define only',
    'enum': 'A FIELD WITH A FIXED SET OF STATES',
}

NOTE = '''
ISOLATION ARM -- keeps exactly ONE rule from Schema Creation's subtree ({keep}),
drops the other three. Everything else byte-identical to dev-schema-creation.toml.

WHY

ablation/dev-schema-creation-no-subtrees.toml dropped the whole subtree and lost
the `supersedes` field 3/3 -- the base emits both `status` and `supersedes`, the
stripped arm only `status`. That established the subtree is load-bearing HERE
while being inert on dev-status-change-enum and dev-instance-creation.

But "the subtree matters" is not actionable: the subtree is four separate rules.
This arm set finds which one.

`supersedes` is a reference to another record, so ONE TYPE PER REQUEST ("a detail
that points at another record is a FIELD on this type, not a second type to
define") is the prior suspect. If that arm alone restores the field, the finding
is narrow and precise: the model needs telling that a cross-record reference
belongs as a field. If instead FIELDS restores it, the rule is about completeness
of extraction. If none does, the rules combine and none is individually
sufficient.

BASE (3 reps, byte-identical): create_schema with name "Architecture Decision
Record" and BOTH fields -- status (enum, allowed_values [proposed, accepted]) and
supersedes (text).

STRIPPED (3 reps, byte-identical): same call, `supersedes` absent.
'''


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, (m.group(1) if m else None)


def keep_only(system: str, keep_marker: str) -> str:
    """Drop every rule paragraph in the candidate subtree except the one whose
    text starts with `keep_marker`. Purpose lines and non-rule blocks stay."""
    ref = system.find('--- Candidate 1:')
    if ref < 0:
        return system
    headp = system[:ref]
    block = system[ref:]

    # Paragraphs are blank-line separated. A "rule" paragraph is one that starts
    # with one of the known markers; everything else (title, Purpose, EXISTING
    # SCHEMAS) is structural and always kept.
    paras = block.split('\n\n')
    out = []
    for p in paras:
        flat = p.strip().replace('\\\n', '').replace('\n', ' ')
        is_rule = any(flat.startswith(m.split(':')[0].split(' NOW')[0]) or flat.startswith(m)
                      for m in RULES.values())
        if is_rule and not flat.startswith(keep_marker):
            continue
        out.append(p)
    return headp + '\n\n'.join(out)


turn_starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
head = src[: turn_starts[0]]
turns = [src[turn_starts[i]: (turn_starts[i + 1] if i + 1 < len(turn_starts) else len(src))]
         for i in range(len(turn_starts))]

for slug, marker in RULES.items():
    rebuilt = []
    for t in turns:
        m, system = system_of(t)
        if system is None:
            rebuilt.append(t)
            continue
        rebuilt.append(t[: m.start(1)] + keep_only(system, marker) + t[m.end(1):])

    name = f'schema-creation-only-{slug}'
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""',
                 'notes = """\n' + NOTE.format(keep=marker).strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = f'# derived-from: dev-schema-creation.toml -- keeps only the "{marker}" rule.\n' + out
    out += ''.join(rebuilt)
    path = f'packages/agent/goldens/ablation/{name}.toml'
    open(path, 'w').write(out)
    print(f'wrote {path}')
