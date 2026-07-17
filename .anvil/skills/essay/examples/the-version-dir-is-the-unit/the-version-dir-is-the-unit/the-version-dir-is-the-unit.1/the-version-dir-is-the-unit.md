# The version dir is the unit

The first time I let an agent revise my own writing, I made the mistake every
file-oriented programmer makes. I gave it the file and told it to improve the
file. It did. The improved file overwrote the old one, the way an edit
always does, and three passes later I had a draft I did not like and no way
to see the one I had abandoned two steps back. The history was a single point
that kept moving. I could not stand on it.

So I stopped editing files and started writing directories.

In anvil, the unit of work is not the document. It is the version directory:
a folder named for the draft and its number — `the-version-dir-is-the-unit.1`,
`.2`, `.3` — that holds the body and a small `_progress.json` recording what
phase it reached. The rule that makes the whole thing work is one sentence
long. Once a version's `_progress.json` says `done`, that directory never
changes again. Revision does not edit it. Revision produces the next
directory.

This sounds like overhead until you have lived without it. The file model
gives you exactly one present and no past you can trust. You can lean on git
for the past, and I do, but a commit log is a record of *changes* — to
reconstruct what version three actually looked like you replay diffs in your
head, and the thing you are reconstructing is precisely the thing you wanted
to compare against. The version directory inverts that. The past is not a
diff to replay. It is a folder you can open.

The payoff shows up the moment a reviewer is involved. A critic in anvil does
not mark up your draft; it writes its own sibling directory next to the one
it reviewed — `the-version-dir-is-the-unit.1.review` next to
`the-version-dir-is-the-unit.1`. The review is immutable too. It scores a
specific, frozen draft, and because that draft can never move, the score
never goes stale. When the next version lands, it gets its own reviewer
sibling. Nothing is overwritten, so nothing is lost, so the question "did
this actually get better" has a real answer instead of a feeling.

That answer matters more than it looks, because the loop has a failure mode
the rubric hides. A draft can climb from thirty-one to thirty-four to
thirty-six, clear the threshold, advance — and be the same draft with the
same flaw, now wearing three rounds of polish over the place it was always
wrong. The total went up. The thing that was broken did not move. You only
catch this by setting two frozen reviews side by side and seeing that the
number rose while the flaw sat still. You cannot do that when each review
clobbered the last.

There is a quieter benefit, and it is the one I underrate every time. State
on disk is the only state I trust. An agent that tells me it revised the
draft is making a claim. A directory named `.4` that did not exist a minute
ago is a fact. The whole loop — what phase a thread is in, whether it is
ready to advance, whether it stalled — is read off the filesystem, not off
anything the agent reports about itself. The orchestration never has to ask
the worker how far it got. It looks.

People reach for the committee reflex here, and I understand the pull. If one
reviewer let a bad number through, surely two would catch it. But two
reviewers scoring the same rubric do not give you two independent reads. They
give you two reads that mostly agree plus a thin band of disagreement, and
the disagreement is where your time goes now, adjudicating a tie neither
critic cared about. The version directory does not need a committee to be
trustworthy. It needs to be immutable. An immutable artifact reviewed once is
worth more than a mutable one reviewed three times, because the single review
still describes the thing in front of you. The triple review describes a
moving target.

None of this is exotic. It is the instinct that makes git commits worth
having, applied one level up — to the draft instead of the diff, to the
folder instead of the line. Programmers already know that a thing you can
return to is worth more than a thing you can only edit. We just forgot to
extend the courtesy to prose.

The file moves under you. The directory stays put. Build on the one that
stays.
