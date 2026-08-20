"""Test the KEEP/DROP rule against PRODUCTION's real Schema Creation guidance.

Everything measured so far used the corpus's hand-authored guidance. Production's
is different: composed from named constants in skill_rules.rs, ~9 rules, and
materially longer. Applying the rule to source without measuring it there first
would repeat exactly the mistake this whole investigation exists to correct.

KEEP/DROP applied to production's nine rules, with the reason for each:

  KEEP  ONE_SCHEMA_PER_REQUEST   ontological: one type per request, and a detail
                                 pointing at another record is a field. This is
                                 the rule the corpus measured as load-bearing
                                 (dropping it lost `supersedes` 3/3).
  KEEP  RELATIONSHIP_VS_FIELD    ontological: when a reference becomes a
                                 relationship rather than a field. No schema
                                 states it.
  KEEP  FIELDS_FROM_REQUEST_ONLY             acts on something ABSENT: derive fields from the
                                 user's request, never copy from EXISTING
                                 SCHEMAS. A schema cannot say where fields come
                                 from.
  KEEP  TARGET_TYPE_MUST_EXIST   acts on absence: omit a relationship whose
                                 target is not in EXISTING SCHEMAS.

  DROP  SCHEMA_ALREADY_EXISTS    "success means stop" -- restates the tool result.
  DROP  SCHEMA_VALIDATION_RETRY  error handling the tool result already conveys.
  DROP  EDIT_DONT_RECREATE       update_schema's own description says this.
  DROP  RENAME_VS_RELABEL        the from/to/friendlyName shape IS the schema's;
                                 the rule even says "see the tool schema".
  DROP  TITLE_TEMPLATE_PLACEHOLDERS  title_template is declared in the schema.
  DROP  UNIQUE_FIELD_FLAGS       unique / unique_case_insensitive are declared in
                                 the schema.

Both arms use the corpus's dev-schema-creation case (which has a measured base)
but swap in production's actual guidance text, so the comparison is
production-prose vs production-prose-trimmed on a case whose correct answer is
known.
"""
import re
import subprocess

# Pull the real rule text out of skill_rules.rs so this cannot drift from source.
src = open('packages/agent/src/skill_rules.rs').read()


def rule_text(const_name: str) -> str:
    m = re.search(rf'{const_name}[^=]*=\s*\w+\s*{{(.*?)}};', src, re.S)
    if not m:
        raise SystemExit(f'rule {const_name} not found')
    im = re.search(r'imperative:\s*"((?:[^"\\]|\\.)*)"', m.group(1), re.S)
    if not im:
        raise SystemExit(f'imperative not found for {const_name}')
    return im.group(1).encode().decode('unicode_escape')


FULL = [
    'ONE_SCHEMA_PER_REQUEST', 'SCHEMA_ALREADY_EXISTS', 'SCHEMA_VALIDATION_ERROR_RETRY',
    'EDIT_DONT_RECREATE', 'RENAME_VS_RELABEL', 'FIELDS_FROM_REQUEST_ONLY', 'RELATIONSHIP_VS_FIELD',
    'TARGET_TYPE_MUST_EXIST', 'TITLE_TEMPLATE_PLACEHOLDERS', 'UNIQUE_FIELD_FLAGS',
]
KEPT = ['ONE_SCHEMA_PER_REQUEST', 'FIELDS_FROM_REQUEST_ONLY', 'ENUM_FORMAT',
        'TARGET_TYPE_MUST_EXIST', 'RELATIONSHIP_VS_FIELD']

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
PRODUCTION GUIDANCE -- {variant}

Every prior arm used the corpus's hand-authored guidance. This one swaps in the
REAL text from packages/agent/src/skill_rules.rs, extracted at generation time so
it cannot drift from source, onto a case whose correct answer is already measured
(dev-schema-creation: create_schema "Architecture Decision Record" with BOTH
status (enum, [proposed, accepted]) and supersedes (text), 3/3).

{detail}

WHY MEASURE PRODUCTION TEXT SEPARATELY

The KEEP/DROP rule was derived and validated on the corpus's guidance, which is
shorter and differently worded than production's. Applying it to source without
testing it there would repeat the exact error this investigation exists to
correct -- a conclusion measured on one prompt and applied to a different one.

KEEP/DROP as applied here:
  KEEP  ontological distinctions, and rules that act on something ABSENT from the
        declared surface (derive fields from the request not from EXISTING
        SCHEMAS; omit a relationship whose target does not exist).
  DROP  anything a parameter schema already carries -- title_template, unique
        flags, rename_fields' from/to/friendlyName shape, update_schema's
        purpose, and "success means stop" restatements of the tool result.
'''

for variant, names in [('trimmed-v3', KEPT)]:
    rules_text = '\n\n'.join(rule_text(n) for n in names)
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

    detail = (f'Contains all {len(names)} production rules, {len(rules_text):,} chars.'
              if variant == 'full' else
              f'Contains {len(names)} of 10 production rules ({len(rules_text):,} chars): '
              + ', '.join(names))

    name = f'prod-guidance-{variant}'
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""',
                 'notes = """\n' + NOTE.format(variant=variant, detail=detail).strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = '# derived-from: dev-schema-creation.toml, guidance swapped for production text.\n' + out
    out += ''.join(rebuilt)
    path = f'packages/agent/goldens/ablation/{name}.toml'
    open(path, 'w').write(out)
    print(f'wrote {path}  rules={len(names)} guidance={len(rules_text):,} chars')
