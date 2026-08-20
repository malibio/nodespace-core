"""Isolate the name truncation: does the guidance's own example leak into output?

prod-guidance-trimmed-v3 produced name "ADR" where the base and the full-guidance
arm both produce "Architecture Decision Record". The user's message never says
"ADR" -- it says "architecture decision records".

The only place the literal token "ADR" appears is inside ONE_SCHEMA_PER_REQUEST's
own worked example:

    Do NOT proactively invent or create related types (e.g. asked for "ADR", do
    not also create "Ticket" or "Sprint")

That example exists to teach a DIFFERENT rule (do not create extra types), but it
supplies a name that the model then adopts for the type it IS creating.

If true, this is contamination of a kind ADR-064 does not currently name: not a
channel conflict, but an illustrative example leaking its incidental details into
output. The eval-contamination guard (#1932) covers eval prompts sharing text
with guidance -- this is the mirror case, guidance supplying text the output then
copies.

Arms, each differing from prod-guidance-trimmed-v3 in exactly one way:

  adr-example-generic   the example's "ADR"/"Ticket"/"Sprint" replaced with
                        neutral placeholders. If the name comes back full, the
                        example was the source.
  adr-example-removed   the parenthetical example deleted entirely, rule text
                        otherwise identical. Controls for the possibility that
                        ANY example in that slot causes it.
"""
import re

src = open('packages/agent/src/skill_rules.rs').read()


def rule_text(const_name):
    m = re.search(rf'{const_name}[^=]*=\s*\w+\s*{{(.*?)}};', src, re.S)
    im = re.search(r'imperative:\s*"((?:[^"\\]|\\.)*)"', m.group(1), re.S)
    return im.group(1).encode().decode('unicode_escape')


KEPT = ['ONE_SCHEMA_PER_REQUEST', 'FIELDS_FROM_REQUEST_ONLY', 'ENUM_FORMAT',
        'TARGET_TYPE_MUST_EXIST', 'RELATIONSHIP_VS_FIELD']

ORIGINAL_EXAMPLE = '(e.g. asked for "ADR", do not also create "Ticket" or "Sprint")'
GENERIC_EXAMPLE = '(e.g. asked for one type, do not also create the types it references)'

VARIANTS = {
    'adr-example-generic': lambda t: t.replace(ORIGINAL_EXAMPLE, GENERIC_EXAMPLE),
    'adr-example-removed': lambda t: t.replace(' ' + ORIGINAL_EXAMPLE, ''),
}

BASE = 'packages/agent/goldens/dev-schema-creation.toml'
base_src = open(BASE).read()
RULE_START = re.compile(r'^[A-Z][A-Z ,\'-]{3,}:|^CALL [a-z_]+ NOW')


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, (m.group(1) if m else None)


def swap(system, rules_text):
    ref = system.find('--- Candidate 1:')
    headp, block = system[:ref], system[ref:]
    out, inserted = [], False
    for p in block.split('\n\n'):
        flat = p.strip().replace('\\\n', '').replace('\n', ' ')
        if RULE_START.match(flat):
            if not inserted:
                out.append(rules_text)
                inserted = True
            continue
        out.append(p)
    return headp + '\n\n'.join(out)


NOTE = '''
NAME-CONTAMINATION PROBE -- {variant}

prod-guidance-trimmed-v3 returned name "ADR" 3/3 where both the corpus base and
the full production guidance return "Architecture Decision Record".

The user's message never contains "ADR". It says "We need to start tracking
architecture decision records". The only occurrence of the token in the entire
prompt is inside ONE_SCHEMA_PER_REQUEST's own worked example:

    Do NOT proactively invent or create related types
    (e.g. asked for "ADR", do not also create "Ticket" or "Sprint")

That example teaches "do not create EXTRA types". It is not about naming. But it
supplies a short name for the very concept the user is asking about, and the
model appears to adopt it.

{detail}

WHY IT MATTERS BEYOND THIS CASE

This is contamination in the opposite direction from the one #1932 guards. That
guard exists so eval prompts do not share text with guidance, making a pass
prove memorisation. Here the guidance supplies an incidental detail -- a name --
that the model copies into output on an unrelated axis.

If confirmed, worked examples need the same discipline as eval prompts: their
incidental details are not inert, and a name chosen for illustration can become
the answer.

REFERENCE RESULTS (3 reps each, byte-identical within arm):
  corpus base                    name "Architecture Decision Record", supersedes as text field
  production full (10 rules)     name correct, supersedes -> relationship w/ INVENTED targetType
  trimmed v2 (no REL rule)       name correct, allowed_values correct, supersedes ABSENT
  trimmed v3 (REL restored)      name "ADR", allowed_values correct, supersedes -> targetType "adr"
'''

for variant, transform in VARIANTS.items():
    parts = []
    for n in KEPT:
        t = rule_text(n)
        if n == 'ONE_SCHEMA_PER_REQUEST':
            t = transform(t)
        parts.append(t)
    rules_text = '\n\n'.join(parts)

    starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', base_src)]
    head = base_src[: starts[0]]
    turns = [base_src[starts[i]: (starts[i + 1] if i + 1 < len(starts) else len(base_src))]
             for i in range(len(starts))]
    rebuilt = []
    for t in turns:
        m, system = system_of(t)
        if system is None:
            rebuilt.append(t)
            continue
        rebuilt.append(t[: m.start(1)] + swap(system, rules_text) + t[m.end(1):])

    detail = ('The example is kept but its type names replaced with neutral wording, so the '
              'rule still teaches "do not create extra types" while supplying no candidate name.'
              if variant == 'adr-example-generic' else
              'The parenthetical example is deleted outright. Controls for the possibility that '
              'the presence of any example, rather than its content, is what changes the name.')

    name = f'prod-guidance-{variant}'
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""',
                 'notes = """\n' + NOTE.format(variant=variant, detail=detail).strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = '# derived-from: dev-schema-creation.toml, production guidance, example varied.\n' + out
    out += ''.join(rebuilt)
    path = f'packages/agent/goldens/ablation/{name}.toml'
    open(path, 'w').write(out)
    print(f'wrote {path}')
