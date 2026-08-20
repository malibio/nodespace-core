"""Production guidance paired with production's REAL tool schemas.

WHY THIS SUPERSEDES THE prod-guidance-* ARMS

Those arms spliced production's guidance text into dev-schema-creation.toml,
whose create_schema tool is HAND-AUTHORED and differs from production's. The
mismatch was not cosmetic:

  corpus tool schema    enum values under `allowed_values`, no required-ness note
  production schema     enum values under `coreValues`, declared "REQUIRED and
                        must be non-empty when type=enum", with the lowercase
                        rule stated inline

So when prod-guidance-trimmed-v2 dropped ENUM_FORMAT and the enum came back
empty, that was read as "ENUM_FORMAT is load-bearing". It is not: production's
schema already carries the rule under a different key name. The arm measured
production prose against a schema that lacks what production's has.

skill_pipeline.rs's own doc comment says exactly this -- NO_NAME_TITLE_FIELD,
NAME_PLACEHOLDER_EXCEPTION, FIELDS_FROM_REQUEST_ONLY and ENUM_FORMAT were
removed from the local-agent path because create_schema's schema states them --
and it was right. The contradicting measurement came from a mispaired fixture,
found by grepping for `allowed_values` (the corpus key) instead of `coreValues`
(production's).

This generator pairs both halves from source so the same class of error cannot
recur:

  guidance      read from packages/agent/src/skill_rules.rs at generation time
  tool schemas  read from `dump_tool_defs` output, i.e.
                model_facing_tool_definitions(), the same accessor production
                and the #2119 gate use

Arms:
  paired-current  production's guidance exactly as skill_pipeline.rs composes it
                  today, with production's tool schemas. The honest reference --
                  no prior arm has measured this pairing.
  paired-trimmed  the same, minus the rules that fail the KEEP test against
                  production's ACTUAL schemas (checked by key, not by memory).
"""
import json
import re

REPO_RULES = 'packages/agent/src/skill_rules.rs'
TOOL_DEFS = '/tmp/tool_defs_fixed.json'
BASE = 'packages/agent/goldens/dev-schema-creation.toml'

src = open(REPO_RULES).read()
defs = {t['name']: t for t in json.load(open(TOOL_DEFS))}


def rule_text(const_name):
    m = re.search(rf'{const_name}[^=]*=\s*\w+\s*{{(.*?)}};', src, re.S)
    if not m:
        raise SystemExit(f'rule {const_name} not found in {REPO_RULES}')
    im = re.search(r'imperative:\s*"((?:[^"\\]|\\.)*)"', m.group(1), re.S)
    return im.group(1).encode().decode('unicode_escape')


# Exactly what schema_creation_guidance() interpolates today, in order.
CURRENT = [
    'ONE_SCHEMA_PER_REQUEST', 'SCHEMA_ALREADY_EXISTS', 'SCHEMA_VALIDATION_ERROR_RETRY',
    'EDIT_DONT_RECREATE', 'RENAME_VS_RELABEL', 'RELATIONSHIP_VS_FIELD',
    'TARGET_TYPE_MUST_EXIST', 'TITLE_TEMPLATE_PLACEHOLDERS', 'UNIQUE_FIELD_FLAGS',
]

# KEEP/DROP re-applied against production's real create_schema/update_schema
# schemas. Each DROP names the schema key that already carries the rule, so the
# claim is checkable rather than asserted.
KEEP = ['ONE_SCHEMA_PER_REQUEST', 'SCHEMA_ALREADY_EXISTS', 'SCHEMA_VALIDATION_ERROR_RETRY',
        'RELATIONSHIP_VS_FIELD', 'TARGET_TYPE_MUST_EXIST']

DROP_REASONS = {
    'EDIT_DONT_RECREATE': "update_schema's own description states its purpose",
    'RENAME_VS_RELABEL': "rename_fields' from/to/friendlyName shape is in update_schema",
    'TITLE_TEMPLATE_PLACEHOLDERS': "title_template is declared in create_schema",
    'UNIQUE_FIELD_FLAGS': "unique / unique_case_insensitive are declared in create_schema",
}

# Verify every DROP claim against the actual schemas before generating anything.
both = json.dumps(defs['create_schema']) + json.dumps(defs['update_schema'])
CHECKS = {
    'EDIT_DONT_RECREATE': 'add_fields',
    'RENAME_VS_RELABEL': 'friendlyName',
    'TITLE_TEMPLATE_PLACEHOLDERS': 'title_template',
    'UNIQUE_FIELD_FLAGS': 'unique_case_insensitive',
}
for rule, key in CHECKS.items():
    present = key in both
    print(f'  DROP {rule:30s} key "{key}" in schema: {present}')
    if not present:
        raise SystemExit(f'ABORT: {rule} dropped on the claim that "{key}" is in the schema, '
                         f'but it is not. This is the mispairing error again.')
print(f'  KEEP ENUM_FORMAT? already absent from the local path; "coreValues" in schema: '
      f'{"coreValues" in both}')

