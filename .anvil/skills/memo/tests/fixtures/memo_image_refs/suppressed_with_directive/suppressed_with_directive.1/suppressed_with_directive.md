# Memo with a suppression directive on a missing ref

The memo references a figure that the drafter will generate later via
`memo-figures`. We suppress the lint with a same-line directive:

![Cohort valuations](exhibits/fig_a.png) <!-- anvil-lint-disable: memo_image_refs_exist -->

Body continues here.
