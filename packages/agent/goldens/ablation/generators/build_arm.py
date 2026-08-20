"""Generate a production-fidelity ablation arm for dev-status-change-enum.

The corpus hand-authors 1-2 minimal tools per case; production Stage 2 sends 9
with full parameter schemas (~15KB). Every corpus measurement therefore ran on
a prompt at ~11% of production size and never exercised tool SELECTION -- with
one tool on offer there is no wrong answer available.

This rewrites the case's two turns with the real 9-tool surface, taken from
`model_facing_tool_definitions()` (the same accessor production and the #2119
gate use) scoped to the union of the three whitelists that actually fired on a
live turn: Node Creation, Graph Editing, Relationship Management.

Everything else -- system prose, candidates, schemas, user messages, expect,
tool_results -- is copied byte-for-byte from the base case, so this arm differs
from it in exactly one dimension: the tool surface.
"""
import json
import re

BASE = 'packages/agent/goldens/dev-status-change-enum.toml'
OUT = 'packages/agent/goldens/ablation/full-tool-surface.toml'

# The union `stage2_tools` produced on the captured live turn. Node Creation +
# Graph Editing + Relationship Management whitelists, deduped, in the order the
# capture rendered them.
LIVE_SURFACE = [
    'search_nodes',
    'search_semantic',
    'get_node',
    'create_node',
    'update_node',
    'update_task_status',
    'create_relationship',
    'get_related_nodes',
    'route_clarify',
]

defs = {t['name']: t for t in json.load(open('/tmp/tool_defs.json'))}
missing = [n for n in LIVE_SURFACE if n not in defs]
assert not missing, f'missing from model_facing_tool_definitions(): {missing}'


def esc_basic(s: str) -> str:
    """Escape a Python str for a TOML basic (single-line) string."""
    return s.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n')


def toml_inline(v) -> str:
    """Render a JSON value as a TOML inline value.

    `parameters_schema` is a `toml::Value` in the case format, not a string --
    emitting it as a multi-line string silently yields `parameters:{}` at the
    template boundary, which is how the first draft of this arm rendered every
    tool at roughly half its production size while still parsing cleanly.
    """
    if isinstance(v, dict):
        return '{ ' + ', '.join(f'{toml_key(k)} = {toml_inline(x)}' for k, x in v.items()) + ' }'
    if isinstance(v, list):
        return '[' + ', '.join(toml_inline(x) for x in v) + ']'
    if isinstance(v, bool):
        return 'true' if v else 'false'
    if isinstance(v, (int, float)):
        return str(v)
    return '"' + esc_basic(str(v)) + '"'


def toml_key(k: str) -> str:
    return k if re.fullmatch(r'[A-Za-z0-9_-]+', k) else '"' + esc_basic(k) + '"'


def render_tool(name: str, indent: str = '  ') -> str:
    d = defs[name]
    lines = [
        f'{indent}[[turn.tool]]',
        f'{indent}name = "{name}"',
        f'{indent}description = "{esc_basic(d["description"])}"',
        f'{indent}parameters_schema = {toml_inline(d["parameters_schema"])}',
    ]
    return '\n'.join(lines)


src = open(BASE).read()

# Split into the two [[turn]] blocks, then strip each turn's existing
# hand-authored [[turn.tool]] entries and append the real surface.
turn_starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
assert len(turn_starts) == 2, f'expected 2 turns, found {len(turn_starts)}'

head = src[: turn_starts[0]]
turns = [src[turn_starts[0]:turn_starts[1]], src[turn_starts[1]:]]

rebuilt = []
for t in turns:
    # Everything before the first [[turn.tool]] is kept verbatim.
    cut = t.find('  [[turn.tool]]')
    assert cut > 0, 'turn has no [[turn.tool]] block'
    kept = t[:cut].rstrip() + '\n\n'
    surface = '\n\n'.join(render_tool(n) for n in LIVE_SURFACE)
    rebuilt.append(kept + surface + '\n')

notes = '''
ABLATION ARM -- production tool-surface fidelity. Built from
dev-status-change-enum.toml, differing in exactly one dimension: the TOOL
SURFACE.

WHY THIS ARM EXISTS

Every case in the parent directory hand-authors the one or two tools its turn
needs. Production Stage 2 sends the union of the retrieved candidates'
whitelists -- 9 tools with full parameter schemas -- because the read tools
(search_nodes, search_semantic, get_node) appear in 5-6 of the 8 seeded
skills, so any three candidates drag most of the registry in.

Measured on a live daemon turn via NODESPACE_PROMPT_DUMP:

  golden turn 1     2,679 chars    1 tool declaration
  real production  24,636 chars    9 tool declarations

The corpus was measuring 11% of the prompt production sends, and the missing
15,373 chars are entirely tool declarations.

WHAT THAT INVALIDATES

Handing the model one tool and asking it to call that tool does not test tool
selection -- no wrong answer is available. So the corpus measured argument
FORMATION (which values reach field_values, whether an enum member is exact)
but never tool SELECTION, and any arm whose result depended on selection is
uninformative.

That specifically includes ablation/no-candidate-block.toml and
ablation/no-instruction-subtree.toml, both of which concluded the Stage-2
candidate block is inert. Disambiguating among nine competing tools is the
block's actual job, and it was absent from what those arms measured. Their
recorded results stand as what they measured; they do not transfer to
production.

The declared-field pattern (../PATTERN.toml) is not affected the same way --
it addresses which KEY a value lands under, not which tool is chosen, and its
eight refuted prose arms are still refuted.

HOW THIS ARM IS BUILT

Tool definitions come from `model_facing_tool_definitions()` -- the same
accessor agent_loop.rs and the #2119 snapshot gate call -- dumped via
`cargo run -p nodespace-agent --bin dump_tool_defs`, not transcribed. The nine
are the union that actually fired on the captured turn (Node Creation + Graph
Editing + Relationship Management).

Everything else is byte-identical to the base: system prose, candidate block,
EXISTING SCHEMAS, user messages, tool_results, expect.

HOW TO READ IT

Base result to compare against: 5/5, turn 1 sending node_type "ticket" with a
status filter of the exact enum member "in_dev", turn 2 sending update_node
with the real uuid from turn 1 plus field_values={"status":
"ready_for_review"}.

A PASS here means the prompt work holds when the model must actually choose
among nine tools, and the corpus's conclusions about argument formation carry
over. Cutting the surface then becomes an optimisation question.

A FAIL is the more important result: it means the corpus has been validating
against a prompt production never sends, and the case set needs rebuilding at
real fidelity before any of its findings are relied on.

A difference inside one rep is noise. Identical code has scored 6/7/7 on the
agent matrix.
'''.strip()

out = head.replace('name = "dev-status-change-enum"', 'name = "full-tool-surface"', 1)
out = re.sub(r'(?s)^notes = """.*?"""', 'notes = """\n' + notes + '\n"""', out, count=1, flags=re.M)
out = '# derived-from: dev-status-change-enum.toml -- the base this arm rebuilds.\n' + out
out += '\n'.join(rebuilt)

open(OUT, 'w').write(out)
print(f'wrote {OUT}  ({len(out):,} chars)')
print(f'tools per turn: {len(LIVE_SURFACE)}')
