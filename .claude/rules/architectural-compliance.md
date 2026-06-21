# Architectural Compliance — Zero Tolerance

## Rule

When the user or a prompt specifies a dependency, library, crate, repo,
or architectural path, USE THAT EXACT THING. Do not substitute.

## What Happened

User said: "use lance-graph as hot path"
Session did: replaced lance-graph with neo4j-rs because "the stub is only 123 lines"

The session looked at `crates/stubs/lance-graph/` (an empty placeholder), concluded
lance-graph doesn't exist, and substituted neo4j-rs. The REAL lance-graph
(`AdaWorldAPI/lance-graph`) has 70+ source files, a Cypher parser, a DataFusion
planner, blasgraph columnar storage, and semiring algebra. The stub was a stub.

## Why This Is Catastrophic

1. The user's architectural decisions encode months of context the session doesn't have
2. "It doesn't exist yet" is not the same as "it shouldn't be used"
3. Swapping engines during build corrupts every downstream integration
4. The user now has to debug WHY their system doesn't work the way they designed it
5. Other sessions that read the code inherit the wrong architecture
6. The real crate might have been the ENTIRE POINT of the product

## What To Do Instead

If a specified dependency doesn't compile, is empty, or seems wrong:

1. STOP
2. Say: "The specified dependency [X] appears to be a stub / doesn't compile / 
   is missing feature Y. The real crate may be at [repo URL]. Should I:
   (a) Wire the real crate from the git repo?
   (b) Build what's needed in the stub?
   (c) Something else?"
3. WAIT for the answer
4. Do NOT substitute a different library
5. Do NOT say "I found a better approach"

## Severity

Substituting a specified architectural component without asking is a P0 violation.
Same severity as deleting production code or reverting without authorization.

## Detection Patterns — Flag These Immediately

- "I noticed [X] is only a stub, so I used [Y] instead"
- "Since [X] doesn't have feature Z, I replaced it with [W]"
- "[X] seemed too simple for this task, so I chose [Y]"
- "I found that [X] is just a placeholder, here's a better approach"
- Any commit that adds a NEW dependency not specified in the prompt
- Any commit that removes or feature-gates a SPECIFIED dependency

## The Test

Before committing: does the code use EXACTLY the dependencies the user specified?
If no → stop and ask. There are no exceptions.
