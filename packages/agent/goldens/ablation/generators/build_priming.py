"""Test whether Schema Creation's subtree works by CONTENT or by PRIMING.

Isolation result that motivates this:

  kept rule                                   supersedes present?
  ONE TYPE PER REQUEST (2 sentences)          YES 3/3
  A FIELD WITH A FIXED SET OF STATES (2 sent) YES 3/3
  FIELDS: define only the fields implied      NO  3/3
  CALL create_schema NOW                      NO  3/3
  (nothing)                                   NO  3/3

If the mechanism were semantic, only ONE TYPE should work -- `supersedes` is a
cross-record reference and that is the rule naming references. The enum rule says
nothing about references, yet also restores the field. And FIELDS, which states
completeness most directly, fails.

What the two winners share is SHAPE, not meaning: both are two sentences with a
concrete consequence attached ("...not a second type to define", "...can never be
filled legally afterwards"). Both losers are single terse directives.

So the hypothesis under test: the slot works by keeping the model in careful
extraction mode, and the specific content matters less than its presence and
weight. Arms:

  irrelevant-long   a two-sentence rule of matching shape about something with
                    NO bearing on fields (title casing). If supersedes appears,
                    content is not the mechanism.
  fields-expanded   the FIELDS rule expanded to two sentences with a consequence,
                    matching the winners' shape. If it now works, shape is the
                    mechanism and the losing rules were simply too terse.
  one-type-terse    ONE TYPE cut to a single terse directive, matching the
                    losers' shape. If it now fails, that confirms shape over
                    content from the other direction.
"""
import re

BASE = 'packages/agent/goldens/dev-schema-creation.toml'
src = open(BASE).read()

MARKERS = ['CALL create_schema NOW', 'ONE TYPE PER REQUEST', 'FIELDS: define only',
           'A FIELD WITH A FIXED SET OF STATES']

REPLACEMENTS = {
    'irrelevant-long': (
        'TITLE CASING: render the type\'s display name in Title Case, capitalising '
        'each significant word. A lowercased display name reads as an internal id '
        'rather than something the user named.'
    ),
    'fields-expanded': (
        'FIELDS: define every field the request implies, and only those. A detail '
        'the user stated that ends up with no field is lost silently while the type '
        'still reports as created.'
    ),
    'one-type-terse': 'ONE TYPE PER REQUEST: create exactly one.',
}

NOTE = '''
PRIMING vs CONTENT -- arm: {slug}

Isolation (each keeping ONE rule from Schema Creation's subtree, 3 reps each,
byte-identical within arm):

  ONE TYPE PER REQUEST          supersedes PRESENT
  A FIELD WITH A FIXED SET...   supersedes PRESENT
  FIELDS: define only...        supersedes ABSENT
  CALL create_schema NOW        supersedes ABSENT
  whole subtree dropped         supersedes ABSENT

A semantic mechanism predicts only ONE TYPE works -- it is the rule that names
cross-record references, and `supersedes` is one. The enum rule mentions no such
thing and works anyway; FIELDS states completeness most directly and fails. What
the winners share is shape: two sentences, each with a concrete consequence.

This arm set separates the two explanations.

  irrelevant-long   two-sentence rule with a consequence, about TITLE CASING --
                    nothing to do with which fields exist. supersedes appearing
                    here means content is not the mechanism.
  fields-expanded   the failing FIELDS rule given the winners' shape. Working
                    here means the losers failed for being terse, not wrong.
  one-type-terse    the winning ONE TYPE rule cut to the losers' shape. Failing
                    here confirms it from the other side.

BASE (3 reps): create_schema "Architecture Decision Record" with BOTH status
(enum, [proposed, accepted]) and supersedes (text).
'''


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, (m.group(1) if m else None)


def replace_rules(system: str, replacement: str) -> str:
    """Drop every rule paragraph, then insert `replacement` in their place."""
    ref = system.find('--- Candidate 1:')
    if ref < 0:
        return system
    headp, block = system[:ref], system[ref:]
    paras = block.split('\n\n')
    out, inserted = [], False
    for p in paras:
        flat = p.strip().replace('\\\n', '').replace('\n', ' ')
        if any(flat.startswith(m) for m in MARKERS):
            if not inserted:
                out.append(replacement)
                inserted = True
            continue
        out.append(p)
    return headp + '\n\n'.join(out)


turn_starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
head = src[: turn_starts[0]]
turns = [src[turn_starts[i]: (turn_starts[i + 1] if i + 1 < len(turn_starts) else len(src))]
         for i in range(len(turn_starts))]

for slug, replacement in REPLACEMENTS.items():
    rebuilt = []
    for t in turns:
        m, system = system_of(t)
        if system is None:
            rebuilt.append(t)
            continue
        rebuilt.append(t[: m.start(1)] + replace_rules(system, replacement) + t[m.end(1):])

    name = f'schema-creation-{slug}'
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""',
                 'notes = """\n' + NOTE.format(slug=slug).strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = f'# derived-from: dev-schema-creation.toml -- rules replaced with: {slug}\n' + out
    out += ''.join(rebuilt)
    path = f'packages/agent/goldens/ablation/{name}.toml'
    open(path, 'w').write(out)
    print(f'wrote {path}')