RULE_START = re.compile(r'^[A-Z][A-Z ,\'-]{3,}:|^CALL [a-z_]+ NOW')


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


def render_tool(name):
    d = defs[name]
    return '\n'.join([
        '  [[turn.tool]]',
        f'  name = "{name}"',
        f'  description = "{esc(d["description"])}"',
        f'  parameters_schema = {toml_inline(d["parameters_schema"])}',
    ])


# Schema Creation's whitelist, which is what stage2_tools would scope to.
SURFACE = ['create_schema', 'update_schema', 'get_node', 'create_node']

base_src = open(BASE).read()
starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', base_src)]
head = base_src[: starts[0]]
turns = [base_src[starts[i]: (starts[i + 1] if i + 1 < len(starts) else len(base_src))]
         for i in range(len(starts))]


def system_of(t):
    m = re.search(r'(?s)system = """\n(.*?)\n"""', t)
    return m, (m.group(1) if m else None)


def swap_rules(system, rules_text):
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
PRODUCTION GUIDANCE + PRODUCTION TOOL SCHEMAS -- {variant}

Both halves read from source at generation time:
  guidance      packages/agent/src/skill_rules.rs
  tool schemas  model_facing_tool_definitions(), via `dump_tool_defs`

WHY THIS REPLACES THE prod-guidance-* ARMS

Those spliced production guidance into this case's HAND-AUTHORED tool schemas,
which differ from production's in a way that mattered: the corpus declares enum
values under `allowed_values`, production under `coreValues` -- declared
"REQUIRED and must be non-empty when type=enum" with the lowercase rule inline.

Dropping ENUM_FORMAT in that mispaired arm produced an empty enum, which was
read as "ENUM_FORMAT is load-bearing". It is not. skill_pipeline.rs's doc
comment already says create_schema's schema carries it, and that comment is
correct. The contradicting result came from grepping the corpus's key name
instead of production's.

{detail}

Every DROP here names the schema key that carries the rule, and the generator
ABORTS if that key is absent -- so this class of error cannot recur silently.

TOOL SURFACE: {surface} -- Schema Creation's whitelist, what stage2_tools would
scope to for this turn.

EXPECTED (from the corpus base, 3/3): create_schema, exactly once, name in the
user's own words ("architecture decision records", never abbreviated in the
request), a status field of type enum carrying its values, and the supersedes
detail captured rather than dropped.

Note the case's own tool_results and expect still speak of `allowed_values`;
production's key is `coreValues`. Read the enum assertion by intent, not by key
name, until the corpus is re-authored against production schemas.
'''

BISECT = {
    'add-edit': KEEP + ['EDIT_DONT_RECREATE'],
    'add-rename': KEEP + ['RENAME_VS_RELABEL'],
    'add-titletpl': KEEP + ['TITLE_TEMPLATE_PLACEHOLDERS'],
    'add-unique': KEEP + ['UNIQUE_FIELD_FLAGS'],
}
VARIANTS = [('paired-current', CURRENT), ('paired-trimmed', KEEP)] + \
           [(f'paired-{k}', v) for k, v in BISECT.items()]

for variant, names in VARIANTS:
    rules_text = '\n\n'.join(rule_text(n) for n in names)
    rebuilt = []
    for t in turns:
        m, system = system_of(t)
        if system is None:
            rebuilt.append(t)
            continue
        t2 = t[: m.start(1)] + swap_rules(system, rules_text) + t[m.end(1):]
        cut = t2.find('  [[turn.tool]]')
        kept = t2[:cut].rstrip() + '\n\n'
        t2 = kept + '\n\n'.join(render_tool(n) for n in SURFACE) + '\n'
        rebuilt.append(t2)

    if variant == 'paired-current':
        detail = (f'Contains all {len(names)} rules schema_creation_guidance() interpolates '
                  f'today ({len(rules_text):,} chars). No prior arm has measured production '
                  f'guidance against production schemas -- this is the honest reference.')
    else:
        dropped = '\n'.join(f'  DROP {r}: {why}' for r, why in DROP_REASONS.items())
        detail = (f'Contains {len(names)} of {len(CURRENT)} rules ({len(rules_text):,} chars).\n'
                  f'Dropped, each with the schema key that carries it:\n{dropped}')

    name = f'prod-{variant}'
    out = re.sub(r'(?m)^name = ".*?"$', f'name = "{name}"', head, count=1)
    out = re.sub(r'(?s)^notes = """.*?"""',
                 'notes = """\n' + NOTE.format(variant=variant, detail=detail,
                                               surface=', '.join(SURFACE)).strip() + '\n"""',
                 out, count=1, flags=re.M)
    out = '# derived-from: dev-schema-creation.toml, with BOTH guidance and tool schemas from source.\n' + out
    out += ''.join(rebuilt)
    path = f'packages/agent/goldens/ablation/{name}.toml'
    open(path, 'w').write(out)
    print(f'wrote {path}  rules={len(names)} guidance={len(rules_text):,} chars')
