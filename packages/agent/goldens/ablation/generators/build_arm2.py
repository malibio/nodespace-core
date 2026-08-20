"""Generate the honest-baseline arm: real 9-tool surface AND the real 3-candidate block.

full-tool-surface.toml replaced the tool surface but kept the base case's single
hand-authored candidate, so it reached 7,233 chars against production's 24,636.
This arm additionally substitutes the real Stage-2 candidate block captured from
a live turn -- three competing instruction subtrees, one of which (Relationship
Management) is actively pointing at a tool the request does not want.

That combination is what production actually sends, and it is the condition under
which the "candidate block is inert" finding needs re-testing: inertness was
measured with neither competing tools nor competing candidates present.
"""
import json
import re

BASE = 'packages/agent/goldens/ablation/full-tool-surface.toml'
OUT = 'packages/agent/goldens/ablation/production-baseline.toml'

candidates = open('/tmp/real_candidates.txt').read().strip()

src = open(BASE).read()

# Each turn's `system` is a TOML multi-line basic string. Replace the block that
# starts at "REFERENCE" and runs to the closing delimiter, keeping the resident
# head and the trailing EXISTING SCHEMAS the base case already carries.
turn_starts = [m.start() for m in re.finditer(r'(?m)^\[\[turn\]\]', src)]
assert len(turn_starts) == 2, f'expected 2 turns, got {len(turn_starts)}'

head = src[: turn_starts[0]]
turns = [src[turn_starts[0]:turn_starts[1]], src[turn_starts[1]:]]


def swap_candidates(turn_text: str) -> str:
    m = re.search(r'(?s)system = """\n(.*?)\n"""', turn_text)
    assert m, 'turn has no system block'
    system = m.group(1)
    ref = system.find('REFERENCE')
    assert ref > 0, 'system block has no REFERENCE section'
    # Keep the resident head verbatim; substitute the real candidate block.
    # TOML basic multi-line: a literal backslash must stay escaped.
    new_system = system[:ref] + candidates.replace('\\', '\\\\')
    return turn_text[: m.start(1)] + new_system + turn_text[m.end(1):]


rebuilt = [swap_candidates(t) for t in turns]

notes = '''
ABLATION ARM -- the honest production baseline. Real 9-tool surface AND the real
three-candidate Stage-2 block, both captured from a live daemon turn.

WHY THIS ARM EXISTS

full-tool-surface.toml fixed the tool surface but kept the base case's single
hand-authored candidate, reaching 7,233 chars against production's 24,636. The
remaining gap was the candidate block: the base carries one clean candidate
(~1KB), production sent three (~6KB) whose guidance actively competes -- one of
them (Relationship Management) instructs the model to call create_relationship
on a turn that wants an update.

This arm substitutes that real block, so the prompt matches what production
sends on both axes that were wrong.

WHAT IT TESTS

The condition under which ablation/no-candidate-block.toml and
ablation/no-instruction-subtree.toml concluded the Stage-2 candidate block is
inert. That conclusion was measured with a single candidate and a single tool
on offer -- neither competing tools nor competing procedures. Disambiguating
among several of both is the block's actual job, so its inertness has never
been tested where it would matter.

Compare against full-tool-surface.toml (5/5, 9 tools, one candidate) to isolate
what the extra two candidates cost or buy. Compare against the base
dev-status-change-enum.toml (5/5, one tool, one candidate) for the full delta.

HOW IT IS BUILT

Candidate block: verbatim from a NODESPACE_PROMPT_DUMP capture of a live turn
(Node Creation + Graph Editing + Relationship Management, in the order retrieval
returned them). Tool surface: inherited from full-tool-surface.toml, itself
built from `model_facing_tool_definitions()` via
`cargo run -p nodespace-agent --bin dump_tool_defs`.

The resident head, EXISTING SCHEMAS, user messages, tool_results, and expect are
unchanged from the base.

HOW TO READ IT

Expected on both turns, matching every prior arm: turn 1 search_nodes with
node_type ticket and the exact enum member "in_dev"; turn 2 update_node with the
uuid from turn 1's tool result plus field_values={"status":
"ready_for_review"}.

A PASS means the prompt work holds at real production fidelity, and the corpus's
argument-formation findings carry over even though its selection findings were
untested.

A FAIL is the more valuable result: it locates the failure in the condition the
corpus never reproduced, and tells us the case set has to be rebuilt at fidelity
before its conclusions are relied on.

A difference inside one rep is noise -- identical code has scored 6/7/7 on the
agent matrix.
'''.strip()

out = head.replace('name = "full-tool-surface"', 'name = "production-baseline"', 1)
out = re.sub(r'(?s)^notes = """.*?"""', 'notes = """\n' + notes + '\n"""', out, count=1, flags=re.M)
out = out.replace(
    '# derived-from: dev-status-change-enum.toml -- the base this arm rebuilds.',
    '# derived-from: full-tool-surface.toml, itself derived from dev-status-change-enum.toml.',
    1,
)
out += ''.join(rebuilt)

open(OUT, 'w').write(out)
print(f'wrote {OUT} ({len(out):,} chars)')
